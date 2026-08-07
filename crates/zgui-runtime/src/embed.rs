//! The seam an embedded-renderer host plugs into the frame loop through.
//!
//! A `surface` element shows a texture some other renderer produced — a game, a video decoder, a
//! one-shot rasteriser. Everything device-specific about that lives in a companion crate for the
//! render backend; what the *runtime* owns is when that work is allowed to touch the frame, and
//! this trait is that moment. It has the same shape as [`HostBinding`](crate::binding::HostBinding):
//! installed per window, called at one fixed point, no other entry into the loop.
//!
//! # Where the sync step sits, and why
//!
//! After layout and the caret plan, before paint emission. After layout because the host binds
//! producers to *laid-out* boxes — a producer is told the device-pixel size its element actually
//! got, and a texture attached this step is attached to a content box that will not move again
//! this frame. Before paint for the same reason the caret plans there: the damage the host absorbs
//! has to be in the set the emit walk is gated on, or a fresh texture sits behind a frame that
//! never redraws its rectangle.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_dom::Document;
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::ContentCache;
use zgui_render::Renderer;
use zgui_vocab::Timestamp;

use crate::replaced::IntrinsicTable;
use crate::wake::RuntimeWaker;

/// Everything one sync step may touch.
///
/// The borrows are one frame stage's: nothing here may be kept. The document and the layout store
/// arrive as shared cells because a host needs each for moments — a walk over the surface nodes, a
/// mark on one that changed — and holding either across the renderer work would hold the whole
/// frame's state exclusively for no reason.
pub struct EmbedSyncCx<'a> {
    /// The document, for reading tokens off surface nodes and marking the ones whose content
    /// changed shape through
    /// [`Document::replaced_content_changed`](zgui_dom::Document::replaced_content_changed).
    pub document: &'a Rc<RefCell<Document>>,
    /// The laid-out boxes, for the content box a producer's texture is stretched over.
    pub layout: &'a Rc<RefCell<LayoutStore>>,
    /// The renderer, for registering and releasing externals — and, through
    /// [`Renderer::as_any_mut`], for whatever the backend companion needs that the contract
    /// cannot name.
    pub renderer: &'a mut dyn Renderer,
    /// Where a surface node's external texture is attached for the emit walk to find.
    pub content: &'a mut ContentCache,
    /// The frame's damage, still open: a host that attached new pixels absorbs their rectangle.
    pub damage: &'a mut DamageSet,
    /// The intrinsics slot surface nodes answer layout from.
    pub intrinsics: &'a Arc<IntrinsicTable>,
    /// How many batches of changes the document has taken, ever.
    ///
    /// A host that remembers the last reading it scanned at re-walks the document only when this
    /// moved: which elements exist and which producers they name can only change inside a batch,
    /// while presents and renders arrive without one.
    pub revision: u64,
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Whether the window is currently invisible, which is what pauses producers.
    pub occluded: bool,
    /// The frame's own clock reading, the same one animations are stepped with.
    pub timestamp: Timestamp,
    /// The thread-safe wake route a host hands to producers.
    pub waker: &'a Arc<RuntimeWaker>,
}

impl EmbedSyncCx<'_> {
    /// Records that `node`'s replaced content changed shape, and buys the frame that shows it.
    ///
    /// Two operations, and the pairing is the point: the sync step runs *after* this frame's
    /// layout, so a mark made here is consumed by the next frame — and a mark with no wake is a
    /// next frame that never comes. The wake folds into "another frame is owed" rather than
    /// pinging the platform, because a frame is exactly what is running.
    pub fn replaced_content_changed(&self, node: zgui_dom::NodeKey) {
        let mut document = self.document.borrow_mut();
        if let Some(index) = document.store().index_of(node) {
            document.replaced_content_changed(index);
        }
        drop(document);
        zgui_reactive::FrameWaker::wake(&**self.waker);
    }
}

/// What one sync step tells the loop.
#[derive(Default, Clone, Copy, Debug)]
pub struct EmbedSyncReport {
    /// Whether any embed wants a frame per refresh — a continuous callback, a producer that
    /// declared a cadence. Feeds the window's animation gate, never a direct frame request:
    /// requesting from inside the frame is the machine-rate spin the loop exists to prevent.
    pub animating: bool,
}

/// A per-window host for embedded producers.
///
/// Installed with [`Window::install_embed_host`](crate::window::Window::install_embed_host);
/// the default is [`NoEmbeds`], which is a window saying surface elements in it show nothing.
pub trait EmbedHost {
    /// Runs the frame's embed work; see the module for where this sits and why.
    fn sync(&mut self, cx: &mut EmbedSyncCx<'_>) -> EmbedSyncReport;

    /// The window is going away; release everything and tell every producer.
    fn shutting_down(&mut self) {}
}

/// The host a window has until an application installs one.
pub struct NoEmbeds;

impl EmbedHost for NoEmbeds {
    fn sync(&mut self, _cx: &mut EmbedSyncCx<'_>) -> EmbedSyncReport {
        EmbedSyncReport::default()
    }
}
