//! One window: its document, the engines over it, its renderer, and its frame.
//!
//! A window owns everything that is per-document — the tree, the rule set, the boxes, the display
//! list, the input state — and one thing that is per-surface: the renderer. The frame sequence
//! itself is [`frame`], because the sequence is the part with rules in it and it should not have
//! to be read past forty fields to find.

pub mod a11y;
pub mod anim;
mod brushes;
mod budget;
pub mod caret;
mod crossing;
mod cursor;
pub mod frame;
pub mod input;
pub mod observe;
pub mod pointer_text;
pub mod present;
pub mod probe;
pub mod resize;
pub mod scale;
pub mod scheme;
pub mod scroll;
mod select;
mod sheets;
mod surface_focus;
mod value;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use zgui_bits::DamageSet;
use zgui_dom::Document;
use zgui_geom::{CssPx, Device, DevicePx, Scale, Size};
use zgui_input::Router;
use zgui_layout::HitIndex;
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::{ContentCache, Painter};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::Renderer;
use zgui_scene::Scene;
use zgui_style::{SheetHandle, SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_view::{Anchor, BuildCxOwned, DomHandle, HostHandle};
use zgui_view_dom::DocumentDom;

use crate::binding::{HostBinding, NoBinding};
use crate::host::RuntimeHost;
use crate::text::TextEngine;

/// What the first starvation probe waits, which is also what the graphics API's acquisition is
/// willing to block — so the first probe costs at most one such block per second.
pub(crate) const STARVE_BACKOFF_START: Duration = Duration::from_secs(1);

/// The most a starvation probe waits. A cap because the probe is also what recovers a *visible*
/// window from a compositor hiccup no event announces the end of.
pub(crate) const STARVE_BACKOFF_CAP: Duration = Duration::from_secs(16);
use crate::timer::Timers;
use crate::wake::FrameGate;

/// How much texture memory a window's rasterised content may hold before cold content is freed.
///
/// Sixty-four mebibytes: several times what a text-heavy document's glyphs cost at any ordinary
/// scale, and well under what the atlas could grow to if nothing bounded it. It is a level the
/// cache returns *below* between frames rather than a ceiling an allocation is refused at, so a
/// single frame that needs more than this gets it and the excess comes back out of the cold
/// generations afterwards — a document is never made to drop content it is drawing.
///
/// Stated here rather than left unset because eviction that nothing bounds has no criterion: an
/// unbounded cache never frees, and a cache freeing to no stated level frees for no reason. A
/// window that wants a different number sets it through
/// [`ContentCache::set_soft_bytes`](zgui_paint::ContentCache::set_soft_bytes).
pub const ATLAS_SOFT_BYTES: u64 = 64 * 1024 * 1024;

/// What a window should be when it is opened.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WindowContent {
    /// The application's own stylesheet, as text.
    pub stylesheet: Option<String>,
    /// This window's own sheet, cascaded after the application's.
    ///
    /// For the window that is not the application: an inspector, a preferences panel, a palette.
    /// Later in the cascade than the application's own sheet, so a rule of equal weight here wins.
    pub window_stylesheet: Option<String>,
    /// What to tell about each frame after it has been produced, if anything.
    ///
    /// An option and not a list: one seam, one occupant. A window with several probes would have
    /// to decide what order they run in and what a slow one does to the frame that called it, and
    /// a tool that wants to feed two consumers can do that on its own side without the loop
    /// growing an opinion about it.
    pub probe: Option<Rc<dyn crate::probe::FrameProbe>>,
}

impl WindowContent {
    /// The application's options with one window's own laid over them.
    ///
    /// The application's stylesheet stays the application's and the window's becomes the second
    /// sheet, so a window sheet extends the application's rather than replacing it. A window that
    /// brought its own probe is watched by that one; one that brought none is watched by whatever
    /// watches the application.
    pub(crate) fn layered_with(&self, window: &Self) -> Self {
        Self {
            stylesheet: self.stylesheet.clone(),
            window_stylesheet: window
                .window_stylesheet
                .clone()
                .or_else(|| window.stylesheet.clone()),
            probe: window.probe.clone().or_else(|| self.probe.clone()),
        }
    }
}

/// One window, and everything that draws into it.
pub struct Window {
    /// What is being drawn into.
    surface: Arc<dyn Surface>,
    /// The tree.
    document: Rc<RefCell<Document>>,
    /// The node-tree seam over it.
    dom: Rc<DocumentDom>,
    /// The handle a view holds.
    dom_handle: DomHandle,
    /// The engine seam a view asks its geometry through.
    host: Rc<RuntimeHost>,
    /// The handle a view holds for it.
    host_handle: HostHandle,
    /// The rule set and the device.
    engine: StyleEngine,
    /// The application's cascade pool, when it runs one.
    style_pool: Option<Rc<zgui_style::engine::thread_pool::StylePool>>,
    /// Whether the next restyle is a broad one, which is what the pool is for.
    ///
    /// True for the first cascade — the largest this window will ever run — and again whenever
    /// the device is rebuilt, which recascades everything. An incremental restyle stays on the
    /// frame thread: measured on the scroll-recycle path, handing a handful of arriving rows to
    /// the pool cost four percent of the frame and bought nothing.
    broad_restyle: bool,
    /// The application's layout pool, when it runs one.
    layout_pool: Option<std::sync::Arc<zgui_layout::tree::parallel::LayoutPool>>,
    /// The boxes, their results and their fragments.
    layout: Rc<RefCell<LayoutStore>>,
    /// What is under a point.
    hit: HitIndex,
    /// The reusable buffers the fragment walk works in, warm across frames.
    diff_scratch: zgui_layout::fragment::diff::DiffScratch,
    /// Where each scroll container is scrolled to, and everything that moves one over time.
    scroll: Rc<RefCell<zgui_scroll::Scroller>>,
    /// What this desktop means by one detent of a wheel, and which way it points.
    ///
    /// Held on the window rather than asked for per event because the platform context is borrowed
    /// for the duration of one callback and a scroll is carried out inside a frame, which is a
    /// different one. It is what the backend answered when the window was opened; a backend that
    /// says nothing gets what an ordinary desktop does.
    scroll_settings: zgui_platform::ScrollSettings,
    /// Whether the window's own scrolling is frozen, and so may not move at all.
    ///
    /// What a modal surface holds while it is open. It is deliberately not a style change: the
    /// page keeps every scroll container, every offset and every reserved gutter it had, so
    /// opening and closing a surface over it moves nothing.
    scroll_frozen: bool,
    /// When the last frame ran, so that a motion knows how much time it has to spend.
    ///
    /// A gap far longer than a refresh interval is a park rather than a slow frame, and buys a
    /// motion nothing: see [`Window::advance_scroll`].
    last_frame: Option<zgui_vocab::Timestamp>,
    /// What the running animations wrote on the last frame, and what still owes an undo.
    animator: zgui_anim::Animator,
    /// The moment the running animations owe their next frame at.
    ///
    /// Held rather than derived when the park is computed, because it is a phase and not a delay:
    /// the next frame belongs one refresh interval after the moment the last one was owed at, and a
    /// deadline recomputed from the present moment is pushed forward by every wake the window has
    /// for any other reason. See [`AnimationCadence`](crate::AnimationCadence).
    animation: crate::window::anim::cadence::AnimationCadence,
    /// The display list.
    scene: Scene,
    /// The paint stage's state between frames.
    painter: Painter,
    /// Whether every replayed range is checked against the cache holding the rasters it draws.
    ///
    /// See [`Window::set_verify_replays`]. It is read once per frame and passed into the walk,
    /// rather than the walk reading the environment, so that the answer is a property of this
    /// window and a test can state it.
    verify_replays: bool,
    /// Whether this window's frames keep the coordinate system every primitive was pushed under.
    ///
    /// See [`Window::set_check_spatial_dependencies`]. Applied to the scene at the start of every
    /// frame rather than read from the environment once, for the same reason as `verify_replays`:
    /// a check nothing can switch on is a check no test can watch working.
    check_spatial_dependencies: bool,
    /// The glyph tiles and decoded images this window draws from.
    content: ContentCache,
    /// The same, for the nodes whose content is an externally rendered surface.
    replaced_surfaces: Arc<crate::replaced::IntrinsicTable>,
    /// The pictures this window's `image` elements name: decode state, texels, and who shows what.
    images: crate::images::ImageLoader,
    /// Who fills this window's `surface` elements; [`NoEmbeds`](crate::embed::NoEmbeds) until an
    /// application installs a host.
    embed: Box<dyn crate::embed::EmbedHost>,
    /// Who lays custom elements out, when an application brought a registry of them.
    custom_layout: Option<Box<dyn zgui_layout::custom::CustomLayoutSource>>,
    /// Who paints them; the two halves of one registry, installed together.
    custom_paint: Option<Box<dyn zgui_paint::content::custom::CustomPaintSource>>,
    /// Whether the last embed sync wanted a frame per refresh; one input to the animation gate.
    embed_animating: bool,
    /// The wake route this window was opened with, retained so the embed sync can hand it to
    /// producers on other threads.
    waker: Arc<crate::wake::RuntimeWaker>,
    /// The outlines this window's drawings have already been placed into their boxes as.
    vectors: zgui_paint::VectorCache,
    /// The actual raster paths selected by each element's most recently encoded vector content.
    vector_routes: rustc_hash::FxHashMap<zgui_dom::NodeKey, zgui_paint::VectorRoutes>,
    /// The document revision at which stale retained vector routes were last retired.
    vector_routes_revision: u64,
    /// The complex-vector elements present in the frame that first constructed Vello.
    vello_initializers: Vec<zgui_dom::NodeKey>,
    /// What each budgeted cache last did, and the levels the entry-counted ones are held to.
    ///
    /// The caches themselves are the fields around this one; what is kept here is the bookkeeping
    /// none of them can do for itself, because none of them counts frames. See
    /// [`budget`](crate::budget).
    budgets: crate::budget::Budgets,
    /// What turns a glyph into pixels.
    raster: Arc<dyn zgui_text::GlyphRaster>,
    /// What this frame must redraw.
    damage: DamageSet,
    /// What every fragment pass this frame ran moved rigidly, and whether it moved anything else.
    ///
    /// The input to deciding whether the renderer may translate pixels it already has rather than
    /// have them drawn again. Reset at the top of each frame and merged across passes, because a
    /// scroll delivered to a listener can relay out inside the frame that delivered it.
    rigid_moves: zgui_layout::fragment::diff::RigidMoves,
    /// What this frame had damaged before its first layout pass ran.
    ///
    /// The other half of [`Window::rigid_moves`]: the walk reports what *it* damaged beyond the
    /// movement, and this is what was already owed before it started. Held separately because a
    /// frame runs the walk more than once and only the first of them starts from what the frame
    /// inherited.
    damage_before_layout: DamageSet,
    /// How many layout passes this frame has run, so the first can be told from the rest.
    layout_passes: u32,
    /// Which containers scrolled this frame, and from where to where.
    ///
    /// Kept because the change log is drained by the dispatch that reports the scroll to the
    /// document, which happens well before anything decides what to draw.
    scrolled_this_frame: Vec<zgui_scroll::report::Scrolled>,
    /// Whether the cascade moved a text colour since the display list's brushes were last copied.
    brushes_moved: bool,
    /// Which brush slot each element's text is drawn through.
    ///
    /// Held per element rather than per cascade result, because a cascade result is a new object
    /// every time the cascade runs while everything already shaped still names the slot the old one
    /// claimed. This is the way back from the element the style engine reports to the slot its
    /// glyphs actually read.
    text_slots: rustc_hash::FxHashMap<
        (zgui_dom::NodeKey, zgui_style::TextRun),
        crate::window::brushes::TextSlot,
    >,
    /// The text engine.
    text: Box<dyn TextEngine>,
    /// What puts the display list on the screen.
    renderer: Box<dyn Renderer>,
    /// What is hovered, pressed, focused and captured.
    router: Router,
    /// Focus moves that have happened and have not yet been announced to the document.
    ///
    /// Announcing one is dispatching two events, and a dispatch may not begin inside another: focus
    /// moves as the *default* of an event that is still being carried out, with the document's
    /// change batch open. So the move is recorded here and the events go out at the point every
    /// other consequence of a handler goes out — after the dispatch that caused it has finished.
    pending_focus: Vec<crate::window::input::FocusMoved>,
    /// Elements the pointer has arrived on or left and that have not yet been told.
    ///
    /// Queued for the same reason focus moves are: the crossing is computed while the event that
    /// caused it is still being routed, with the document's change batch open, and a dispatch may
    /// not begin inside another.
    pending_crossings: Vec<crate::window::crossing::Crossing>,
    /// What the raw touch stream means: taps, long presses, drags and flicks.
    gestures: zgui_input::Gestures,
    /// The containers a drag in progress is scrolling, innermost first.
    ///
    /// Latched when the drag begins, because the content moves under the contact: a chain
    /// re-derived from where the finger is now stops naming the list as soon as the drag carries
    /// it past the edge of its own scrollport.
    panning: Vec<zgui_dom::NodeKey>,
    /// When the earliest contact still being held becomes a long press.
    ///
    /// Kept as a moment on the loop's own clock rather than derived when the park is computed,
    /// because the recogniser measures from event timestamps and the park is expressed in the
    /// platform's instants; the two are reconciled once, in the frame that has both.
    long_press_due: Option<Instant>,
    /// The editing model of every editable element that has been typed into.
    editors: crate::editing::Editors,
    /// The caret's phase, and what this frame draws for it.
    carets: crate::caret::Carets,
    /// How the user last interacted, which is what a programmatic focus move borrows its ring
    /// decision from.
    ///
    /// A surface that opens on a click and moves focus into itself is acting *for* the pointer,
    /// and a ring on the field it chose reads as a keyboard highlight nobody asked for; the same
    /// surface opened with the keyboard has to show where focus went or the keyboard user is
    /// lost. Starts as keyboard, because before anyone has interacted at all the safe reading is
    /// the one that keeps focus visible.
    focus_modality: zgui_input::FocusSource,
    /// The column a run of vertical caret motions is aiming for, in the paragraph's own pixels.
    ///
    /// Held between the arrow presses of one run and dropped by anything else that moves the
    /// caret, because the column belongs to the run: stepping through a short line and reading the
    /// caret's own x afterwards would leave the caret at that line's end for every line below it.
    vertical_goal: Option<f32>,
    /// The drag that is selecting text right now, if one is.
    ///
    /// Latched at the press and held until the release, because a selection's anchor is where the
    /// gesture *began*: re-deriving it from what is under the pointer now would make every drag
    /// select nothing at all.
    selecting: Option<crate::window::pointer_text::Selecting>,
    /// Text an editing command asked to be put on the clipboard, waiting for a turn of the loop
    /// that holds the platform context.
    ///
    /// A cut or a copy happens inside a frame, and the clipboard is reachable only from the loop
    /// that owns the platform — so what the model asked for is recorded here and taken by whoever
    /// has one. Dropping it instead is a cut that deletes the text and copies nothing, which is the
    /// worst of the two failures because the text is gone.
    clipboard: Vec<String>,
    /// Elements whose paste is waiting for the clipboard to be read.
    ///
    /// The same boundary as [`Window::clipboard`], crossed the other way: the chord is recognised
    /// inside a frame, the clipboard is readable only from the loop that owns the platform, and
    /// so the request is recorded here and answered by whoever has one.
    wants_paste: Vec<zgui_dom::NodeKey>,
    /// Clipboard text on its way into an element, waiting for the frame that will type it in.
    ///
    /// A paste edits the document and announces an input event, and both of those are a frame's
    /// to do: applying the text from outside one would begin a dispatch in the middle of a loop
    /// turn, against boxes no frame has settled.
    pastes: Vec<(zgui_dom::NodeKey, String)>,
    /// What the surface has been told about text input, and what it is owed.
    ime: zgui_input::Ime,
    /// Platform events that arrived since the last frame.
    queued: Vec<SurfaceEvent>,
    /// The window's own reactive scope.
    scope: Option<zgui_reactive::Mounted>,
    /// The mounted view, held because dropping it unmounts the tree.
    view: Option<Box<dyn Anchor>>,
    /// The application's own sheets, held because dropping a handle removes its sheet.
    _sheets: Vec<SheetHandle>,
    /// The sheets views installed for themselves, by the name each was installed under.
    ///
    /// Held so that installing the same name again *replaces* that sheet's text and keeps its
    /// place in the cascade. Removing and adding instead would move it to the end of the author
    /// origin, where it would start beating every sheet it used to lose to — which is a theme
    /// switch that silently changes what wins.
    view_sheets: rustc_hash::FxHashMap<String, SheetHandle>,
    /// The surface this frame is being built for.
    viewport: Viewport,
    /// The surface extent the viewport above was derived from, in device pixels.
    ///
    /// Kept beside it rather than recovered from it: the viewport is the extent divided by the
    /// scale, and dividing and multiplying by a fractional scale does not return the number that
    /// went in. This is what an arriving resize is compared against to tell a move from a repeat.
    extent: Option<Size<DevicePx, Device>>,
    /// How many device pixels there are to a CSS pixel.
    scale: f32,
    /// The light or dark preference the surface is being presented under.
    ///
    /// Kept beside the viewport because the viewport is rebuilt from the surface's extent every
    /// time the surface moves, and a preference that lived only inside it would be discarded by
    /// the next resize.
    scheme: zgui_style::ColorScheme,
    /// Whether the surface is entirely hidden.
    occluded: bool,
    /// Whether the surface is the one receiving the keyboard.
    ///
    /// Kept because a window system re-states it: a focus report that describes the state the
    /// window is already in must not settle a field a second time, and "settled once" is the whole
    /// contract of the event that leaving a field announces.
    ///
    /// A window that has just been opened is treated as focused. Every backend reports focus
    /// arriving rather than being held, so the alternative is a window that answers its first real
    /// report by doing the work of losing focus it never had.
    surface_focused: bool,
    /// Whether the surface has to be reconfigured before the next frame is built.
    reconfigure: bool,
    /// How often a configure may be answered with a frame, and what that has skipped.
    pace: crate::window::resize::ResizePace,
    /// How many times the renderer has been pointed at a new surface extent.
    configured: u64,
    /// How many offered frames were refused because the reconfiguration they owed was too early.
    declined: u64,
    /// When a frame that has been asked for is allowed to start.
    present: crate::window::present::PresentPace,
    /// The output refresh rate last reported, so a change in it is logged once rather than per
    /// frame. `None` inside the option is a backend that reports no rate at all.
    reported_rate: std::cell::Cell<Option<Option<u32>>>,
    /// When to try again after a frame that did not reach the screen, if one is owed.
    ///
    /// A frame whose acquisition timed out, or whose surface the compositor replaced underneath it,
    /// leaves the window showing the picture it had and owes another — but it owes it *later*. The
    /// condition is the presentation engine having nothing to hand over, and the way that is
    /// resolved is by the compositor coming back to the surface, not by this process asking again
    /// sooner. Asking again straight away spends a whole pipeline and then blocks in the
    /// acquisition for as long as the graphics API is willing to wait, on the thread that reads
    /// input — which is how one stall becomes several and the window stops answering the pointer.
    ///
    /// So it is a moment rather than a request, and the park is what comes back for it.
    retry_after: Option<Instant>,
    /// Whether the surface is starved: the compositor stopped handing over buffers.
    ///
    /// Latched by an acquisition that timed out, which under a queued presentation mode is what a
    /// surface the compositor is not drawing produces — the window sits on another workspace, or
    /// behind something, on a platform that reports no occlusion. While it is set the window is
    /// treated as the occluded one it very probably is: animations park, the blink stops, and a
    /// frame that runs anyway — a timer's — skips the present that would block the loop a second.
    /// What ends it is evidence of visibility (input, a focus, a configure) or the probe at
    /// [`Window::retry_after`] succeeding.
    starved: bool,
    /// How long the next starvation probe waits, doubling to [`STARVE_BACKOFF_CAP`].
    ///
    /// The probe is the one attempt the latch allows: it runs the pipeline and lets the
    /// acquisition block, so its price is up to a second of the loop — which is why the spacing
    /// grows while the surface stays starved and resets the moment anything presents.
    starve_backoff: Duration,
    /// One wall-clock deadline for shedding cold renderer and embed high-water resources.
    maintenance_due: Option<Instant>,
    /// How many offered frames were held back so that they would start closer to being shown.
    held: u64,
    /// The clock the pacing is measured on.
    ///
    /// Held here as well as inside the host because a configure arrives outside a frame, and the
    /// frame is where the clock is otherwise handed in. Two clocks would disagree about how long
    /// ago the last resize frame ran, which is the one thing the pacing is derived from.
    clock: Arc<dyn zgui_platform::Clock>,
    /// Whether the next frame is the first one.
    first_frame: bool,
    /// Whether a frame has been asked for and has not run yet.
    ///
    /// The platform coalesces repeated requests into one frame, so a second request costs nothing
    /// on the screen — but "one wake, one frame" is a property of the loop that is asserted on,
    /// and an assertion cannot tell a requester that fired twice from two requesters that each
    /// fired once. This is what keeps the count equal to the number of reasons there were.
    awaiting_frame: std::cell::Cell<bool>,
    /// Whether a frame is in flight and what it owes.
    gate: Arc<FrameGate>,
    /// The scheduled callbacks of every window.
    timers: Rc<RefCell<Timers>>,
    /// The downstream script engine, if one was installed.
    binding: Box<dyn HostBinding>,
    /// What to tell about each frame after it has been produced.
    probe: Option<Rc<dyn crate::probe::FrameProbe>>,
    /// What the accessibility tree was last told, and what it is owed.
    a11y: zgui_a11y::A11yBuilder,
    /// The elements this frame's fragment pass carried to a new position without changing.
    ///
    /// Held on the window rather than passed through, because the pass that discovers them and the
    /// stage that answers them are at opposite ends of the frame. Drained by
    /// [`Window::publish_a11y`], and kept between frames only in the sense that a frame with no
    /// fragment pass leaves it empty.
    a11y_moves: Vec<zgui_dom::NodeKey>,
    /// The coordinate systems this frame resolved to a different matrix than the last one.
    ///
    /// The other half of [`Window::a11y_moves`], and the half that does not depend on any fragment
    /// having been touched. A transform that is being animated rewrites the matrix under a name and
    /// leaves every rectangle measured through it exactly where it was, so the fragment pass has
    /// nothing to report and everything drawn in that space is somewhere else. Filled where the
    /// frame's matrices are published and drained by [`Window::publish_a11y`].
    moved_spaces: Vec<zgui_scene::SpatialId>,
    /// Which node the accessibility tree was last told holds focus.
    ///
    /// Focus rides on every accessibility update, so a frame that moved focus and changed no node
    /// at all still owes one — and without remembering what was published, nothing can tell that
    /// frame from one where focus did not move.
    published_focus: Option<zgui_dom::NodeKey>,
    /// The handle the application holds this window by, and reads its state through.
    handle: crate::windows::WindowHandle,
    /// The cursor the window was last told to show.
    ///
    /// Held so that a frame asking for the cursor it is already showing asks the windowing system
    /// for nothing. The pointer sits still over one element for most of a session, so the answer is
    /// the same on nearly every frame.
    cursor: zgui_platform::CursorStyle,
    /// The document revision the last completed frame serviced.
    ///
    /// The reactive flush is thread-wide, so a frame in *another* window runs effects that write
    /// this one's document, and the wake that would have asked this window to draw was serviced
    /// over there. Comparing what the document has taken against what a frame here has serviced is
    /// what notices that, whichever way the write arrived.
    serviced_revision: Cell<u64>,
}

impl Window {
    /// Opens a window over `surface`, drawn by `renderer`, and builds `view` into it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Stylesheet`](crate::AppError::Stylesheet) when the application's own
    /// sheet was rejected outright. A sheet with one unrecognised declaration is not rejected: the
    /// rest of it applies and the drop is reported.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        surface: Arc<dyn Surface>,
        document: zgui_dom::DocumentId,
        renderer: Box<dyn Renderer>,
        text: Box<dyn TextEngine>,
        raster: Arc<dyn zgui_text::GlyphRaster>,
        metrics: Arc<dyn zgui_text::FontMetricsSource>,
        clock: Arc<dyn zgui_platform::Clock>,
        timers: Rc<RefCell<Timers>>,
        waker: Arc<crate::wake::RuntimeWaker>,
        options: &WindowContent,
        handle: crate::windows::WindowHandle,
        close: Rc<RefCell<crate::commands::CloseCallbacks>>,
        view: impl FnOnce(&mut zgui_view::BuildCx<'_>) -> Box<dyn Anchor>,
    ) -> Self {
        // The identity the runtime minted for this window. Every node handle carries it, which is
        // what stops one window's handle resolving inside another window's tree.
        let document = Rc::new(RefCell::new(Document::with_id(document)));
        let replaced_images = crate::replaced::IntrinsicTable::new();
        let replaced_surfaces = crate::replaced::IntrinsicTable::new();
        document.borrow_mut().install_replaced_content(Arc::new(
            crate::replaced::ReplacedMux::new(vec![
                Arc::clone(&replaced_images),
                Arc::clone(&replaced_surfaces),
            ]),
        ));
        let dom = Rc::new(DocumentDom::new(Rc::clone(&document)));
        // The atlas may create textures as large as the device allows, so a photo the device can
        // hold is cached whole rather than clamped to the smallest supported device's limit.
        let atlas_limits = zgui_atlas::AtlasLimits {
            max_texture_size: renderer.capabilities().max_texture_size,
            ..zgui_atlas::AtlasLimits::default()
        };
        // The `- 2` keeps a maximal decode allocatable once the atlas pads the tile.
        let images = crate::images::ImageLoader::new(
            Arc::clone(&replaced_images),
            (atlas_limits.max_texture_size - 2).max(1) as u32,
        );
        let sources = images.source_queue();
        dom.set_attribute_hook(Rc::new(move |node, name, value| {
            if name.as_str() == "src" {
                sources.borrow_mut().push((node, value.map(str::to_owned)));
            }
        }));
        let dom_handle = DomHandle::from_rc(Rc::clone(&dom) as Rc<dyn zgui_view::Dom>);
        let document_id = dom.document_id();

        let layout = Rc::new(RefCell::new(LayoutStore::new(
            document.borrow().store().document(),
        )));
        let scroll = Rc::new(RefCell::new(zgui_scroll::Scroller::new()));

        let scale = surface.scale_factor() as f32;
        let size = surface.size();
        let viewport = Viewport::new(CssPx(size.width.0 / scale), CssPx(size.height.0 / scale))
            .at_scale(scale);

        let mut engine = {
            let borrowed = document.borrow();
            StyleEngine::new(&borrowed, metrics, viewport)
        };
        let mut sheets = Vec::new();
        // The application's sheet first and this window's own second, so a rule of equal weight in
        // a window's sheet wins over the application's.
        for css in [
            options.stylesheet.as_deref(),
            options.window_stylesheet.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let borrowed = document.borrow();
            let (handle, diagnostics) =
                engine.add_sheet(&borrowed, SheetOrigin::Author, SheetSource::Text(css));
            for report in diagnostics.iter() {
                tracing::warn!(target: "zgui::css", "{}", report.message);
            }
            sheets.push(handle);
        }
        let gate = Arc::clone(waker.gate());
        let scope = zgui_reactive::Mounted::new();
        let host = scope.with(|| {
            Rc::new(RuntimeHost::new(
                document_id,
                Rc::clone(&document),
                Rc::clone(&layout),
                Rc::clone(&scroll),
                Rc::clone(&timers),
                Arc::clone(&waker),
                Arc::clone(&clock),
            ))
        });
        host.set_scale(scale);
        let host_handle = HostHandle::from_rc(Rc::clone(&host) as Rc<dyn zgui_view::ViewHost>);

        let cx = BuildCxOwned::new(
            dom_handle.clone(),
            host_handle.clone(),
            scope.owner().clone(),
            document_id,
        );
        let root = dom.root_node();
        let mut built = scope.with(|| {
            // The engines have to be reachable from the *context* as well as from the build
            // context: `set_timeout` and `NodeRef`'s imperative escapes are written as free
            // functions and find the host that way. Without this a component that schedules
            // anything panics with nothing to point at.
            zgui_view::provide_host(host_handle.clone());
            // This window, so that `use_window` resolves the one a component is running in the way
            // `set_timeout` resolves the host it schedules against — and so that a component in one
            // window never reaches another's by accident.
            zgui_reactive::provide_local_context(handle.clone());
            zgui_reactive::provide_local_context(Rc::clone(&close));
            // The window is the last owner a task can fall back to, so a spawn outside any
            // component still dies with the window rather than outliving it.
            zgui_reactive::provide_task_set();
            view(&mut cx.cx())
        });
        built.mount(&dom_handle, root, None);

        Self {
            surface,
            document,
            dom,
            dom_handle,
            host,
            host_handle,
            engine,
            style_pool: None,
            broad_restyle: true,
            layout_pool: None,
            layout,
            hit: HitIndex::new(),
            diff_scratch: zgui_layout::fragment::diff::DiffScratch::default(),
            scroll,
            last_frame: None,
            animator: zgui_anim::Animator::new(),
            animation: crate::window::anim::cadence::AnimationCadence::parked(),
            scene: Scene::new(),
            painter: Painter::new(),
            verify_replays: zgui_layout::invariants::enabled(),
            check_spatial_dependencies: zgui_scene::invariant::enabled(),
            content: ContentCache::new(atlas_limits.with_soft_bytes(ATLAS_SOFT_BYTES)),
            replaced_surfaces,
            images,
            embed: Box::new(crate::embed::NoEmbeds),
            custom_layout: None,
            custom_paint: None,
            embed_animating: false,
            waker: Arc::clone(&waker),
            vectors: zgui_paint::VectorCache::new(),
            vector_routes: rustc_hash::FxHashMap::default(),
            vector_routes_revision: 0,
            vello_initializers: Vec::new(),
            budgets: crate::budget::Budgets::new(),
            raster,
            damage: DamageSet::full(),
            rigid_moves: zgui_layout::fragment::diff::RigidMoves::default(),
            damage_before_layout: DamageSet::new(),
            layout_passes: 0,
            scrolled_this_frame: Vec::new(),
            brushes_moved: false,
            text_slots: rustc_hash::FxHashMap::default(),
            text,
            renderer,
            router: Router::new(),
            pending_focus: Vec::new(),
            pending_crossings: Vec::new(),
            gestures: zgui_input::Gestures::new(),
            scroll_settings: zgui_platform::ScrollSettings::default(),
            scroll_frozen: false,
            panning: Vec::new(),
            long_press_due: None,
            editors: crate::editing::Editors::new(),
            carets: crate::caret::Carets::new(),
            focus_modality: zgui_input::FocusSource::Keyboard,
            vertical_goal: None,
            selecting: None,
            clipboard: Vec::new(),
            wants_paste: Vec::new(),
            pastes: Vec::new(),
            ime: zgui_input::Ime::new(),
            queued: Vec::new(),
            scope: Some(scope),
            view: Some(built),
            _sheets: sheets,
            view_sheets: rustc_hash::FxHashMap::default(),
            viewport,
            scale,
            scheme: viewport.scheme,
            occluded: false,
            surface_focused: true,
            extent: None,
            reconfigure: true,
            pace: crate::window::resize::ResizePace::new(),
            configured: 0,
            declined: 0,
            present: crate::window::present::PresentPace::free_running(),
            reported_rate: std::cell::Cell::new(None),
            retry_after: None,
            starved: false,
            starve_backoff: STARVE_BACKOFF_START,
            maintenance_due: None,
            held: 0,
            clock,
            first_frame: true,
            awaiting_frame: std::cell::Cell::new(false),
            gate,
            timers,
            binding: Box::new(NoBinding),
            probe: options.probe.clone(),
            a11y: zgui_a11y::A11yBuilder::new(),
            a11y_moves: Vec::new(),
            moved_spaces: Vec::new(),
            published_focus: None,
            serviced_revision: Cell::new(0),
            handle,
            cursor: zgui_platform::CursorStyle::Default,
        }
    }

    /// The handle the application holds this window by.
    pub fn handle(&self) -> &crate::windows::WindowHandle {
        &self.handle
    }

    /// Whether the document has been written since this window's last frame.
    ///
    /// True when another window's frame flushed an effect that wrote here: that flush serviced the
    /// wake, so nothing else will ever ask this window for the frame that shows it.
    pub(crate) fn owes_frame_for_document(&self) -> bool {
        self.dom.revision() != self.serviced_revision.get()
    }

    /// Records that a frame has serviced everything the document holds.
    pub(crate) fn serviced_document(&self) {
        self.serviced_revision.set(self.dom.revision());
    }

    /// Installs a downstream script engine on this window.
    pub fn install_binding(&mut self, binding: Box<dyn HostBinding>) {
        self.binding = binding;
    }

    /// Installs the host that fills this window's `surface` elements.
    ///
    /// One slot, like the frame binding above it: a window has one embed host and the host
    /// multiplexes its producers. Installing one over another replaces it without ceremony, which
    /// is the same contract `install_binding` states.
    pub fn install_embed_host(&mut self, host: Box<dyn crate::embed::EmbedHost>) {
        self.embed.shutting_down();
        self.embed = host;
        // Whatever the new host will show, the old host's attachments are stale now.
        self.damage = DamageSet::full();
        self.request_frame();
    }

    /// Installs the two halves of the registry custom elements are answered from.
    ///
    /// Together, because they are one registry seen from two stages: a box measured by one
    /// implementation and painted by another is the incoherence the pairing exists to prevent.
    pub fn install_custom_sources(
        &mut self,
        layout: Box<dyn zgui_layout::custom::CustomLayoutSource>,
        paint: Box<dyn zgui_paint::content::custom::CustomPaintSource>,
    ) {
        self.custom_layout = Some(layout);
        self.custom_paint = Some(paint);
        self.damage = DamageSet::full();
        self.request_frame();
    }

    /// What is being drawn into.
    pub fn surface(&self) -> &Arc<dyn Surface> {
        &self.surface
    }

    /// Which window this is, as the view layer numbers documents.
    pub fn document_id(&self) -> zgui_view::DocumentId {
        self.dom.document_id()
    }

    /// The rule set and the device this window is styled against.
    pub fn style_engine(&self) -> &StyleEngine {
        &self.engine
    }

    /// The tree this window draws.
    pub fn document(&self) -> &Rc<RefCell<Document>> {
        &self.document
    }

    /// The node-tree seam over it.
    pub fn dom(&self) -> &Rc<DocumentDom> {
        &self.dom
    }

    /// Whether the surface is starved: the compositor stopped handing over buffers.
    pub fn is_starved(&self) -> bool {
        self.starved
    }

    /// The engine seam a view asks its geometry through.
    pub fn host(&self) -> &Rc<RuntimeHost> {
        &self.host
    }

    /// The handle a view holds for this window's node tree.
    pub fn dom_handle(&self) -> &DomHandle {
        &self.dom_handle
    }

    /// The handle a view holds for this window's engines.
    pub fn host_handle(&self) -> &HostHandle {
        &self.host_handle
    }

    /// Asks for a frame, unless one has already been asked for and has not run yet.
    ///
    /// Every one of the runtime's own requests goes through here: an event that arrived, a wake
    /// from another thread, a deadline the park found had already passed, and the frame's own last
    /// phase. Asking twice for the same frame is not wrong, but it is indistinguishable from two
    /// things each having a reason to ask, which is the thing worth being able to tell.
    pub fn request_frame(&self) {
        if !self.awaiting_frame.replace(true) {
            zgui_profile::latency::mark("req.redraw");
            self.surface.request_redraw();
        }
    }

    /// Takes whatever an editing command asked to be put on the clipboard.
    ///
    /// Taken rather than read, so that the same cut is never written twice: the second write would
    /// replace whatever the user copied in between with text they had already pasted.
    pub fn take_clipboard(&mut self) -> Vec<String> {
        core::mem::take(&mut self.clipboard)
    }

    /// Takes the elements whose paste requests are waiting for the clipboard's text.
    pub fn take_paste_requests(&mut self) -> Vec<zgui_dom::NodeKey> {
        core::mem::take(&mut self.wants_paste)
    }

    /// Hands a paste request the text it was waiting for; the next frame types it in.
    pub fn paste(&mut self, node: zgui_dom::NodeKey, text: String) {
        self.pastes.push((node, text));
    }

    /// Records that the frame that was asked for is now running.
    pub(crate) fn frame_started(&mut self) {
        self.awaiting_frame.set(false);
    }

    /// Whether a frame offered at `now` is worth running.
    ///
    /// A redraw request is not always this window's own. A windowing backend turns a configure into
    /// one whether or not anybody asked — winit's Wayland loop sets the redraw flag on the window
    /// the moment the compositor resizes it — so a window that has decided a configure is too soon
    /// to answer is handed the frame anyway.
    ///
    /// Declining it is what makes that decision real. Nothing is built and nothing is thrown away:
    /// no layout, no repaint, and above all no swapchain rebuild, which waits for the device to go
    /// idle. What is owed stays owed, and [`Window::merged_deadline`] is what comes back for it.
    ///
    /// **Who asked does not enter into it.** A frame that reconfigures the surface pays the same
    /// stall whether a configure, a pointer sample, an animation tick or a task finishing on
    /// another thread asked for it — so while a reconfiguration is owed and could not yet be seen,
    /// every one of them is refused alike. Pacing only the configures leaves the whole design
    /// conditional on nothing else happening during a drag, and a drag is the one time something
    /// always is: a resize that arrives beside a pointer stream then rebuilds the swapchain once
    /// per pointer sample, which is the rate the compositor delivers rather than the rate the
    /// output can show.
    ///
    /// A window that owes no reconfiguration is never refused anything, which is what keeps this
    /// out of the path of an ordinary click.
    pub fn wants_a_frame(&self, now: Instant) -> bool {
        !self.reconfigure || !self.pace.too_soon(now, self.refresh_interval())
    }

    /// Records that an offered frame was refused, so that the deadline can ask for it again.
    ///
    /// Dropping the request is the whole of it. [`Window::request_frame`] is idempotent while a
    /// frame is outstanding, so a request left standing after the frame it named was refused is a
    /// request nothing can renew — the deadline arrives, asks, is told one is already on its way,
    /// and the window stops redrawing until something unrelated happens. Refusing a frame and
    /// forgetting that one was asked for are therefore the same act.
    pub fn declined_a_frame(&mut self) {
        self.declined += 1;
        self.awaiting_frame.set(false);
        zgui_profile::latency::mark("f.declined");
    }

    /// Whether a frame offered at `now` is being held back so that it starts closer to being shown.
    ///
    /// A frame is worth running long before it is worth *starting*. Started the moment it is asked
    /// for, it composes the world as it was when the last buffer was released and then waits inside
    /// the acquisition — on the thread that reads input — for the display to release the next one.
    /// Everything that arrives during that wait is answered by the frame after this one.
    ///
    /// So the frame is held, the events keep arriving and keep being queued, and the one frame that
    /// runs when the hold expires is built from all of them. Nothing is skipped and nothing is
    /// presented later: what moves is how old the picture is when it appears, and how much of each
    /// interval the loop can answer a pointer in. See [`PresentPace`](crate::PresentPace) for how
    /// long the hold is and when there is none.
    ///
    /// Asking is what starts the hold, so a caller that asks must be prepared to act on the answer.
    pub fn holds_a_frame(&self, now: Instant) -> bool {
        self.present.holds_a_frame(now)
    }

    /// How long the next frame this window is asked for is held back before it starts.
    ///
    /// Zero for a window that has never been made to wait for a surface to present into, which is
    /// every window presenting to something with an image always spare.
    pub const fn present_hold(&self) -> std::time::Duration {
        self.present.hold()
    }

    /// Records that an offered frame was held back, so that the deadline can ask for it again.
    ///
    /// The same act as [`Window::declined_a_frame`] and for the same reason: a request left
    /// standing is a request nothing can renew, so forgetting one was asked for is part of not
    /// answering it.
    pub fn held_a_frame(&mut self) {
        self.held += 1;
        self.awaiting_frame.set(false);
        zgui_profile::latency::mark("f.held");
    }

    /// How many offered frames were held back to start closer to the moment they would be shown.
    ///
    /// Nothing was skipped: each of these ran a little later in the same interval. It counts
    /// offers rather than frames because a burst of input inside one hold offers several and they
    /// all become the one frame that runs at the end of it.
    pub const fn held_frames(&self) -> u64 {
        self.held
    }

    /// How many offered frames were refused for a reconfiguration that could not yet be seen.
    ///
    /// Each one is a layout, a full-surface repaint and a swapchain rebuild that a drag did not
    /// pay for. It counts refusals rather than configures because the two differ by exactly the
    /// thing worth seeing: a backend produces a redraw per configure on its own account, and
    /// anything else happening at the same time produces more.
    pub const fn declined_frames(&self) -> u64 {
        self.declined
    }

    /// Whether the surface is entirely hidden.
    pub fn is_occluded(&self) -> bool {
        self.occluded
    }

    /// How many configures moved the window's size without a pipeline run of their own.
    ///
    /// A resize is a level rather than a stream of events, so a configure that arrives before the
    /// last one could have reached the screen is not answered with a frame: it moves the level, and
    /// the frame that eventually runs is built for whatever the window is by then. This counts the
    /// layouts, repaints and swapchain rebuilds that were skipped because their result would have
    /// been superseded before anything could have been seen.
    pub const fn deferred_resizes(&self) -> u64 {
        self.pace.deferred()
    }

    /// How many times the renderer has been pointed at a new surface extent.
    ///
    /// This is the count of the expensive half of a resize, and it is what separates a window that
    /// coalesces configures from one that merely asks for fewer frames. Rebuilding a swap chain
    /// waits for the graphics device to go completely idle before it can begin, so it costs the
    /// same whether the window is being dragged by a pixel or across a monitor — and one per
    /// configure, at the rate a compositor delivers them, is a queue that drains more slowly than
    /// the drag that fills it.
    ///
    /// Counted rather than derived because nothing else can see it: a window that reconfigures on
    /// every configure and one that reconfigures once per frame of the output lay out the same
    /// document, present the same picture and differ only in how much work they threw away.
    pub const fn surface_configures(&self) -> u64 {
        self.configured
    }

    /// How long one frame of the output this window is on lasts.
    ///
    /// The rate a resize is answered at, and the rate an animation ticks at, are both this — so it
    /// is read from the surface rather than assumed, and a window that has been dragged onto
    /// another output is paced by that output from the next frame onwards.
    ///
    /// A backend that reports no rate is answered with sixty hertz, and that fallback paces
    /// everything a window does: a resize step, an animation tick, a held frame. Logged when it
    /// changes, because a fast display silently paced at sixty is indistinguishable from a fast
    /// display that is simply slow, and there is nothing else that would ever say which.
    pub fn refresh_interval(&self) -> std::time::Duration {
        // What the platform measured, where it measures: an interval taken from the presentation
        // of this surface's own frames is the output it is actually on, restated every frame,
        // where a rate is whatever the surface was last told about.
        if let Some(measured) = self
            .surface
            .presentation_timing()
            .and_then(|timing| timing.interval)
            .filter(|interval| !interval.is_zero())
        {
            return measured;
        }
        let rate = self.surface.refresh_rate_millihertz();
        if self.reported_rate.replace(Some(rate)) != Some(rate) {
            match rate {
                Some(rate) => tracing::debug!(millihertz = rate, "output refresh rate"),
                None => tracing::info!(
                    "the output reports no refresh rate; pacing this window at sixty hertz"
                ),
            }
        }
        zgui_platform::refresh_interval(rate)
    }

    /// How many device pixels there are to a CSS pixel.
    pub fn scale(&self) -> Scale<zgui_geom::Css, zgui_geom::Device> {
        Scale::new(self.scale)
    }

    /// The display list the last frame built.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// What every coordinate system in the drawn frame resolves to.
    ///
    /// The answers, not the tree: a rectangle measured inside a transformed subtree is where a
    /// person sees it only after the matrix its coordinate system resolves to has been applied,
    /// and these are the matrices the frame on the screen was composed through. A caller holding a
    /// fragment's rectangle and its coordinate system turns the two into a place on the surface
    /// with [`onto_device`](zgui_layout::fragment::transform::placed::onto_device).
    pub fn placements(&self) -> std::cell::Ref<'_, zgui_scene::Placements> {
        self.host.placements()
    }

    /// What the last frame reported as needing redrawing.
    pub fn damage(&self) -> &DamageSet {
        &self.damage
    }

    /// The renderer, for a caller that wants to ask it something directly.
    pub fn renderer(&self) -> &dyn Renderer {
        self.renderer.as_ref()
    }

    /// The raster paths most recently selected by `node`'s own vector content.
    pub fn vector_routes(&self, node: zgui_dom::NodeKey) -> zgui_paint::VectorRoutes {
        self.vector_routes
            .get(&node)
            .copied()
            .unwrap_or(zgui_paint::VectorRoutes::NONE)
    }

    /// Every vector raster path selected by `node` or one of its descendants.
    ///
    /// This is what lets an inspector answer for a `span` wrapping an icon rather than reporting
    /// only on the child generated for the drawing itself.
    pub fn vector_routes_in_subtree(&self, node: zgui_dom::NodeKey) -> zgui_paint::VectorRoutes {
        let document = self.document.borrow();
        let Some(root) = document.store().index_of(node) else {
            return zgui_paint::VectorRoutes::NONE;
        };
        let mut routes = zgui_paint::VectorRoutes::NONE;
        let mut stack = vec![root];
        while let Some(index) = stack.pop() {
            routes.union_with(self.vector_routes(document.store().key_of(index)));
            let mut child = document.store().core(index).first_child();
            while let Some(index) = child {
                stack.push(index);
                child = document.store().core(index).next_sibling();
            }
        }
        routes
    }

    /// The elements in the frame that caused Vello's lazy initialization.
    ///
    /// Keys are retained even if an element is later removed, so the inspector can distinguish a
    /// vanished cause from no recorded cause. Generation checks prevent a replacement node from
    /// being mistaken for the original.
    pub fn vello_initializers(&self) -> &[zgui_dom::NodeKey] {
        &self.vello_initializers
    }

    /// The boxes, their results and their fragments.
    pub fn layout(&self) -> &Rc<RefCell<LayoutStore>> {
        &self.layout
    }

    /// Where every scroll container in this window is scrolled to.
    pub fn scroll(&self) -> &Rc<RefCell<zgui_scroll::Scroller>> {
        &self.scroll
    }

    /// The glyph tiles and decoded images this window draws from.
    ///
    /// This is where an application attaches a decoded picture to a replaced node, and where a
    /// budget assertion asks how many tiles a document's text actually cost.
    pub fn content(&self) -> &ContentCache {
        &self.content
    }

    /// The same, for attaching content to a replaced node.
    pub fn content_mut(&mut self) -> &mut ContentCache {
        &mut self.content
    }

    /// Turns on the check that every replayed range still owns the rasters it draws.
    ///
    /// On by default only when `ZGUI_INVARIANTS` is set, because it costs a lookup per distinct
    /// raster per replayed fragment — a per-frame cost proportional to what the replay saved. A
    /// test about the check itself turns it on directly, rather than depending on how the process
    /// was launched.
    pub fn set_verify_replays(&mut self, verify: bool) {
        self.verify_replays = verify;
    }

    /// Turns on the check that every primitive is still drawn through the coordinate system it was
    /// pushed under.
    ///
    /// On by default only when `ZGUI_INVARIANTS` is set, because it costs a word of storage per
    /// primitive per frame and a lookup per primitive to answer. A test about the check itself
    /// turns it on directly, rather than depending on how the process was launched — and until
    /// something could, the check was wired into the frame loop with nothing able to watch it run
    /// there.
    ///
    /// It takes effect on the next frame: the names are kept alongside the log and are meaningless
    /// unless they have been kept since that log began.
    pub fn set_check_spatial_dependencies(&mut self, check: bool) {
        self.check_spatial_dependencies = check;
    }

    /// How many of this frame's primitives carry the name of a coordinate system.
    ///
    /// Zero for a window that was never asked to keep them, and the non-vacuity control for any
    /// test that watches the check pass: a frame that recorded nothing has nothing to compare, and
    /// [`Scene::check_spatial_dependencies`](zgui_scene::Scene::check_spatial_dependencies) is
    /// perfectly happy about it.
    pub fn spatial_dependencies_recorded(&self) -> usize {
        self.scene.spatial_dependencies_recorded()
    }

    /// Every raster a live paint record names that the content cache no longer holds.
    ///
    /// Empty for a window whose records own what they draw, and that is the whole assertion: a key
    /// here names a rectangle of a texture some fragment is going to replay out of and something
    /// else is now free to write into. It is the state no other observation of the window can see
    /// — the display list is what it was, the geometry never moved, and the pixels are wrong only
    /// once the rectangle has been handed out and filled.
    pub fn stale_replay_resources(&self) -> Vec<zgui_atlas::AtlasKey> {
        self.painter
            .cache()
            .resources()
            .filter(|key| !self.content.atlas().contains(*key))
            .collect()
    }

    /// Unmounts the view and drops everything the window was holding on its behalf.
    pub fn close(&mut self) {
        self.binding.shutting_down();
        self.embed.shutting_down();
        self.view = None;
        self.timers.borrow_mut().forget(self.dom.document_id());
        if let Some(scope) = self.scope.take() {
            scope.unmount();
        }
        self.dom.end_frame();
    }
}

impl Window {
    /// Hands this window the application's cascade pool.
    pub(crate) fn set_style_pool(&mut self, pool: Rc<zgui_style::engine::thread_pool::StylePool>) {
        self.style_pool = Some(pool);
    }

    /// Hands this window the application's layout pool.
    pub(crate) fn set_layout_pool(
        &mut self,
        pool: std::sync::Arc<zgui_layout::tree::parallel::LayoutPool>,
    ) {
        self.layout_pool = Some(pool);
    }
}

impl crate::dispatch::Handlers for Window {
    fn handler(
        &self,
        id: zgui_dom::side::listeners::ListenerId,
    ) -> Option<crate::dispatch::Handler> {
        self.dom.handler(id)
    }
}
