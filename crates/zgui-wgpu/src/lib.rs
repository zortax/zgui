//! Filling `surface` elements with textures another renderer produced.
//!
//! A `surface` element is a replaced box whose pixels come from outside the framework entirely: a
//! game rendering at its own cadence, a video decoder on its own thread, a one-shot rasteriser.
//! This crate is the wgpu half of that story — the runtime's [`EmbedHost`] seam says *when* embed
//! work may touch a frame, and this crate is what does the touching, because handing a texture to
//! a renderer is an operation only the backend that owns the device can define.
//!
//! # The two ways in
//!
//! **A producer with its own cadence brings its own texture.** It holds a [`SurfaceHandle`],
//! creates textures on zgui's device — delivered through [`SurfaceEvent::Attached`] — and calls
//! [`SurfaceHandle::present`] whenever a frame is ready. Present is a latest-wins mailbox: the
//! next zgui frame shows the newest texture and earlier ones were simply never shown. This is the
//! shape for a game or a video player.
//!
//! **A producer that draws on demand implements [`SurfaceRenderer`].** zgui owns the texture,
//! sizes it to the element's content box, and calls
//! [`render`](SurfaceRenderer::render) on the frames where drawing is owed: the first one, a
//! resize, a device loss, or every refresh while the renderer keeps asking through
//! [`SurfaceRenderCx::request_animation_frame`]. This is the shape for a pdf page or a spinning
//! part preview.
//!
//! # Texture lifetime, in one paragraph
//!
//! Attaching holds a view of the presented texture until the *next* attach replaces it, and wgpu
//! keeps the resource alive under the view. A producer that reuses textures therefore round-robins
//! at least two — three to be comfortable — or gates reuse on
//! `Queue::on_submitted_work_done`. There is no intra-texture tearing to worry about: zgui and the
//! producer share one device queue, so writes submitted before a present are ordered before the
//! frame that samples them.

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::NodeKey;
use zgui_dom::host::replaced::{Intrinsic, ReplacedId};
use zgui_geom::{Css, CssPx, Device, Size};
use zgui_reactive::FrameWaker;
use zgui_render::{ExternalTexture, TextureHandle};
use zgui_render_wgpu::Gpu;
use zgui_render_wgpu::WgpuRenderer;
use zgui_runtime::embed::{
    EmbedHost, EmbedMaintenanceCx, EmbedMemoryReport, EmbedSyncCx, EmbedSyncReport,
};
use zgui_runtime::wake::RuntimeWaker;
use zgui_scene::ExternalTextureId;
use zgui_vocab::{PropKey, PropValue, Timestamp};

/// The re-exported graphics API, so a producer names the same wgpu the renderer links.
pub use zgui_render_wgpu::wgpu;

/// One shared device, plus the epoch that says when it stopped being the one you knew.
///
/// Handed to producers in [`SurfaceEvent::Attached`] and [`SurfaceEvent::DeviceLost`]. The
/// generation moves when the renderer rebuilt its device; every texture created against an older
/// generation is gone, which is exactly what `DeviceLost` says.
#[derive(Clone)]
pub struct GpuShare {
    /// The device, the queue and everything around them.
    gpu: Arc<Gpu>,
    /// Which device epoch this is.
    generation: u64,
}

impl GpuShare {
    /// The device textures for this window must be created on.
    pub fn device(&self) -> &wgpu::Device {
        self.gpu.device()
    }

    /// The one queue this device's work is ordered on.
    pub fn queue(&self) -> &wgpu::Queue {
        self.gpu.queue()
    }

    /// The device epoch; two shares with different generations share nothing.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// What the natural size of a surface's content is, before CSS has its say.
///
/// The same three independent answers replaced content gives everywhere; see
/// [`Intrinsic`]. Constructors for the two common shapes are here so a producer states one line.
#[derive(Clone, Copy, Debug, Default)]
pub struct SurfaceIntrinsic {
    /// The natural size in CSS pixels, if there is one.
    pub size: Option<(f32, f32)>,
    /// The width-to-height ratio, if one is known before a size is.
    pub ratio: Option<f32>,
}

impl SurfaceIntrinsic {
    /// Content whose natural size is known: a video's frame size, a page's point size.
    pub fn size(width: f32, height: f32) -> Self {
        Self {
            size: Some((width, height)),
            ratio: None,
        }
    }

    /// Content that knows its shape before its size: a 16∶9 stream still probing.
    pub fn ratio(ratio: f32) -> Self {
        Self {
            size: None,
            ratio: Some(ratio),
        }
    }

    /// The document-side form.
    fn lower(self) -> Intrinsic {
        Intrinsic {
            size: self
                .size
                .map(|(width, height)| Size::<CssPx, Css>::new(CssPx(width), CssPx(height))),
            ratio: self.ratio,
            baseline: None,
        }
    }
}

/// How a [`SurfaceHandle`] is set up.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceConfig {
    /// Whether presented textures carry premultiplied alpha. When `false` the compositor
    /// premultiplies while sampling, which costs nothing but says so.
    pub premultiplied: bool,
    /// What is known of the content's natural size up front.
    pub intrinsic: SurfaceIntrinsic,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            premultiplied: true,
            intrinsic: SurfaceIntrinsic::default(),
        }
    }
}

/// What the runtime tells a producer.
///
/// Delivered through the sink installed with [`SurfaceHandle::set_events`], on the UI thread,
/// during the frame's embed step. A sink forwards — over a channel, into an atomic — and returns;
/// it must not draw, block, or call back into the framework.
#[derive(Clone)]
pub enum SurfaceEvent {
    /// The element is mounted and laid out; textures go on this device, at this size.
    Attached {
        /// The device to create textures on.
        gpu: GpuShare,
        /// The content box, in device pixels.
        size: Size<u32, Device>,
        /// Device pixels per CSS pixel.
        scale: f32,
    },
    /// The content box changed size. Reallocate at your own pace: a stale-size texture keeps
    /// being drawn, scaled over the new box, until the next present.
    Resized {
        /// The new content box, in device pixels.
        size: Size<u32, Device>,
        /// Device pixels per CSS pixel.
        scale: f32,
    },
    /// Whether anyone can currently see the surface. Pause producing while `false`; presents
    /// made anyway are absorbed but buy nothing.
    Visible(bool),
    /// The device is gone and every texture with it. Recreate on the share and present again.
    DeviceLost {
        /// The replacement device.
        gpu: GpuShare,
    },
    /// The element is gone. Release everything; the handle itself may be bound again later.
    Detached,
}

/// Where a producer's events go.
type EventSink = Arc<dyn Fn(SurfaceEvent) + Send + Sync>;

/// The shared half of one handle: what presents, intrinsics and events meet on.
struct HandleState {
    /// How the producer said its textures should be read.
    config: SurfaceConfig,
    /// The newest presented texture, not yet taken by a frame. Latest wins.
    latest: Option<Arc<wgpu::Texture>>,
    /// A natural-size update not yet filed with layout.
    intrinsic_owed: Option<SurfaceIntrinsic>,
    /// Where events go, once the producer said.
    events: Option<EventSink>,
    /// How to wake the UI loop, once the element is bound to a window.
    waker: Option<Arc<RuntimeWaker>>,
}

/// The registered handles, weakly: dropping the last [`SurfaceHandle`] clone unregisters.
fn handles() -> std::sync::MutexGuard<'static, FxHashMap<u64, Weak<Mutex<HandleState>>>> {
    static REGISTRY: OnceLock<Mutex<FxHashMap<u64, Weak<Mutex<HandleState>>>>> = OnceLock::new();
    REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

thread_local! {
    /// Callback renderers parked by the element builder until the host adopts them.
    ///
    /// Thread-local because a [`SurfaceRenderer`] is UI-thread state — it is handed the frame's
    /// device access — while the handle registry above must be reachable from producer threads.
    static CALLBACKS: RefCell<FxHashMap<u64, Box<dyn SurfaceRenderer>>> =
        RefCell::new(FxHashMap::default());
}

/// One token, never reused, whichever kind of producer it names.
fn next_token() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The handle a producer with its own cadence presents through.
///
/// Cloneable, `Send + Sync`; the registration lives while any clone does. Bind it to an element
/// with [`SurfaceElementExt::source`].
#[derive(Clone)]
pub struct SurfaceHandle {
    /// The name the element carries.
    token: u64,
    /// The shared state, kept alive by the clones.
    state: Arc<Mutex<HandleState>>,
}

impl SurfaceHandle {
    /// Registers a producer described by `config`.
    pub fn new(config: SurfaceConfig) -> Self {
        let token = next_token();
        let state = Arc::new(Mutex::new(HandleState {
            config,
            latest: None,
            intrinsic_owed: Some(config.intrinsic),
            events: None,
            waker: None,
        }));
        let mut registry = handles();
        registry.retain(|_, held| held.upgrade().is_some());
        registry.insert(token, Arc::downgrade(&state));
        Self { token, state }
    }

    /// Presents `texture` as the surface's newest content.
    ///
    /// Latest-wins: a present that lands before the previous one was shown replaces it. The
    /// texture must live on the device [`SurfaceEvent::Attached`] delivered, be 2D,
    /// single-sampled, and of a filterable colour format. Wakes the UI loop when the element is
    /// bound; before that the texture parks and is shown by the frame that binds it.
    pub fn present(&self, texture: Arc<wgpu::Texture>) {
        let waker = {
            let mut state = self.lock();
            state.latest = Some(texture);
            state.waker.clone()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Refines what the content's natural size is; layout follows on the next frame.
    pub fn set_intrinsic(&self, intrinsic: SurfaceIntrinsic) {
        let waker = {
            let mut state = self.lock();
            state.config.intrinsic = intrinsic;
            state.intrinsic_owed = Some(intrinsic);
            state.waker.clone()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Installs where this producer hears about the element's life; see [`SurfaceEvent`].
    pub fn set_events(&self, sink: impl Fn(SurfaceEvent) + Send + Sync + 'static) {
        self.lock().events = Some(Arc::new(sink));
    }

    /// The state, locked; a poisoned lock is inherited rather than doubled.
    fn lock(&self) -> std::sync::MutexGuard<'_, HandleState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One run of an on-demand renderer.
pub struct SurfaceRenderCx<'a> {
    /// The device the texture lives on.
    pub device: &'a wgpu::Device,
    /// The queue to submit on; work submitted here is ordered before the frame that samples it.
    pub queue: &'a wgpu::Queue,
    /// The texture to fill. zgui owns it; draw into it and return.
    pub texture: &'a wgpu::Texture,
    /// A plain view of it, for a render pass.
    pub view: &'a wgpu::TextureView,
    /// The texture's extent — the element's content box — in device pixels.
    pub size: Size<u32, Device>,
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// The frame's clock, the same one animations read.
    pub timestamp: Timestamp,
    /// Whether another frame was asked for; written through
    /// [`request_animation_frame`](SurfaceRenderCx::request_animation_frame).
    animate: &'a mut bool,
}

impl SurfaceRenderCx<'_> {
    /// Asks to be rendered again next refresh. One-shot, like `requestAnimationFrame`: a
    /// renderer that stops calling it stops being called.
    pub fn request_animation_frame(&mut self) {
        *self.animate = true;
    }
}

/// A renderer zgui drives: it owns no texture, no cadence and no lifecycle, only the drawing.
pub trait SurfaceRenderer: 'static {
    /// Fills the texture. Called on the UI thread, after layout and before paint, on the frames
    /// where drawing is owed: the first, a resize, a device loss, or continuously while
    /// [`SurfaceRenderCx::request_animation_frame`] keeps being called.
    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>);

    /// The format of the texture zgui allocates for this renderer.
    fn format(&self) -> wgpu::TextureFormat {
        wgpu::TextureFormat::Rgba8Unorm
    }

    /// What the content's natural size is, for an element CSS leaves `auto`.
    fn intrinsic(&self) -> SurfaceIntrinsic {
        SurfaceIntrinsic::default()
    }
}

/// The element half: what binds a producer to a `surface` element.
pub trait SurfaceElementExt {
    /// Shows what `handle`'s producer presents.
    #[must_use]
    fn source(self, handle: &SurfaceHandle) -> Self;

    /// Shows what `renderer` draws, on zgui's own texture and cadence.
    #[must_use]
    fn renderer(self, renderer: impl SurfaceRenderer) -> Self;
}

impl SurfaceElementExt for zgui_elements::Element<zgui_elements::Surface> {
    fn source(self, handle: &SurfaceHandle) -> Self {
        self.property(
            PropKey::new(zgui_vocab::prop::surface::SOURCE),
            PropValue::Integer(handle.token as i64),
        )
    }

    fn renderer(self, renderer: impl SurfaceRenderer) -> Self {
        let token = next_token();
        CALLBACKS.with(|callbacks| {
            callbacks.borrow_mut().insert(token, Box::new(renderer));
        });
        self.property(
            PropKey::new(zgui_vocab::prop::surface::SOURCE),
            PropValue::Integer(token as i64),
        )
    }
}

/// What one binding's producer is.
enum Producer {
    /// A handle in the shared registry, presenting at its own cadence.
    Handle(Arc<Mutex<HandleState>>),
    /// A renderer this host drives, and the texture it owns for it.
    Callback {
        /// The renderer.
        renderer: Box<dyn SurfaceRenderer>,
        /// The texture the last render filled, kept until the size or device changes.
        texture: Option<wgpu::Texture>,
        /// Whether the last render asked to run again.
        animate: bool,
        /// Whether anything has been drawn since the texture last changed identity.
        drawn: bool,
    },
}

/// One surface element this host is filling.
struct Binding {
    /// The producer the element names.
    token: u64,
    /// What it is.
    producer: Producer,
    /// The display list's name for the texture.
    external: ExternalTextureId,
    /// The renderer's handle from the last registration, for release on unbind.
    registered: Option<TextureHandle>,
    /// The extent and premultiplication last registered, so a change re-registers.
    described: Option<(Size<i32, Device>, bool)>,
    /// The presented texture currently attached, pinned until the next attach.
    attached: Option<Arc<wgpu::Texture>>,
    /// The content box's device-pixel size as of the last sync, for resize edges.
    last_size: Option<Size<u32, Device>>,
    /// Whether the producer was last told it is visible.
    visible: bool,
    /// The first frame timestamp at which this surface was continuously invisible.
    invisible_since: Option<Timestamp>,
    /// Whether the content cache currently points at this binding's external name.
    content_attached: bool,
}

/// Cold callback textures are cheap to reproduce but expensive to retain indefinitely.
const INVISIBLE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The wgpu embed host: install one per window, or let the umbrella crate do it.
pub struct WgpuSurfaces {
    /// Every bound surface element, by node.
    bindings: FxHashMap<NodeKey, Binding>,
    /// The document revision the bindings were last scanned at.
    scanned_at: Option<u64>,
    /// The device epoch, as the identity of the renderer's `Gpu`.
    device: Option<usize>,
    /// The device generation handed out in [`GpuShare`]s.
    generation: u64,
    /// The next external-texture name.
    next_external: u64,
}

impl WgpuSurfaces {
    /// A host with nothing bound.
    pub fn new() -> Self {
        Self {
            bindings: FxHashMap::default(),
            scanned_at: None,
            device: None,
            generation: 0,
            next_external: 1,
        }
    }
}

impl Default for WgpuSurfaces {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbedHost for WgpuSurfaces {
    fn sync(&mut self, cx: &mut EmbedSyncCx<'_>) -> EmbedSyncReport {
        let mut report = EmbedSyncReport::default();

        // The door to the device. A renderer with no door — capture, headless — leaves the host
        // doing bookkeeping only: elements bind and claim intrinsics so layout is truthful, and
        // nothing is attached because there is nothing to attach to. The share is taken here and
        // the borrow dropped, because everything below borrows the context again.
        let share = gpu_of(cx).map(|gpu| GpuShare {
            gpu,
            generation: self.generation,
        });

        if let Some(share) = share.as_ref() {
            let identity = Arc::as_ptr(&share.gpu) as usize;
            if self.device != Some(identity) {
                let lost = self.device.is_some();
                self.device = Some(identity);
                self.generation += 1;
                let share = GpuShare {
                    gpu: Arc::clone(&share.gpu),
                    generation: self.generation,
                };
                for binding in self.bindings.values_mut() {
                    // Everything attached died with the old device; `recover()` rebuilt the
                    // renderer around us and wiped its external tables.
                    binding.registered = None;
                    binding.described = None;
                    binding.attached = None;
                    binding.content_attached = false;
                    match &mut binding.producer {
                        Producer::Handle(state) => {
                            if lost {
                                tell(state, SurfaceEvent::DeviceLost { gpu: share.clone() });
                            }
                        }
                        Producer::Callback { texture, drawn, .. } => {
                            *texture = None;
                            *drawn = false;
                        }
                    }
                }
            }
        }
        // Re-stated after the epoch bump so every consumer below sees the current generation.
        let share = share.map(|share| GpuShare {
            generation: self.generation,
            ..share
        });

        if self.scanned_at != Some(cx.revision) {
            self.scanned_at = Some(cx.revision);
            self.rescan(cx);
        }

        for (&node, binding) in &mut self.bindings {
            let outcome = Self::sync_one(node, binding, cx, share.as_ref());
            report.animating |= outcome;
        }
        report
    }

    fn maintain(&mut self, cx: &mut EmbedMaintenanceCx<'_>) {
        for (&node, binding) in &mut self.bindings {
            if binding.visible
                || binding
                    .invisible_since
                    .is_none_or(|since| cx.timestamp.saturating_since(since) < INVISIBLE_GRACE)
            {
                continue;
            }
            let Producer::Callback { texture, drawn, .. } = &mut binding.producer else {
                // Producer-owned textures are intentionally never released by maintenance.
                continue;
            };
            if texture.take().is_none() {
                continue;
            }
            *drawn = false;
            binding.content_attached = false;
            binding.described = None;
            cx.content.remove_image(ReplacedId::new(node));
            if let Some(handle) = binding.registered.take() {
                cx.renderer.release_external(handle);
            }
        }
    }

    fn content_forgotten(&mut self) {
        for binding in self.bindings.values_mut() {
            binding.content_attached = false;
        }
    }

    fn memory(&self) -> EmbedMemoryReport {
        let mut report = EmbedMemoryReport::default();
        let mut producer_textures = FxHashSet::default();
        for binding in self.bindings.values() {
            match &binding.producer {
                Producer::Callback { texture, .. } => {
                    report.callback_owned += texture.as_ref().map_or(0, texture_bytes);
                }
                Producer::Handle(state) => {
                    if let Some(attached) = binding.attached.as_ref()
                        && producer_textures.insert(Arc::as_ptr(attached) as usize)
                    {
                        report.producer_owned += texture_bytes(attached);
                    }
                    let latest = lock(state).latest.clone();
                    if let Some(latest) = latest
                        && producer_textures.insert(Arc::as_ptr(&latest) as usize)
                    {
                        report.producer_owned += texture_bytes(&latest);
                    }
                }
            }
        }
        report
    }

    fn shutting_down(&mut self) {
        for (_, binding) in self.bindings.drain() {
            if let Producer::Handle(state) = &binding.producer {
                tell(state, SurfaceEvent::Detached);
            }
        }
    }
}

impl WgpuSurfaces {
    /// Re-reads which surface elements exist and which producers they name.
    fn rescan(&mut self, cx: &mut EmbedSyncCx<'_>) {
        let document = cx.document.borrow();
        let store = document.store();
        let mut seen: FxHashMap<NodeKey, u64> = FxHashMap::default();
        for slot in 0..store.slot_count() as u32 {
            let index = zgui_dom::NodeIndex::new(slot);
            let Some(record) = store.try_core(index) else {
                continue;
            };
            if !record
                .flags()
                .contains(zgui_dom::node::flags::NodeFlags::IS_REPLACED)
            {
                continue;
            }
            let key = store.key_of(index);
            if let Some(token) = zgui_dom::side::surface::token(store, key) {
                seen.insert(key, token);
            }
        }
        drop(document);

        // Elements that left, or that name a different producer now.
        let stale: Vec<NodeKey> = self
            .bindings
            .iter()
            .filter(|(node, binding)| seen.get(node) != Some(&binding.token))
            .map(|(node, _)| *node)
            .collect();
        for node in stale {
            self.unbind(node, cx);
        }

        // Elements that arrived.
        for (node, token) in seen {
            if self.bindings.contains_key(&node) {
                continue;
            }
            let producer = if let Some(state) = handles().get(&token).and_then(Weak::upgrade) {
                Producer::Handle(state)
            } else if let Some(renderer) =
                CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(&token))
            {
                Producer::Callback {
                    renderer,
                    texture: None,
                    animate: false,
                    drawn: false,
                }
            } else {
                // A token whose producer is gone — the handle dropped, or a callback adopted by
                // another window. Showing nothing is the only honest answer.
                continue;
            };

            let external = ExternalTextureId(self.next_external);
            self.next_external += 1;
            let binding = Binding {
                token,
                producer,
                external,
                registered: None,
                described: None,
                attached: None,
                last_size: None,
                visible: false,
                invisible_since: Some(cx.timestamp),
                content_attached: false,
            };

            let id = ReplacedId::new(node);
            match &binding.producer {
                Producer::Handle(state) => {
                    {
                        let mut state = lock(state);
                        state.waker = Some(Arc::clone(cx.waker));
                    }
                    let intrinsic = lock(state).config.intrinsic;
                    cx.intrinsics.set(id, intrinsic.lower());
                }
                Producer::Callback { renderer, .. } => {
                    cx.intrinsics.set(id, renderer.intrinsic().lower());
                }
            }
            mark(cx, node);
            self.bindings.insert(node, binding);
        }
    }

    /// Releases everything one binding holds, and tells its producer.
    fn unbind(&mut self, node: NodeKey, cx: &mut EmbedSyncCx<'_>) {
        let Some(binding) = self.bindings.remove(&node) else {
            return;
        };
        let id = ReplacedId::new(node);
        cx.content.remove_image(id);
        cx.intrinsics.remove(id);
        if let Some(handle) = binding.registered {
            cx.renderer.release_external(handle);
        }
        if let Producer::Handle(state) = &binding.producer {
            let mut locked = lock(state);
            locked.waker = None;
            drop(locked);
            tell(state, SurfaceEvent::Detached);
        }
        mark(cx, node);
    }

    /// One binding's frame: visibility, resizes, presents, renders. Returns whether it animates.
    fn sync_one(
        node: NodeKey,
        binding: &mut Binding,
        cx: &mut EmbedSyncCx<'_>,
        share: Option<&GpuShare>,
    ) -> bool {
        let id = ReplacedId::new(node);

        // Where the element ended up this frame, if it is on the screen at all.
        let (content_box, ink) = {
            let layout = cx.layout.borrow();
            let content_box = layout
                .boxes_of(node)
                .first()
                .and_then(|key| layout.fragments_of_box(*key).first().copied())
                .and_then(|fragment| {
                    layout
                        .fragment(fragment)
                        .map(|fragment| fragment.content_box)
                });
            let ink = zgui_layout::fragment::index::ink_of(&layout, node);
            (content_box, ink)
        };
        let size = content_box.map(|content_box| {
            Size::<u32, Device>::new(
                content_box.size.width.0.round().max(0.0) as u32,
                content_box.size.height.0.round().max(0.0) as u32,
            )
        });

        let viewport = zgui_geom::Rect::new(
            zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
            zgui_geom::Size::new(
                zgui_geom::DevicePx(cx.viewport.width.max(0) as f32),
                zgui_geom::DevicePx(cx.viewport.height.max(0) as f32),
            ),
        );
        let visible = !cx.occluded
            && size.is_some_and(|size| size.width > 0 && size.height > 0)
            && ink
                .intersection(viewport)
                .is_some_and(|visible| !visible.is_empty());
        if visible != binding.visible {
            binding.visible = visible;
            binding.invisible_since = (!visible).then_some(cx.timestamp);
            if let Producer::Handle(state) = &binding.producer {
                tell(state, SurfaceEvent::Visible(visible));
            }
        }

        let resized = visible && binding.last_size != size;
        if resized {
            let was_bound = binding.last_size.is_some();
            binding.last_size = size;
            if let (Producer::Handle(state), Some(share)) = (&binding.producer, share) {
                let size = size.expect("resized implies a size");
                let event = if was_bound {
                    SurfaceEvent::Resized {
                        size,
                        scale: cx.scale,
                    }
                } else {
                    SurfaceEvent::Attached {
                        gpu: share.clone(),
                        size,
                        scale: cx.scale,
                    }
                };
                tell(state, event);
            }
        }

        // An intrinsic that changed reaches layout whether or not anything can draw.
        if let Producer::Handle(state) = &binding.producer {
            let owed = lock(state).intrinsic_owed.take();
            if let Some(intrinsic) = owed {
                cx.intrinsics.set(id, intrinsic.lower());
                mark(cx, node);
            }
        }

        let Some(share) = share else {
            return false;
        };
        let gpu = Arc::clone(&share.gpu);

        match &mut binding.producer {
            Producer::Handle(state) => {
                let taken = visible.then(|| lock(state).latest.take()).flatten();
                let premultiplied = lock(state).config.premultiplied;
                if let Some(texture) = taken {
                    let extent =
                        Size::<i32, Device>::new(texture.width() as i32, texture.height() as i32);
                    attach(
                        cx,
                        AttachAsk {
                            id: binding.external,
                            node,
                            extent,
                            premultiplied,
                            texture: &texture,
                            described: &mut binding.described,
                            registered: &mut binding.registered,
                            content_attached: &mut binding.content_attached,
                            content_box,
                        },
                    );
                    binding.attached = Some(texture);
                } else if visible
                    && !binding.content_attached
                    && let Some(texture) = binding.attached.as_ref()
                {
                    let extent =
                        Size::<i32, Device>::new(texture.width() as i32, texture.height() as i32);
                    attach(
                        cx,
                        AttachAsk {
                            id: binding.external,
                            node,
                            extent,
                            premultiplied,
                            texture,
                            described: &mut binding.described,
                            registered: &mut binding.registered,
                            content_attached: &mut binding.content_attached,
                            content_box,
                        },
                    );
                }
                false
            }
            Producer::Callback {
                renderer: producer,
                texture,
                animate,
                drawn,
            } => {
                let Some(size) = size.filter(|_| visible) else {
                    return false;
                };
                let needs_texture = texture
                    .as_ref()
                    .is_none_or(|held| held.width() != size.width || held.height() != size.height);
                if needs_texture {
                    *texture = Some(gpu.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("zgui.surface"),
                        size: wgpu::Extent3d {
                            width: size.width.max(1),
                            height: size.height.max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: producer.format(),
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    }));
                    *drawn = false;
                }
                let held = texture.as_ref().expect("just ensured");
                let due = !*drawn || *animate;
                if due {
                    let view = held.create_view(&wgpu::TextureViewDescriptor::default());
                    *animate = false;
                    let mut cx_render = SurfaceRenderCx {
                        device: gpu.device(),
                        queue: gpu.queue(),
                        texture: held,
                        view: &view,
                        size,
                        scale: cx.scale,
                        timestamp: cx.timestamp,
                        animate,
                    };
                    producer.render(&mut cx_render);
                    *drawn = true;
                }

                if due || !binding.content_attached {
                    let extent = Size::<i32, Device>::new(size.width as i32, size.height as i32);
                    attach(
                        cx,
                        AttachAsk {
                            id: binding.external,
                            node,
                            extent,
                            // zgui's own render target: the renderer draws premultiplied, as the
                            // whole pipeline does.
                            premultiplied: true,
                            texture: held,
                            described: &mut binding.described,
                            registered: &mut binding.registered,
                            content_attached: &mut binding.content_attached,
                            content_box,
                        },
                    );
                }
                *animate
            }
        }
    }
}

/// Everything one attach needs, named so the call sites stay readable.
struct AttachAsk<'a> {
    /// The display list's name for the texture.
    id: ExternalTextureId,
    /// The element being filled.
    node: NodeKey,
    /// The texture's extent.
    extent: Size<i32, Device>,
    /// Whether its colour is already scaled by its alpha.
    premultiplied: bool,
    /// The resource itself.
    texture: &'a wgpu::Texture,
    /// The description last registered, updated when it changes.
    described: &'a mut Option<(Size<i32, Device>, bool)>,
    /// The renderer's handle, for later release.
    registered: &'a mut Option<TextureHandle>,
    /// Whether the content cache currently names this external.
    content_attached: &'a mut bool,
    /// Where the element is, for the damage the fresh pixels owe.
    content_box: Option<zgui_geom::Rect<zgui_geom::DevicePx, Device>>,
}

/// Registers (when the description moved), attaches, and damages one texture.
fn attach(cx: &mut EmbedSyncCx<'_>, ask: AttachAsk<'_>) {
    if *ask.described != Some((ask.extent, ask.premultiplied)) {
        let handle = cx.renderer.register_external(ExternalTexture {
            id: ask.id,
            handle: TextureHandle(0),
            size: ask.extent,
            premultiplied: ask.premultiplied,
        });
        *ask.registered = Some(handle);
        *ask.described = Some((ask.extent, ask.premultiplied));
    }
    let attached = cx
        .renderer
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<WgpuRenderer>())
        .is_some_and(|renderer| renderer.attach_external(ask.id, ask.texture));
    debug_assert!(
        attached,
        "the description above is what makes attach accept"
    );
    cx.content.set_external(ReplacedId::new(ask.node), ask.id);
    *ask.content_attached = true;
    if let Some(content_box) = ask.content_box {
        cx.damage
            .absorb(zgui_layout::fragment::diff::pixels(content_box));
    }
}

/// Exact allocated texel bytes for a texture's mip chain and sample count.
fn texture_bytes(texture: &wgpu::Texture) -> u64 {
    let format = texture.format();
    let Some(block_bytes) = format.block_copy_size(None) else {
        return 0;
    };
    let (block_width, block_height) = format.block_dimensions();
    let base = texture.size();
    let mut total = 0_u64;
    for level in 0..texture.mip_level_count() {
        let width = (base.width >> level).max(1).div_ceil(block_width);
        let height = (base.height >> level).max(1).div_ceil(block_height);
        let depth = (base.depth_or_array_layers >> level).max(1);
        total += u64::from(width)
            * u64::from(height)
            * u64::from(depth)
            * u64::from(block_bytes)
            * u64::from(texture.sample_count());
    }
    total
}

/// The device behind the frame's renderer, if the renderer has one to give.
///
/// A short borrow on purpose: the caller needs the context back the moment this answers.
fn gpu_of(cx: &mut EmbedSyncCx<'_>) -> Option<Arc<Gpu>> {
    cx.renderer
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<WgpuRenderer>())
        .map(|renderer| Arc::clone(renderer.gpu()))
}

/// Marks `node`'s replaced content changed, through the seam's own mark-and-wake pairing.
fn mark(cx: &EmbedSyncCx<'_>, node: NodeKey) {
    cx.replaced_content_changed(node);
}

/// Locks one handle's state, inheriting a poisoned lock.
fn lock(state: &Arc<Mutex<HandleState>>) -> std::sync::MutexGuard<'_, HandleState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Delivers one event to one producer, if it installed a sink.
fn tell(state: &Arc<Mutex<HandleState>>, event: SurfaceEvent) {
    let sink = lock(state).events.clone();
    if let Some(sink) = sink {
        sink(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_registers_and_the_last_clone_unregisters() {
        let handle = SurfaceHandle::new(SurfaceConfig::default());
        let token = handle.token;
        assert!(handles().get(&token).and_then(Weak::upgrade).is_some());

        let clone = handle.clone();
        drop(handle);
        assert!(
            handles().get(&token).and_then(Weak::upgrade).is_some(),
            "a clone keeps the producer reachable"
        );
        drop(clone);
        assert!(
            handles().get(&token).and_then(Weak::upgrade).is_none(),
            "an element naming this token now shows nothing, which is the honest answer"
        );
    }

    #[test]
    fn an_intrinsic_update_is_owed_until_a_sync_takes_it() {
        let handle = SurfaceHandle::new(SurfaceConfig::default());
        // The registration itself owes the initial claim.
        assert!(handle.lock().intrinsic_owed.is_some());
        handle.lock().intrinsic_owed = None;

        let worker = handle.clone();
        std::thread::spawn(move || {
            worker.set_intrinsic(SurfaceIntrinsic::ratio(16.0 / 9.0));
        })
        .join()
        .expect("the producer thread finishes");
        let owed = handle.lock().intrinsic_owed.expect("the update is owed");
        assert_eq!(owed.ratio, Some(16.0 / 9.0));
        let lowered = owed.lower();
        assert_eq!(lowered.ratio, Some(16.0 / 9.0));
        assert_eq!(lowered.size, None);
    }

    #[test]
    fn tokens_are_never_reused_across_kinds() {
        let first = SurfaceHandle::new(SurfaceConfig::default()).token;
        let second = next_token();
        let third = SurfaceHandle::new(SurfaceConfig::default()).token;
        assert!(first < second && second < third);
    }
}
