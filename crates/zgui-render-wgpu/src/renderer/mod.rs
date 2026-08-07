//! The renderer: what holds the device, and what happens to a display list.
//!
//! | Module | Contents |
//! |---|---|
//! | [`builder`] | how a renderer is put together over a device |
//! | [`draw`] | one frame: the plan, the recording, the acquisition and the copy |
//! | [`frame`] | the buffers a frame stages its data through |
//! | [`readback`] | reading a target back, for a test that has to look at pixels |

pub mod builder;
pub mod draw;
pub mod frame;
pub mod readback;
pub mod shared;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render::{ExternalTexture, RenderTarget, VectorBackend};
use zgui_scene::ExternalTextureId;

use crate::atlas_backend::sink::AtlasTextures;
use crate::bind::globals::SubpixelOrder;
use crate::frame::fault::FaultInjector;
use crate::frame::pass::AttachedTexture;
use crate::frame::vector::VectorSource;

/// A backend companion's lazy vector-rasteriser constructor.
pub type VectorFactory = fn(&Arc<Gpu>, Size<i32, Device>) -> Box<dyn VectorSource>;
use crate::gpu::device::Gpu;
use crate::pipeline::Pipelines;
use crate::pipeline::kind::PipelineKind;
use crate::renderer::frame::FrameBuffers;
use crate::renderer::readback::Pixels;
use crate::renderer::shared::{DeviceState, SharedGraphics};
use crate::target::acquire::Acquisition;
use crate::target::group_pool::GroupPool;
use crate::target::scene_texture::SceneTexture;
use crate::target::swapchain::Presentation;

/// Something to run between submitting a frame's work and putting it on the screen.
///
/// A compositor wants to be told that a frame is coming before it arrives, and the call that tells
/// it belongs to whatever owns the native window. This is where that call is made, at the one
/// point in the frame where it is meaningful. It runs only for an acquisition that will actually
/// be presented: announcing a frame after an acquisition failed can leave a compositor waiting for
/// a surface commit that never comes.
pub type PrePresent = Box<dyn Fn() + Send + Sync>;

/// How a renderer was built, which is what it needs to rebuild itself.
///
/// A lost device takes everything on it: pipelines, targets, buffers and every cached raster. What
/// survives is above the renderer — the atlas's own allocator and its keys, which are policy with
/// no device in them — so recovery is a rebuild rather than a repair, and this is what it is
/// rebuilt from.
#[derive(Clone, Copy, Debug)]
pub enum Origin {
    /// A texture stood in for the surface, and one can be created again from nothing.
    Offscreen {
        /// The format presented in.
        format: wgpu::TextureFormat,
        /// Whether the device could view a texture under a second format.
        mutable_texture_formats: bool,
    },
    /// A window's surface, which only whatever owns the window can produce again.
    Surface,
}

/// A renderer over one graphics device.
pub struct WgpuRenderer {
    /// The device everything here lives on.
    gpu: Arc<Gpu>,
    /// Where a composed frame is copied to.
    presentation: Presentation,
    /// The persistent target frames are composed into.
    composed: SceneTexture,
    /// The bind group the copy reads the composed target through.
    composed_binding: wgpu::BindGroup,
    /// The pipelines, shared with every other renderer on this device.
    ///
    /// Shared because compiling one costs the same whichever window asked for it, and because the
    /// atlas bind groups were built against these layouts and no others. Keyed by format as well as
    /// kind, so two windows whose displays negotiated different formats add entries here rather
    /// than needing maps of their own.
    pipelines: Rc<RefCell<Pipelines>>,
    /// The graphics this renderer belongs to, when it was built from a shared device.
    ///
    /// `None` for a renderer built through [`Builder`](crate::Builder), which owns its device
    /// alone. It is what lets every window converge on one replacement device after a loss instead
    /// of opening one each.
    shared: Option<SharedGraphics>,
    /// This frame's buffers.
    buffers: FrameBuffers,
    /// The atlas textures.
    atlas: AtlasTextures,
    /// Targets isolated content is composed in.
    groups: GroupPool,
    /// The sampler every magnifying read goes through.
    sampler: wgpu::Sampler,
    /// Whatever rasterises the vector parts of a scene, when one has been attached.
    vectors: Option<Box<dyn VectorSource>>,
    /// How to initialize `vectors` on the first frame that actually contains complex paths.
    vector_factory: Option<VectorFactory>,
    /// Which backend the lazy factory intends to build; replaced by the raster's own answer once
    /// it has run, so a Vello construction failure reports the coverage fallback truthfully.
    vector_backend: Option<VectorBackend>,
    /// Answers to give instead of asking the surface.
    faults: FaultInjector,
    /// Whether the next frame has to redraw the whole surface whatever its damage set says.
    full_damage_next: bool,
    /// The scratch a scroll shift stages its copy through, once one has been asked for.
    ///
    /// `None` for a window that has never shifted anything, which is every window that has never
    /// scrolled a self-contained region.
    shift_scratch: Option<crate::frame::shift::ShiftScratch>,
    /// A region of the composed target whose pixels are to be moved before this frame draws.
    ///
    /// Taken by the next `draw`. Held rather than performed on the spot because the copy belongs in
    /// the frame's own encoder, ahead of the passes that redraw what the move left undefined.
    pending_shift: Option<zgui_render::ScrollShift>,
    /// Whether the composed target still has to reach the surface.
    ///
    /// A failed acquisition happens after composition, so its damage is retired, but no surface
    /// received the composed target. The retry may therefore have no new damage and still owe the
    /// final copy and presentation.
    present_composed_next: bool,
    /// The surface being drawn for.
    target: RenderTarget,
    /// Which way round the display's subpixels run.
    subpixel_order: SubpixelOrder,
    /// Textures the renderer did not draw, with the resource behind each.
    externals: BTreeMap<ExternalTextureId, AttachedTexture>,
    /// Textures described but not yet handed over.
    pending_externals: BTreeMap<ExternalTextureId, ExternalTexture>,
    /// The next handle to hand out.
    next_handle: u64,
    /// What to run between submission and presentation.
    pre_present: Option<PrePresent>,
    /// How long the last frame waited to be handed a surface to present into.
    acquire_block: std::time::Duration,
    /// How many times *this* surface's acquisition has failed validation in a row.
    ///
    /// Per renderer rather than per device: a stuck swap chain is one window's health, and a device
    /// shared by several would otherwise have one window's successful frames clear another window's
    /// run of failures. The escalation it leads to is still device-wide, because that is what a
    /// device that will not present again means.
    consecutive_validation_failures: u32,
    /// How this renderer was built, so that it can be built again.
    origin: Origin,
    /// Which backends were enumerated, so recovery considers the same ones.
    backends: wgpu::Backends,
    /// Whether a frame with more vector passes than the scratch can hold stops the program.
    vector_shortfall_is_fatal: bool,
}

impl std::fmt::Debug for WgpuRenderer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuRenderer")
            .field("adapter", &self.gpu.describe())
            .field("target", &self.target)
            .field("formats", &self.presentation.formats())
            .finish_non_exhaustive()
    }
}

impl WgpuRenderer {
    /// Assembles a renderer over `device`, presenting to `presentation`.
    pub(crate) fn assemble(
        device: Rc<DeviceState>,
        shared: Option<SharedGraphics>,
        presentation: Presentation,
        target: RenderTarget,
        pre_present: Option<PrePresent>,
        origin: Origin,
        backends: wgpu::Backends,
    ) -> Self {
        let gpu = Arc::clone(&device.gpu);
        let pipelines = Rc::clone(&device.pipelines);
        let formats = presentation.formats();
        let composed = SceneTexture::new(&gpu, target.size, formats.scene);
        let composed_binding = bind_composed(&gpu, &pipelines.borrow(), &composed);
        // The atlas builds a bind group per texture, and a bind group has to be built against the
        // same layout the pipeline was: one layout, cloned, rather than two that happen to agree.
        let atlas = AtlasTextures::new(
            Arc::clone(&gpu),
            pipelines.borrow().layouts().sampled.clone(),
        );
        let buffers = FrameBuffers::new(&gpu);
        let groups = GroupPool::new(target.size, GroupPool::BUDGET);
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zgui.sampler.filtering"),
            // Clamping is what a magnifying read wants at the edge of a target; everything a
            // filter must see fade to transparent is cleared to transparent inside the target
            // rather than left to the sampler's address mode, which no core device can be asked
            // for a transparent border on.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        // Building the pipelines now rather than at the first frame is what keeps a driver's first
        // compilation out of a frame the user is waiting for. A second window on the same device
        // finds them already built unless its format differs, in which case only the entries that
        // format needs are compiled.
        {
            let mut built = pipelines.borrow_mut();
            for kind in PipelineKind::ALL {
                let format = match kind {
                    PipelineKind::Blit | PipelineKind::BlitUndoSrgb => formats.present_attachment(),
                    _ => formats.scene,
                };
                let _ = built.get(&gpu, kind, format);
            }
            built.persist();
        }
        Self {
            gpu,
            presentation,
            composed,
            composed_binding,
            pipelines,
            shared,
            buffers,
            atlas,
            groups,
            sampler,
            vectors: None,
            vector_factory: None,
            vector_backend: None,
            faults: FaultInjector::from_environment(),
            // Nothing has ever been composed, so the first frame cannot rely on what the target
            // holds however small its damage set is.
            full_damage_next: true,
            shift_scratch: None,
            pending_shift: None,
            present_composed_next: false,
            target,
            subpixel_order: SubpixelOrder::default(),
            externals: BTreeMap::new(),
            pending_externals: BTreeMap::new(),
            next_handle: 1,
            pre_present,
            acquire_block: std::time::Duration::ZERO,
            consecutive_validation_failures: 0,
            origin,
            backends,
            // Loud where a developer will see it, survivable where a user is waiting. A frame
            // planning more vector passes than there are scratch layers is a ceiling that has been
            // reached rather than a condition anyone chose, and it costs real content either way.
            vector_shortfall_is_fatal: cfg!(debug_assertions),
        }
    }

    /// Decides whether running out of vector scratch layers stops the program.
    ///
    /// It does by default in a build with debug assertions on, because a ceiling reached silently
    /// is a ceiling nobody raises. A test that drives a frame past the ceiling on purpose turns it
    /// off, and so may an application that would rather lose part of a drawing than a process.
    pub fn set_vector_shortfall_fatal(&mut self, fatal: bool) {
        self.vector_shortfall_is_fatal = fatal;
    }

    /// Rebuilds everything after the device was lost.
    ///
    /// Recovery is per *device*, and it is a rebuild in a fixed order: adapter, device and queue
    /// through the candidate loop, then what is presented to, then the pipelines, the composed
    /// target, the buffers and the atlas textures. Nothing a device held survives it — not a
    /// pipeline, not a target, and not one cached tile's contents — so cached rasters are produced
    /// again on demand rather than salvaged.
    ///
    /// The surface arm cannot be recovered here: a surface belongs to a window, and only whatever
    /// owns that window can produce another one.
    pub fn recover(&mut self) -> Result<(), zgui_render::GpuUnavailable> {
        let Origin::Offscreen {
            format,
            mutable_texture_formats,
        } = self.origin
        else {
            return Err(zgui_render::GpuUnavailable::new().rejected(
                "the current surface",
                "a window's surface has to be created again by whatever owns the window",
            ));
        };
        // The old device is dropped before the new one is asked for, so its memory is not held
        // twice across the changeover.
        let target = self.target;
        let subpixel_order = self.subpixel_order;
        let vector_factory = self.vector_factory;
        let vector_backend = self.vector_backend;
        let rebuilt = match self.shared.clone() {
            // Shared graphics: every window on the dead device converges on one replacement rather
            // than opening one each, which is what keeps them sharing anything at all afterwards.
            Some(shared) => {
                let pre_present = self.pre_present.take();
                let state = shared.replacement_for(&self.gpu)?;
                let presentation = Presentation::Offscreen(crate::target::swapchain::Offscreen::new(
                    &state.gpu,
                    Size::new(target.size.width.max(1), target.size.height.max(1)),
                    format,
                    mutable_texture_formats,
                ));
                Self::assemble(
                    state,
                    Some(shared),
                    presentation,
                    target,
                    pre_present,
                    self.origin,
                    self.backends,
                )
            }
            None => {
                let mut builder = crate::Builder::with_backends(self.backends);
                if let Some(notify) = self.pre_present.take() {
                    builder = builder.with_pre_present(notify);
                }
                builder.offscreen(target, format, mutable_texture_formats)?
            }
        };
        // A rasteriser holds resources of the old device, so it does not survive either. It is
        // dropped rather than carried over, and whoever attached it attaches one again — carrying
        // it would mean compositing from a scratch texture that no longer exists.
        let had_raster = self.vectors.is_some();
        *self = rebuilt;
        self.subpixel_order = subpixel_order;
        self.vector_factory = vector_factory;
        self.vector_backend = vector_backend;
        if had_raster {
            tracing::warn!(
                "the device was rebuilt; the vector rasteriser it held did not survive it"
            );
        }
        // Nothing the old device held survives, least of all the target frames were composed into.
        self.full_damage_next = true;
        Ok(())
    }

    /// The device everything is drawn on.
    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.gpu
    }

    /// The formats this renderer draws in.
    pub fn formats(&self) -> crate::gpu::formats::Formats {
        self.presentation.formats()
    }

    /// The device side of the atlas, which is where rasterised tiles are uploaded.
    ///
    /// The atlas's *policy* — which tile goes where, what is evicted — is not a renderer's
    /// business and lives above it; this is the half that needs a device.
    pub fn atlas(&mut self) -> &mut AtlasTextures {
        &mut self.atlas
    }

    /// Which way round the display's subpixels are assumed to run.
    pub fn set_subpixel_order(&mut self, order: SubpixelOrder) {
        self.subpixel_order = order;
    }

    /// Hands over the resource behind a texture the renderer did not draw.
    ///
    /// The display list names such a texture by an opaque identifier, because what a texture *is*
    /// is a renderer's knowledge and the list is backend-neutral. This is where the two meet: a
    /// quad naming an identifier nothing has attached is counted as undrawn rather than drawn
    /// against whatever texture happened to be bound.
    ///
    /// Returns whether the identifier had been described by
    /// [`zgui_render::Renderer::register_external`] first; a texture
    /// nobody described has no opacity, no extent and no statement about its alpha, so there is
    /// nothing to draw it with.
    pub fn attach_external(&mut self, id: ExternalTextureId, texture: &wgpu::Texture) -> bool {
        let Some(described) = self.pending_externals.get(&id).copied() else {
            return false;
        };
        self.externals.insert(
            id,
            AttachedTexture {
                texture: described,
                view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            },
        );
        true
    }

    /// Answers the next `times` acquisitions with `answer` instead of asking the surface.
    ///
    /// Six of the seven answers a surface can give are ordinary events in a window's life and none
    /// of them happens on demand, so the only way the paths that handle them are ever taken in a
    /// test is by asking for them. `ZGUI_SURFACE_FAULT=<answer>[,n]` asks for the same thing from
    /// the environment.
    pub fn inject_surface_fault(&mut self, answer: Acquisition, times: u32) {
        self.faults.inject(answer, times);
    }

    /// Attaches whatever rasterises the vector parts of a scene.
    ///
    /// Until one is attached, a display list's vector passes are counted as undrawn rather than
    /// drawn from an unwritten scratch. Which implementation this is — a path rasteriser running
    /// compute shaders, or the simpler one that does not need them — is decided above the renderer,
    /// from what the device turned out to be able to do.
    pub fn set_vector_raster(&mut self, raster: Box<dyn VectorSource>) {
        self.vector_backend = Some(raster.backend());
        self.vectors = Some(raster);
        self.vector_factory = None;
    }

    /// Installs a constructor that runs only when a frame has vector passes to rasterise.
    pub fn set_vector_factory(&mut self, factory: VectorFactory) {
        self.set_vector_factory_for(factory, VectorBackend::Other);
    }

    /// Installs a lazy constructor and names the backend it is expected to build.
    ///
    /// The constructed rasteriser's own answer replaces `backend`, which makes a fallback visible.
    pub fn set_vector_factory_for(&mut self, factory: VectorFactory, backend: VectorBackend) {
        self.vector_factory = Some(factory);
        self.vectors = None;
        self.vector_backend = Some(backend);
    }

    /// Whether a vector rasteriser is attached.
    pub fn has_vector_raster(&self) -> bool {
        self.vectors.is_some() || self.vector_factory.is_some()
    }

    /// Whether the configured rasteriser has paid its fixed initialization cost yet.
    pub fn vector_raster_initialized(&self) -> bool {
        self.vectors.is_some()
    }

    /// Targets isolated content is composed in.
    pub fn groups(&self) -> &GroupPool {
        &self.groups
    }

    /// How many bytes of isolated targets may be resident before the pool reduces resolution.
    ///
    /// Setting it discards every target the pool holds, because a budget that is now too small has
    /// to be met by giving something back rather than by refusing the next lease alone.
    pub fn set_group_budget(&mut self, bytes: u64) {
        self.groups = GroupPool::new(self.composed.used().size, bytes);
    }

    /// Reads back the persistent target frames are composed into.
    pub fn read_composed(&self) -> Pixels {
        readback::read(
            &self.gpu,
            self.composed.texture(),
            self.composed.format(),
            self.target.size,
        )
    }

    /// Reads back what was presented, when what was presented is a texture rather than a window.
    ///
    /// A window's surface cannot be read back — the compositor owns it the moment it is
    /// presented — so this answers `None` for one, and every pixel assertion runs against a
    /// stand-in surface configured by the same rules.
    pub fn read_presented(&self) -> Option<Pixels> {
        match &self.presentation {
            Presentation::Offscreen(offscreen) => Some(readback::read(
                &self.gpu,
                offscreen.texture(),
                offscreen.formats().surface,
                self.target.size,
            )),
            Presentation::Surface(_) => None,
        }
    }

    /// Rebuilds everything sized against the surface.
    fn resize(&mut self, size: Size<i32, Device>) {
        self.presentation.resize(&self.gpu, size);
        self.groups.resize(size);
        if self.composed.resize(&self.gpu, size) {
            self.composed_binding =
                bind_composed(&self.gpu, &self.pipelines.borrow(), &self.composed);
            // A reallocated target holds nothing at all, so no rectangle outside this frame's
            // damage set still shows what the frame before it drew.
            self.full_damage_next = true;
        }
    }
}

/// The bind group the copy reads the composed target through.
fn bind_composed(gpu: &Gpu, pipelines: &Pipelines, composed: &SceneTexture) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("zgui.bind.composed"),
        layout: &pipelines.layouts().loaded,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(composed.view()),
        }],
    })
}
