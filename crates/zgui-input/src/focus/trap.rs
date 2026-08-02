//! Confining sequential focus navigation to one subtree.
//!
//! A modal dialog that can be tabbed out of is not modal, and the failure is not visible until
//! someone tries: focus lands on a control behind the dimmed backdrop, the keyboard operates
//! something the user cannot see, and everything looks fine on screen. So a trap is a stack rather
//! than a flag — a dialog opened from a dialog confines to the newer one and gives the older one
//! back when it closes — and installing one records where focus was, so that closing it can put
//! focus back rather than dropping it at the root.

use smallvec::SmallVec;
use zgui_dom::{DocumentStore, NodeKey};

/// One installed trap's name.
///
/// Never reused, so removing a trap that has already been removed removes nothing rather than
/// removing whichever trap happened to take its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FocusTrapId(u64);

impl FocusTrapId {
    /// The name numbered `value`.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// How a trap behaves while it is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrapOptions {
    /// Whether moving past the last focusable element wraps round to the first.
    pub wrap: bool,
    /// Whether focus moves into the subtree as the trap is installed.
    pub auto_focus: bool,
    /// Whether focus returns to wherever it was when the trap is removed.
    pub restore: bool,
}

impl TrapOptions {
    /// Cycling, self-focusing and restoring: what a modal surface wants.
    pub const MODAL: Self = Self {
        wrap: true,
        auto_focus: true,
        restore: true,
    };

    /// Confines traversal without moving focus in or out, which is what a menu opened from a
    /// toolbar wants: the toolbar keeps its focus where it was.
    pub const CONFINE_ONLY: Self = Self {
        wrap: true,
        auto_focus: false,
        restore: false,
    };
}

impl Default for TrapOptions {
    fn default() -> Self {
        Self::MODAL
    }
}

/// One installed trap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trap {
    /// Its name.
    pub id: FocusTrapId,
    /// The subtree traversal is confined to.
    pub root: NodeKey,
    /// How it behaves.
    pub options: TrapOptions,
    /// What had focus when it was installed, so that removing it can give focus back.
    pub restore_to: Option<NodeKey>,
}

/// The stack of live traps, topmost last.
///
/// ```
/// use zgui_dom::{Document, EverythingMatters};
/// use zgui_input::focus::{FocusTraps, TrapOptions};
/// use zgui_interned::ElementName;
///
/// let document = Document::new();
/// let (dialog, outside) = document
///     .edit(&EverythingMatters, |edit| {
///         let root = edit.create_element(ElementName::new("root"));
///         edit.insert_before(document.document_index(), root, None);
///         let dialog = edit.create_element(ElementName::new("surface"));
///         edit.insert_before(root, dialog, None);
///         let outside = edit.create_element(ElementName::new("control"));
///         edit.insert_before(root, outside, None);
///         (dialog, outside)
///     })
///     .expect("not poisoned");
/// let store = document.store();
///
/// let mut traps = FocusTraps::default();
/// let id = traps.push(store.key_of(dialog), TrapOptions::MODAL, None);
///
/// assert!(traps.confines(store, store.key_of(dialog)));
/// assert!(!traps.confines(store, store.key_of(outside)));
///
/// traps.pop(id);
/// assert!(traps.confines(store, store.key_of(outside)), "with nothing installed, nothing is out");
/// ```
#[derive(Clone, Debug, Default)]
pub struct FocusTraps {
    /// Installed traps, oldest first.
    stack: SmallVec<[Trap; 2]>,
    /// The next name to hand out.
    next: u64,
}

impl FocusTraps {
    /// Installs a trap over `root`, recording `focused` as the element to restore to.
    pub fn push(
        &mut self,
        root: NodeKey,
        options: TrapOptions,
        focused: Option<NodeKey>,
    ) -> FocusTrapId {
        let id = FocusTrapId::new(self.next);
        self.next += 1;
        self.stack.push(Trap {
            id,
            root,
            options,
            restore_to: focused,
        });
        id
    }

    /// Removes a trap, answering with it.
    ///
    /// Removing one that is not the topmost is allowed and leaves the rest of the stack in place:
    /// two overlays can be dismissed in either order, and refusing the unusual one would leave a
    /// window that cannot be tabbed out of again.
    pub fn pop(&mut self, id: FocusTrapId) -> Option<Trap> {
        let position = self.stack.iter().position(|trap| trap.id == id)?;
        Some(self.stack.remove(position))
    }

    /// The trap in force, which is the most recently installed one.
    pub fn topmost(&self) -> Option<&Trap> {
        self.stack.last()
    }

    /// Removes every trap whose subtree has left the document, answering with them oldest first.
    ///
    /// A trap is normally uninstalled by the guard that holds it, as the surface it confines is
    /// taken away. The case this exists for is the other order: the surface has gone and the guard
    /// has not been dropped — a component that failed to unmount, a handle held past the life of
    /// the thing it names.
    ///
    /// What that leaves is a trap over a subtree the document no longer has, and while it stands
    /// every traversal is confined to nothing: no key moves focus anywhere in the window, for the
    /// rest of the session. A trap that confines a subtree nobody can reach is not confining
    /// anything, so it is not kept.
    pub fn drop_stranded(&mut self, store: &DocumentStore) -> SmallVec<[Trap; 2]> {
        let mut stranded: SmallVec<[Trap; 2]> = SmallVec::new();
        self.stack.retain(|trap| {
            // Attached, rather than merely resolvable. A removed subtree's records stay readable
            // until the frame that removed them ends, and a trap held over one of those is exactly
            // as unreachable as one held over a slot that has already been recycled.
            let live = store
                .index_of(trap.root)
                .is_some_and(|index| store.core(index).parent().is_some());
            if !live {
                stranded.push(*trap);
            }
            live
        });
        stranded
    }

    /// Whether the trap named `id` is still installed.
    ///
    /// What anything holding on to a trap between one frame and the next has to ask before acting
    /// on it: a surface can open and close inside one frame, and a trap that is gone confines
    /// nothing and is owed nothing.
    pub fn holds(&self, id: FocusTrapId) -> bool {
        self.stack.iter().any(|trap| trap.id == id)
    }

    /// How many traps are installed.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Whether none is.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Whether traversal may reach `node`.
    ///
    /// With no trap installed, everything is reachable. With one installed, only its own subtree
    /// is — and the trap's root counts as inside itself.
    pub fn confines(&self, store: &DocumentStore, node: NodeKey) -> bool {
        match self.topmost() {
            None => true,
            Some(trap) => contains(store, trap.root, node),
        }
    }

    /// The subtree traversal is confined to, or `None` when it is the whole document.
    pub fn confined_to(&self) -> Option<NodeKey> {
        self.topmost().map(|trap| trap.root)
    }
}

/// Whether `node` is `ancestor` or sits inside it.
pub fn contains(store: &DocumentStore, ancestor: NodeKey, node: NodeKey) -> bool {
    let Some(mut index) = store.index_of(node) else {
        return false;
    };
    loop {
        let record = store.core(index);
        if record.key() == ancestor {
            return true;
        }
        match record.parent() {
            Some(parent) => index = parent,
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKey};
    use zgui_interned::ElementName;

    use super::{FocusTraps, TrapOptions, contains};

    /// `root > (first_dialog > inner, second_dialog)`.
    fn document() -> (Document, [NodeKey; 4]) {
        let document = Document::new();
        let indices = document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let first = edit.create_element(ElementName::new("surface"));
                edit.insert_before(root, first, None);
                let inner = edit.create_element(ElementName::new("control"));
                edit.insert_before(first, inner, None);
                let second = edit.create_element(ElementName::new("surface"));
                edit.insert_before(root, second, None);
                [root, first, inner, second]
            })
            .expect("not poisoned");
        let keys = indices.map(|index| document.store().key_of(index));
        (document, keys)
    }

    #[test]
    fn a_dialog_opened_from_a_dialog_confines_to_the_newer_one() {
        let (document, [root, first, inner, second]) = document();
        let store = document.store();
        let mut traps = FocusTraps::default();

        let outer = traps.push(first, TrapOptions::MODAL, Some(root));
        assert!(traps.confines(store, inner));

        let inner_trap = traps.push(second, TrapOptions::MODAL, Some(inner));
        assert!(
            !traps.confines(store, inner),
            "the newer trap is the one in force"
        );
        assert_eq!(traps.confined_to(), Some(second));
        assert_eq!(traps.len(), 2);

        let removed = traps.pop(inner_trap).expect("it was installed");
        assert_eq!(
            removed.restore_to,
            Some(inner),
            "and it knows where focus came from"
        );
        assert!(
            traps.confines(store, inner),
            "the older trap is in force again"
        );

        traps.pop(outer);
        assert!(traps.is_empty());
    }

    #[test]
    fn a_trap_over_a_subtree_that_has_gone_stops_confining_anything() {
        // The surface was taken away and its guard was not dropped. What is left confines
        // traversal to a subtree the document no longer has, which is a window where no key moves
        // focus anywhere — so it is not kept.
        let (document, [root, first, _, second]) = document();
        let mut traps = FocusTraps::default();
        let outer = traps.push(first, TrapOptions::MODAL, Some(root));
        traps.push(second, TrapOptions::MODAL, Some(root));

        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                let index = document.store().index_of(second).expect("still there");
                edit.remove(index);
            })
            .expect("not poisoned");

        let stranded = traps.drop_stranded(document.store());
        assert_eq!(stranded.len(), 1, "one surface went, so one trap did");
        assert_eq!(
            stranded[0].restore_to,
            Some(root),
            "and it still says where focus came from, so it can be given back"
        );
        assert_eq!(
            traps.topmost().map(|trap| trap.id),
            Some(outer),
            "the trap whose surface is still there is in force again"
        );

        assert!(
            traps.drop_stranded(document.store()).is_empty(),
            "and nothing else is dropped by asking twice"
        );
    }

    #[test]
    fn removing_a_trap_that_is_not_the_topmost_leaves_the_rest_in_place() {
        let (_document, [_, first, _, second]) = document();
        let mut traps = FocusTraps::default();
        let outer = traps.push(first, TrapOptions::MODAL, None);
        let top = traps.push(second, TrapOptions::MODAL, None);

        assert!(traps.pop(outer).is_some());
        assert_eq!(traps.topmost().map(|trap| trap.id), Some(top));
        assert!(traps.pop(outer).is_none(), "and a name is never reused");
    }

    #[test]
    fn containment_includes_the_root_itself_and_excludes_a_stranger() {
        let (document, [root, first, inner, second]) = document();
        let store = document.store();
        assert!(contains(store, first, first));
        assert!(contains(store, first, inner));
        assert!(contains(store, root, second));
        assert!(!contains(store, second, inner));
    }

    #[test]
    fn the_confining_options_move_no_focus() {
        // A menu opened from a toolbar confines traversal and leaves the toolbar's focus alone,
        // which is the one difference between the two shipped shapes.
        assert_eq!(
            TrapOptions::CONFINE_ONLY,
            TrapOptions {
                wrap: true,
                auto_focus: false,
                restore: false,
            }
        );
        assert_eq!(TrapOptions::default(), TrapOptions::MODAL);
        assert_ne!(TrapOptions::MODAL, TrapOptions::CONFINE_ONLY);
    }
}
