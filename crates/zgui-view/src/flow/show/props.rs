//! Writing a conditional as a tag.
//!
//! A view written as tags reaches a component through a props struct and a builder for it, and
//! this is that pair for [`Show`].

use core::marker::PhantomData;

use crate::flow::given::{HasShown, HasWhen, Set, Unset};
use crate::flow::show::Show;
use crate::view::{AnyView, ChildrenFn, IntoView};

/// The props of [`Show`].
///
/// ```
/// # use zgui_reactive::prelude::*;
/// # use zgui_reactive::{Mounted, RwSignal, install};
/// # use zgui_view::{AnyView, IntoView, ShowProps};
/// # install().ok();
/// # let window = Mounted::new();
/// let open = window.with(|| RwSignal::new(false));
/// let conditional = ShowProps::builder()
///     .when(move || open.get())
///     .children(|| AnyView::new("open"))
///     .fallback(|| AnyView::new("closed"))
///     .build()
///     .render();
/// # let _ = conditional.into_view();
/// # window.unmount();
/// ```
pub struct ShowProps {
    /// The condition, read inside one effect.
    when: Box<dyn Fn() -> bool>,
    /// What is shown while it holds.
    children: ChildrenFn,
    /// What is shown while it does not.
    fallback: Option<ChildrenFn>,
}

impl ShowProps {
    /// A builder for these props, with nothing given yet.
    pub fn builder() -> ShowPropsBuilder {
        ShowPropsBuilder {
            when: None,
            children: None,
            fallback: None,
            given: PhantomData,
        }
    }

    /// Whether [`Show`] takes slot children.
    #[doc(hidden)]
    pub const ACCEPTS_SLOTS: bool = false;

    /// Builds the conditional these props describe.
    pub fn render(self) -> impl IntoView {
        let children = self.children;
        let shown = Show::new(self.when, move || children.view());
        match self.fallback {
            Some(fallback) => shown.fallback(move || fallback.view()),
            None => shown,
        }
    }
}

/// A [`ShowProps`] under construction.
///
/// Each required prop is a type parameter, flipped from [`Unset`] to [`Set`] by its own setter, so
/// that leaving one out is a compile error naming the prop rather than a branch that never shows.
pub struct ShowPropsBuilder<W = Unset, C = Unset> {
    /// The condition.
    when: Option<Box<dyn Fn() -> bool>>,
    /// What is shown while it holds.
    children: Option<ChildrenFn>,
    /// What is shown while it does not.
    fallback: Option<ChildrenFn>,
    /// Which props have been given.
    given: PhantomData<(W, C)>,
}

impl<W, C> ShowPropsBuilder<W, C> {
    /// The condition.
    ///
    /// It is the *answer* that is compared, not the signals it was computed from, so a condition
    /// written `move || items.get().is_empty()` swaps the branch when the collection becomes empty
    /// and does nothing at all when a row is added to one that was already full.
    pub fn when(self, when: impl Fn() -> bool + 'static) -> ShowPropsBuilder<Set, C> {
        ShowPropsBuilder {
            when: Some(Box::new(when)),
            children: self.children,
            fallback: self.fallback,
            given: PhantomData,
        }
    }

    /// What is shown while the condition holds.
    pub fn children<V: IntoView + 'static>(
        self,
        children: impl Fn() -> V + 'static,
    ) -> ShowPropsBuilder<W, Set> {
        ShowPropsBuilder {
            when: self.when,
            children: Some(ChildrenFn::new(move || AnyView::new(children()))),
            fallback: self.fallback,
            given: PhantomData,
        }
    }

    /// What is shown while it does not, which is nothing unless this says otherwise.
    #[must_use]
    pub fn fallback<V: IntoView + 'static>(mut self, fallback: impl Fn() -> V + 'static) -> Self {
        self.fallback = Some(ChildrenFn::new(move || AnyView::new(fallback())));
        self
    }
}

impl<W, C> ShowPropsBuilder<W, C>
where
    W: HasWhen,
    C: HasShown,
{
    /// The props, once every required one has been given.
    pub fn build(self) -> ShowProps {
        ShowProps {
            when: self.when.expect("`when` is set on a builder that has it"),
            children: self
                .children
                .expect("the children are set on a builder that has them"),
            fallback: self.fallback,
        }
    }
}
