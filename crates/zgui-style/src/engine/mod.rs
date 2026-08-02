//! The style engine over one document: the rule set, the device, and the frame phases that drive
//! them.
//!
//! | Module | Contents |
//! |---|---|
//! | [`stylist`] | the rule set, and the root-metrics fixpoint |
//! | [`guards`] | the read guard every sheet and every restyle is taken under |
//! | [`thread_pool`] | the workers a restyle may run on, and the ceiling |
//! | [`restyle`] | styling the document once, and what that leaves the frame owing |
//! | [`animate`] | advancing the document's animations one frame |

pub mod animate;
pub mod guards;
pub mod restyle;
pub mod stylist;
pub mod thread_pool;

use std::sync::Arc;

use style::shared_lock::SharedRwLock;
use style::stylist::Stylist;
use zgui_dom::Document;
use zgui_text::FontMetricsSource;

pub use crate::engine::restyle::{TextPaintUpdate, TextRun};

use crate::damage::layout_damage::TextKeyStore;
use crate::deps::{StyleDependencies, StyleFilterView};
use crate::device::{self, DeviceEpoch, Viewport};
use crate::driver;
use crate::driver::animations::Animations;
use crate::sheets::errors::CssDiagnostics;
use crate::sheets::origin::SheetOrigin;
use crate::sheets::set::SheetHandle;
use crate::sheets::ua::USER_AGENT_SHEET;
use crate::sheets::{SheetSource, Sheets};

/// The style engine over one document.
///
/// It is not shareable and not sendable, and that is a consequence rather than an oversight: it
/// holds the rule set, which holds a device, which holds a font-metrics source whose lock is not
/// required to travel between threads. The restyle itself runs across a worker pool, which is a
/// different thing — the pool is handed to it for the duration of one traversal.
pub struct StyleEngine {
    /// The compiled rule set.
    stylist: Stylist,
    /// The lock shared with the document.
    lock: SharedRwLock,
    /// Where the cascade's font-metric answers come from, kept so that every device built for this
    /// document shares one memo.
    metrics: Arc<dyn FontMetricsSource>,
    /// The surface the current device was built for.
    viewport: Viewport,
    /// The installed sheets.
    sheets: Sheets,
    /// What the installed sheets can be affected by.
    deps: StyleDependencies,
    /// Each element's text keys, as of its last restyle.
    texts: TextKeyStore,
    /// The root-relative quantities units resolve against.
    root_metrics: stylist::RootMetrics,
    /// The text colours that changed in the last restyle.
    text_paint_updates: Vec<TextPaintUpdate>,
    /// The animations and transitions this document is running.
    ///
    /// Held here because the cascade both writes it — a transition is created from the difference
    /// between two cascade results — and reads it, and because it has to survive the frame that
    /// created it.
    animations: Animations,
    /// Whether some element is waiting for the animation-only traversal.
    ///
    /// The traversal that services those elements is a second, separate descent that no other
    /// input asks for, so a frame that would otherwise have nothing to do still has to run it —
    /// and a frame that has nothing waiting must not, because the descent is not free.
    animation_restyle_owed: bool,
    /// The user-agent sheet, held so that it stays installed.
    _user_agent: SheetHandle,
}

impl StyleEngine {
    /// A style engine for `document`, with this framework's user-agent sheet installed.
    ///
    /// `metrics` answers the cascade's font-metric questions and is shared with every device this
    /// engine builds afterwards, so its memo survives every resize.
    pub fn new(
        document: &Document,
        metrics: Arc<dyn FontMetricsSource>,
        viewport: Viewport,
    ) -> Self {
        // Every one of these flags is read at *parse* time, so a sheet parsed before they are set
        // silently loses those declarations rather than reporting them.
        zgui_css::enable_css_features();

        let mut stylist = stylist::new(device::build::build(viewport, &metrics));
        let mut sheets = Sheets::new();
        let lock = document.store().lock().clone();
        let (user_agent, dropped) = sheets.add(
            &mut stylist,
            &lock,
            document.store().host().sheets(),
            SheetOrigin::UserAgent,
            SheetSource::Text(USER_AGENT_SHEET),
        );
        debug_assert!(
            dropped.is_empty(),
            "this framework's own user-agent sheet must parse whole: {dropped:?}"
        );

        Self {
            stylist,
            lock,
            metrics,
            viewport,
            sheets,
            deps: StyleDependencies::unusable(),
            texts: TextKeyStore::new(),
            root_metrics: stylist::RootMetrics::default(),
            text_paint_updates: Vec::new(),
            animations: Animations::new(),
            animation_restyle_owed: false,
            _user_agent: user_agent,
        }
    }

    /// The animations and transitions this document is running.
    pub fn animations(&self) -> &Animations {
        &self.animations
    }

    /// The surface the current device was built for.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Rebuilds the device for `next` and invalidates what the change invalidates.
    ///
    /// Runs before anything else in a frame that follows a resize, a monitor change or a theme
    /// flip, and before the restyle, because which rules apply is decided by the device.
    pub fn device_epoch(&mut self, document: &mut Document, next: Viewport) -> DeviceEpoch {
        let epoch = device::epoch(
            &mut self.stylist,
            &self.lock,
            document,
            &self.metrics,
            self.viewport,
            next,
        );
        self.viewport = next;
        epoch
    }

    /// Whether the installed sheets have changed since the last restyle.
    ///
    /// This is the frame's one style input that marks nothing on any node: adding, replacing or
    /// dropping a sheet, and a device change that re-matched a media query, all reach the document
    /// through here and nowhere else. A restyle gate that read only the document's own
    /// obligations would be false for every one of them, and a saved stylesheet would change
    /// nothing on screen.
    pub fn sheets_have_changed(&mut self) -> bool {
        self.remove_dropped_sheets();
        self.stylist.stylesheets_have_changed()
    }

    /// Disables the dependency filters if the sheet set changed, and reports whether they are now
    /// disabled.
    ///
    /// Runs before the mutations of a frame are applied. For the one frame in which the sheets
    /// changed, every mutation takes the full path, because the answers the filters give are built
    /// from an index that still describes the previous sheets. The tail of the same frame's
    /// restyle rebuilds them.
    pub fn disable_filters_if_sheets_changed(&mut self) -> bool {
        if self.sheets_have_changed() {
            self.deps.disable();
        }
        self.deps.is_disabled()
    }

    /// The filter the document's mutation API consults.
    pub fn filter(&self) -> StyleFilterView<'_> {
        StyleFilterView::new(&self.stylist, &self.deps)
    }

    /// What the installed sheets can be affected by.
    pub fn dependencies(&self) -> &StyleDependencies {
        &self.deps
    }

    /// Whether a restyle would do anything this frame.
    pub fn needs_restyle(&mut self, document: &Document) -> bool {
        self.animation_restyle_owed
            || driver::document_owes_restyle(document)
            || self.sheets_have_changed()
    }

    /// The text colours the last restyle changed.
    ///
    /// Empty unless a restyle moved an element's colour or text shadow. Whoever owns the paint
    /// table drains this and rewrites the named slots in place, in the same frame, so that a
    /// paragraph cached from an earlier frame resolves to the new colour without being shaped
    /// again.
    pub fn text_paint_updates(&self) -> &[TextPaintUpdate] {
        &self.text_paint_updates
    }

    /// Installs a sheet at the end of `origin`'s sheets.
    ///
    /// Never fails. An unrecognised declaration drops that declaration, a rejected selector drops
    /// that rule, an at-rule this build does not implement drops that block, and everything else
    /// in the sheet applies — so the diagnostics are the only place a dropped item is visible.
    /// A named source is resolved through the loader installed on the document.
    pub fn add_sheet(
        &mut self,
        document: &Document,
        origin: SheetOrigin,
        source: SheetSource<'_>,
    ) -> (SheetHandle, CssDiagnostics) {
        self.sheets.add(
            &mut self.stylist,
            &self.lock,
            document.store().host().sheets(),
            origin,
            source,
        )
    }

    /// The same, placing the sheet immediately before the one `before` names.
    ///
    /// # Panics
    ///
    /// Panics if `before` names no sheet installed in this engine.
    pub fn insert_sheet_before(
        &mut self,
        document: &Document,
        origin: SheetOrigin,
        source: SheetSource<'_>,
        before: &SheetHandle,
    ) -> (SheetHandle, CssDiagnostics) {
        self.sheets.insert_before(
            &mut self.stylist,
            &self.lock,
            document.store().host().sheets(),
            origin,
            source,
            before,
        )
    }

    /// Replaces the text of the sheet `handle` names, keeping its place in the cascade.
    ///
    /// Always replaces, even when the new text has errors: the valid rules apply and the dropped
    /// ones are reported. There is no failure state in which the old sheet survives.
    ///
    /// # Panics
    ///
    /// Panics if `handle` names no sheet installed in this engine.
    pub fn replace_sheet(
        &mut self,
        document: &Document,
        handle: &SheetHandle,
        source: SheetSource<'_>,
    ) -> CssDiagnostics {
        self.sheets.replace(
            &mut self.stylist,
            &self.lock,
            document.store().host().sheets(),
            handle,
            source,
        )
    }

    /// Removes every sheet whose handle has been dropped.
    fn remove_dropped_sheets(&mut self) {
        self.sheets.remove_dropped(&mut self.stylist, &self.lock);
    }
}
