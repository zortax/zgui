//! Writing a keyed list as a tag.
//!
//! A view written as tags reaches a component through a props struct and a builder for it, and
//! this is that pair for [`For`]. It is written out rather than generated from a function, because
//! the list is generic in ways a function's own arguments cannot state: what the collection holds
//! and what identifies a row are decided by the closures, not by the call.

use core::hash::Hash;
use core::marker::PhantomData;

use crate::flow::each::For;
use crate::flow::given::{HasEach, HasKey, HasRow, Set, Unset};
use crate::view::{AnyView, IntoView};

/// What produces the collection.
type Each<T> = Box<dyn Fn() -> Vec<T>>;

/// What identifies a row.
type Key<T, K> = Box<dyn Fn(&T) -> K>;

/// What builds a row.
type Row<T> = Box<dyn Fn(T) -> AnyView>;

/// The props of [`For`].
///
/// ```
/// # use zgui_reactive::prelude::*;
/// # use zgui_reactive::{Mounted, RwSignal, install};
/// # use zgui_view::{AnyView, ForProps, IntoView};
/// # install().ok();
/// # let window = Mounted::new();
/// let rows = window.with(|| RwSignal::new(vec![1_i32, 2, 3]));
/// let list = ForProps::builder()
///     .each(move || rows.get())
///     .key(|row: &i32| *row)
///     .children(move |row| AnyView::new(row.to_string()))
///     .build()
///     .render();
/// # let _ = list.into_view();
/// # window.unmount();
/// ```
pub struct ForProps<T, K> {
    /// Produces the collection, and is the list's only reactive dependency.
    each: Each<T>,
    /// Identifies a row.
    key: Key<T, K>,
    /// Builds a row.
    children: Row<T>,
}

impl<T: 'static, K: Eq + Hash + Clone + 'static> ForProps<T, K> {
    /// A builder for these props, with nothing given yet.
    pub fn builder() -> ForPropsBuilder<T, K> {
        ForPropsBuilder {
            each: None,
            key: None,
            children: None,
            given: PhantomData,
        }
    }

    /// Whether [`For`] takes slot children.
    #[doc(hidden)]
    pub const ACCEPTS_SLOTS: bool = false;

    /// Builds the list these props describe.
    pub fn render(self) -> impl IntoView {
        For::new(self.each, self.key, self.children)
    }
}

/// A [`ForProps`] under construction.
///
/// Each required prop is a type parameter, flipped from [`Unset`] to [`Set`] by its own setter, so
/// that leaving one out is a compile error naming the prop rather than a panic when the list runs.
pub struct ForPropsBuilder<T, K, E = Unset, KF = Unset, VF = Unset> {
    /// Produces the collection.
    each: Option<Each<T>>,
    /// Identifies a row.
    key: Option<Key<T, K>>,
    /// Builds a row.
    children: Option<Row<T>>,
    /// Which props have been given.
    given: PhantomData<(E, KF, VF)>,
}

impl<T: 'static, K, E, KF, VF> ForPropsBuilder<T, K, E, KF, VF> {
    /// The collection, which is the list's only reactive dependency.
    ///
    /// It is re-read when what it reads changes, and the rows are brought into line with the keys
    /// it produced. A row's own content follows that row's own signals and is not touched here.
    pub fn each<I: IntoIterator<Item = T>>(
        self,
        each: impl Fn() -> I + 'static,
    ) -> ForPropsBuilder<T, K, Set, KF, VF> {
        ForPropsBuilder {
            each: Some(Box::new(move || each().into_iter().collect())),
            key: self.key,
            children: self.children,
            given: PhantomData,
        }
    }

    /// What identifies a row, and therefore which row is which after the collection changes.
    pub fn key(self, key: impl Fn(&T) -> K + 'static) -> ForPropsBuilder<T, K, E, Set, VF>
    where
        K: 'static,
    {
        ForPropsBuilder {
            each: self.each,
            key: Some(Box::new(key)),
            children: self.children,
            given: PhantomData,
        }
    }

    /// What one row looks like.
    pub fn children<V: IntoView + 'static>(
        self,
        children: impl Fn(T) -> V + 'static,
    ) -> ForPropsBuilder<T, K, E, KF, Set> {
        ForPropsBuilder {
            each: self.each,
            key: self.key,
            children: Some(Box::new(move |row| AnyView::new(children(row)))),
            given: PhantomData,
        }
    }
}

impl<T, K, E, KF, VF> ForPropsBuilder<T, K, E, KF, VF>
where
    E: HasEach,
    KF: HasKey,
    VF: HasRow,
{
    /// The props, once every required one has been given.
    pub fn build(self) -> ForProps<T, K> {
        ForProps {
            each: self.each.expect("`each` is set on a builder that has it"),
            key: self.key.expect("`key` is set on a builder that has it"),
            children: self
                .children
                .expect("the children are set on a builder that has them"),
        }
    }
}
