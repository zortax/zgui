//! Panels with dividers a user can drag.

mod handle;
mod layout;
mod panel;
mod style;

pub use crate::resizable::handle::{ResizableHandle, ResizableHandleProps};
pub use crate::resizable::layout::{PanelBound, drag, normalise};
pub use crate::resizable::panel::{ResizablePanel, ResizablePanelProps};
pub use crate::resizable::style::ResizableStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::Orientation;

/// What the resizable group's rules are installed under.
pub(crate) const SHEET: &str = "zui-resizable";

/// One thing written inside a group, in the order it was written.
#[derive(Copy, Clone, PartialEq, Debug)]
enum Entry {
    /// A panel, with the share of the group it currently takes.
    Panel {
        /// What it is called inside the group.
        id: u64,
        /// How small and how large it may be.
        bound: PanelBound,
        /// What share of the group its caller asked for.
        ///
        /// Kept beside the current share, and it is what the group is shared out from when a panel
        /// comes or goes. Sharing out from the *current* sizes would compound: the first panel of
        /// two would be given the whole group on its own and then keep two thirds of it.
        declared: f64,
        /// What share of the group it takes now.
        size: f64,
    },
    /// A divider.
    Handle {
        /// What it is called inside the group.
        id: u64,
    },
}

/// What a panel reads to know its share, and a divider to know what it moves.
#[derive(Copy, Clone)]
pub struct ResizableContext {
    /// Which way the panels run.
    direction: Orientation,
    /// The panels and the dividers, in the order they were written.
    entries: RwSignal<Vec<Entry>, LocalStorage>,
    /// The next name to hand out.
    next: RwSignal<u64, LocalStorage>,
    /// The group's own element, which is what a drag is measured against.
    group: NodeRef,
    /// Told whenever the sizes settle somewhere new.
    on_change: StoredValue<Option<UnsyncCallback<Vec<f64>>>, LocalStorage>,
}

impl ResizableContext {
    /// The group the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which way the panels run.
    #[must_use]
    pub fn direction(self) -> Orientation {
        self.direction
    }

    /// The group's own element.
    #[must_use]
    pub fn group(self) -> NodeRef {
        self.group
    }

    /// Adds a panel to the end of the group, and takes it out again when its scope goes away.
    #[must_use]
    pub fn register_panel(self, bound: PanelBound, size: f64) -> u64 {
        let id = self.mint();
        self.entries.update(|entries| {
            entries.push(Entry::Panel {
                id,
                bound,
                declared: size,
                size,
            })
        });
        self.forget_on_cleanup(id);
        self.rebalance();
        id
    }

    /// Adds a divider at the point it was written.
    #[must_use]
    pub fn register_handle(self) -> u64 {
        let id = self.mint();
        self.entries
            .update(|entries| entries.push(Entry::Handle { id }));
        self.forget_on_cleanup(id);
        id
    }

    /// What share of the group the panel called `id` takes, as a percentage.
    #[must_use]
    pub fn size_of(self, id: u64) -> f64 {
        self.entries.with(|entries| {
            entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Panel {
                        id: found, size, ..
                    } if *found == id => Some(*size),
                    _ => None,
                })
                .unwrap_or(0.0)
        })
    }

    /// The panel a divider takes from, and how far it can be moved, as a percentage.
    ///
    /// The three numbers a splitter announces: where it is, and the two ends of its travel. They
    /// are the *panel before the divider*'s share and bounds, because that is the one thing a
    /// divider's position can be stated as without inventing a coordinate.
    #[must_use]
    pub fn travel_of(self, handle: u64) -> Option<(f64, f64, f64)> {
        let before = self.panel_before(handle)?;
        self.entries.with(|entries| {
            entries.iter().find_map(|entry| match entry {
                Entry::Panel {
                    id, bound, size, ..
                } if *id == before => Some((*size, bound.min, bound.max)),
                _ => None,
            })
        })
    }

    /// Moves the divider called `handle` by `delta` percentage points, and reports what moved.
    pub fn drag_by(self, handle: u64, delta: f64) -> f64 {
        let Some(boundary) = self.boundary_of(handle) else {
            return 0.0;
        };
        let (mut sizes, bounds) = self.panels();
        let moved = layout::drag(&mut sizes, &bounds, boundary, delta);
        if moved != 0.0 {
            self.write(&sizes);
        }
        moved
    }

    /// Puts the panel before the divider called `handle` at `size` percent outright.
    pub fn set_before(self, handle: u64, size: f64) -> f64 {
        let Some((now, _, _)) = self.travel_of(handle) else {
            return 0.0;
        };
        self.drag_by(handle, size - now)
    }

    /// Which pair of panels the divider called `handle` sits between.
    fn boundary_of(self, handle: u64) -> Option<usize> {
        self.entries.with_untracked(|entries| {
            let at = entries.iter().position(|entry| match entry {
                Entry::Handle { id } => *id == handle,
                Entry::Panel { .. } => false,
            })?;
            let before = entries[..at]
                .iter()
                .filter(|entry| matches!(entry, Entry::Panel { .. }))
                .count();
            (before > 0).then(|| before - 1)
        })
    }

    /// Which panel the divider called `handle` takes from.
    fn panel_before(self, handle: u64) -> Option<u64> {
        let boundary = self.boundary_of(handle)?;
        self.entries.with_untracked(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Panel { id, .. } => Some(*id),
                    Entry::Handle { .. } => None,
                })
                .nth(boundary)
        })
    }

    /// Every panel's share and bounds, in order.
    fn panels(self) -> (Vec<f64>, Vec<PanelBound>) {
        self.entries.with_untracked(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Panel { bound, size, .. } => Some((*size, *bound)),
                    Entry::Handle { .. } => None,
                })
                .unzip()
        })
    }

    /// Every panel's declared share and bounds, in order.
    fn declarations(self) -> (Vec<f64>, Vec<PanelBound>) {
        self.entries.with_untracked(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry {
                    Entry::Panel {
                        bound, declared, ..
                    } => Some((*declared, *bound)),
                    Entry::Handle { .. } => None,
                })
                .unzip()
        })
    }

    /// Writes a new set of shares back, in panel order, and tells whoever asked to be told.
    fn write(self, sizes: &[f64]) {
        self.entries.update(|entries| {
            let mut next = sizes.iter();
            for entry in entries.iter_mut() {
                if let Entry::Panel { size, .. } = entry
                    && let Some(value) = next.next()
                {
                    *size = *value;
                }
            }
        });
        if let Some(on_change) = self.on_change.get_value() {
            on_change.run(sizes.to_vec());
        }
    }

    /// Shares the group out again, which is what a panel coming or going calls for.
    ///
    /// From what the panels were *declared* as rather than from what they currently take, because
    /// the second compounds: the first panel of two is the whole group on its own, and sharing out
    /// from that would leave it two thirds when the second one arrived.
    fn rebalance(self) {
        let (sizes, bounds) = self.declarations();
        let balanced = layout::normalise(&sizes, &bounds);
        self.entries.update(|entries| {
            let mut next = balanced.iter();
            for entry in entries.iter_mut() {
                if let Entry::Panel { size, .. } = entry
                    && let Some(value) = next.next()
                {
                    *size = *value;
                }
            }
        });
    }

    /// A name nothing else in this group has.
    fn mint(self) -> u64 {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        id
    }

    /// Takes `id` out of the group when the calling scope goes away.
    fn forget_on_cleanup(self, id: u64) {
        on_cleanup_local(move || {
            self.entries.try_update(|entries| {
                entries.retain(|entry| match entry {
                    Entry::Panel { id: found, .. } | Entry::Handle { id: found } => *found != id,
                });
            });
        });
    }
}

/// A row or a column of panels with dividers between them.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A list beside the thing it lists.
/// #[component]
/// fn Split() -> impl IntoView {
///     view! {
///         ResizablePanelGroup(label = "Messages and reading pane") {
///             ResizablePanel(default_size = 30.0, min_size = 15.0) {
///                 text {"Inbox"}
///             }
///             ResizableHandle(label = "Resize the message list")
///             ResizablePanel(default_size = 70.0) {
///                 text {"The message"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Shares, not pixels
///
/// Every panel's size is a percentage of the group, so a window that is resized keeps the
/// proportions the user chose instead of giving all the new room to whichever panel happened to be
/// flexible. The arithmetic is [`drag`] and [`normalise`], which are plain functions over numbers
/// and are tested as such.
///
/// # Keyboard
///
/// Each divider is focusable and operable: the arrow keys for the group's own axis move it one
/// step, <kbd>Home</kbd> and <kbd>End</kbd> take the panel before it to its smallest and largest,
/// and <kbd>Enter</kbd> folds that panel away and brings it back at the size it had.
#[component]
pub fn ResizablePanelGroup(
    /// Which way the panels run.
    #[prop(default = Orientation::Horizontal)]
    direction: Orientation,
    /// Told whenever the sizes settle somewhere new, as one percentage per panel.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Vec<f64>>>,
    /// What the whole group is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The panels and the dividers between them.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ResizableStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = ResizableContext {
        direction,
        entries: RwSignal::new_local(Vec::new()),
        next: RwSignal::new_local(1),
        group: element,
        on_change: StoredValue::new_local(on_change),
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-resizable"), true)
        .attribute(
            zgui::view::AttrName::new("data-direction"),
            direction.name(),
        )
        .a11y_from(semantics);

    view! {
        box(node_ref = element, class = ResizableStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
