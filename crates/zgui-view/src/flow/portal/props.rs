//! Writing a portal as a tag.
//!
//! A view written as tags reaches a component through a props struct and a builder for it, and
//! this is that pair for [`Portal`]. Without it a portal could only be written as a value in a
//! block — which is exactly the wrong way round, because the thing most often portalled is a
//! subtree of ordinary tags.

use core::marker::PhantomData;

use crate::dom::OverlayLayer;
use crate::flow::given::{HasShown, Set, Unset};
use crate::flow::portal::Portal;
use crate::view::{AnyView, ChildrenFn, IntoView};

/// The props of [`Portal`].
///
/// ```
/// # use zgui_reactive::{Mounted, install};
/// # use zgui_view::{AnyView, IntoView, OverlayLayer, PortalProps};
/// # install().ok();
/// # let window = Mounted::new();
/// let floating = PortalProps::builder()
///     .layer(OverlayLayer::Modal)
///     .children(|| AnyView::new("in the overlay"))
///     .build()
///     .render();
/// # let _ = floating.into_view();
/// # window.unmount();
/// ```
pub struct PortalProps {
    /// Which band the content goes on.
    layer: OverlayLayer,
    /// The content.
    children: ChildrenFn,
}

impl PortalProps {
    /// A builder for these props, with nothing given yet.
    pub fn builder() -> PortalPropsBuilder {
        PortalPropsBuilder {
            layer: OverlayLayer::default(),
            children: None,
            given: PhantomData,
        }
    }

    /// Whether [`Portal`] takes slot children.
    #[doc(hidden)]
    pub const ACCEPTS_SLOTS: bool = false;

    /// Builds the portal these props describe.
    pub fn render(self) -> impl IntoView {
        let children = self.children;
        Portal::new(move || children.view()).layer(self.layer)
    }
}

/// A [`PortalProps`] under construction.
///
/// The children are a type parameter, flipped from [`Unset`] to [`Set`] by their setter, so that a
/// portal with nothing in it is a compile error rather than an overlay band that silently gains an
/// empty subtree.
pub struct PortalPropsBuilder<C = Unset> {
    /// Which band the content goes on.
    layer: OverlayLayer,
    /// The content.
    children: Option<ChildrenFn>,
    /// Which props have been given.
    given: PhantomData<C>,
}

impl<C> PortalPropsBuilder<C> {
    /// Which band the content goes on. The popover band unless this says otherwise.
    #[must_use]
    pub fn layer(mut self, layer: OverlayLayer) -> Self {
        self.layer = layer;
        self
    }

    /// What is rendered on the overlay band.
    pub fn children<V: IntoView + 'static>(
        self,
        children: impl Fn() -> V + 'static,
    ) -> PortalPropsBuilder<Set> {
        PortalPropsBuilder {
            layer: self.layer,
            children: Some(ChildrenFn::new(move || AnyView::new(children()))),
            given: PhantomData,
        }
    }
}

impl<C: HasShown> PortalPropsBuilder<C> {
    /// The props, once the children have been given.
    pub fn build(self) -> PortalProps {
        PortalProps {
            layer: self.layer,
            children: self
                .children
                .expect("the children are set on a builder that has them"),
        }
    }
}
