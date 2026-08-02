//! What names a surface, and what describes it.

use zgui::prelude::*;

/// The heading and the explanation of one surface, published so the surface can point at them.
///
/// A dialog is named by its own title and described by its own description, and both of those are
/// written by the caller as children — somewhere below the element that has to name them. Nothing
/// in a retained tree lets a parent reach a descendant it did not create, so the surface publishes
/// two empty handles on the way down and the title and the description bind themselves to them on
/// the way up.
///
/// A surface with no title relates to nothing, which is the correct answer rather than a missing
/// one: an unbound handle names no node, and a reader is told the surface is unlabelled instead of
/// being sent to an element that is not there.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::overlay::SurfaceLabels;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let labels = SurfaceLabels::provide();
///     assert!(!labels.title().is_bound(), "nothing has written a title yet");
///     assert!(SurfaceLabels::current().is_some());
/// });
/// scope.unmount();
/// ```
#[derive(Copy, Clone, Default)]
pub struct SurfaceLabels {
    /// The element whose text names the surface.
    title: NodeRef,
    /// The element whose text explains it.
    description: NodeRef,
}

impl SurfaceLabels {
    /// A pair of handles nothing has bound yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pair and publishes it to every scope below this one.
    pub fn provide() -> Self {
        let labels = Self::new();
        provide_local_context(labels);
        labels
    }

    /// The pair the nearest enclosing surface published, when there is one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The element whose text names the surface.
    #[must_use]
    pub fn title(&self) -> NodeRef {
        self.title
    }

    /// The element whose text explains it.
    #[must_use]
    pub fn description(&self) -> NodeRef {
        self.description
    }
}
