//! What the framework itself does about an event, beyond delivering it.
//!
//! Pressing a control focuses it. Letting go over the control that was pressed activates it.
//! Turning a wheel scrolls the nearest thing that scrolls. Tab moves focus. None of that is a
//! listener, and all of it is cancellable: a handler that has taken responsibility for an event
//! says so, and then none of this happens.
//!
//! These are *computed*, not performed. Focus and scroll writes belong to whoever is driving the
//! frame, which is also the only thing that knows whether a handler asked for the default to be
//! skipped — and it cannot know that until every listener on the path has run.

use zgui_dom::{DocumentStore, NodeKey};
use zgui_layout::LayoutStore;
use zgui_vocab::{Key, KeyEvent, Modifiers, NamedKey, ScrollDelta, ScrollPhase};

use crate::focus::order::{self, FocusDirection};
use crate::hit::HitChain;
use crate::state::focus::FocusSource;

/// Something the framework does about an event on its own account.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum FrameworkDefault {
    /// Move focus to this element, or clear focus when there is none to move it to.
    Focus {
        /// Where focus should go.
        node: Option<NodeKey>,
        /// How it got there, which decides whether a ring is shown.
        source: FocusSource,
    },
    /// Activate this element: what a press and release over the same element means.
    Activate(NodeKey),
    /// Scroll this container by this much.
    Scroll {
        /// The element that scrolls.
        container: NodeKey,
        /// How far, in the units the device reported.
        delta: ScrollDelta,
        /// Where the scroll sat in a gesture, which decides whether it travels or arrives.
        ///
        /// A [`ScrollPhase::Discrete`] scroll is one detent of a notched wheel: it arrived whole
        /// and something has to carry the content to its new place over the next few frames. Every
        /// other phase belongs to a continuous surface, whose deltas are already a motion — and
        /// animating a motion again is what makes a trackpad feel like treacle.
        phase: ScrollPhase,
    },
    /// Move focus sequentially, which is what the tab key asks for.
    MoveFocus(FocusDirection),
    /// Put one axis of this container at this offset, leaving the other where it is.
    ///
    /// What dragging a scrollbar thumb asks for. Absolute rather than relative because a drag is a
    /// position and not a movement: the offset is a function of where the pointer is now and of the
    /// grab it started with, so accumulating deltas would let rounding walk the thumb away from the
    /// pointer over a long drag.
    ScrollAlong {
        /// The element that scrolls.
        container: NodeKey,
        /// Which axis moves.
        axis: zgui_layout::Axis,
        /// Where it goes, in device pixels.
        to: f32,
    },
    /// Move one axis of this container by one screenful.
    ///
    /// What a press on a scrollbar track asks for. How far a screenful is belongs to the container
    /// and is resolved by whoever carries this out, exactly as a wheel's line height is.
    ScrollPage {
        /// The element that scrolls.
        container: NodeKey,
        /// Which axis moves.
        axis: zgui_layout::Axis,
        /// Whether it moves towards the end of the content rather than towards the start.
        forward: bool,
    },
}

/// What a press does: focus whatever on the path can take it.
///
/// Pressing something unfocusable takes focus away rather than leaving it where it was, which is
/// what makes clicking the background dismiss a field's caret. The nearest focusable *ancestor*
/// counts, so pressing the text inside a button focuses the button.
pub fn on_press(
    store: &DocumentStore,
    layout: Option<&LayoutStore>,
    chain: &HitChain,
) -> FrameworkDefault {
    FrameworkDefault::Focus {
        node: nearest_focusable(store, layout, chain),
        source: FocusSource::Pointer,
    }
}

/// What a release does: activate the element the press landed on, if this release is over it.
///
/// A press that slid off its element before being let go is not an activation, which is the
/// affordance that lets someone change their mind mid-click.
pub fn on_release(chain: &HitChain, pressed: Option<NodeKey>) -> Option<FrameworkDefault> {
    let pressed = pressed?;
    chain
        .contains(pressed)
        .then_some(FrameworkDefault::Activate(pressed))
}

/// What a wheel does: scroll the nearest scrolling ancestor, if there is one.
///
/// The delta travels on unconverted, in whichever unit the device reported, and that is the
/// second half of the answer rather than an omission: a notched wheel reports *lines*, and how far
/// a line is depends on the used line height of the element being scrolled — which is not known
/// until this call has said which element that is. So a caller resolves the units for the
/// container this hands back and then converts, with
/// [`normalize::scroll::to_device`](crate::normalize::scroll::to_device).
pub fn on_wheel(
    store: &DocumentStore,
    layout: &LayoutStore,
    chain: &HitChain,
    delta: ScrollDelta,
    phase: ScrollPhase,
) -> Option<FrameworkDefault> {
    nearest_scrollable(store, layout, chain).map(|container| FrameworkDefault::Scroll {
        container,
        delta,
        phase,
    })
}

/// What a key does on the framework's own account.
///
/// Two things, and no more: tab moves focus, and enter or space activates whatever has focus. A
/// repeat is dropped for both, because holding tab down must not run through the whole document
/// and holding enter must not activate a button forty times.
pub fn on_key(
    event: &KeyEvent,
    modifiers: Modifiers,
    focused: Option<NodeKey>,
) -> Option<FrameworkDefault> {
    if !crate::normalize::keyboard::accepts(event, crate::normalize::keyboard::Reading::Command) {
        return None;
    }
    match crate::normalize::keyboard::shortcut_key(event) {
        Key::Named(NamedKey::Tab) => Some(FrameworkDefault::MoveFocus(if modifiers.shift() {
            FocusDirection::Prev
        } else {
            FocusDirection::Next
        })),
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
            focused.map(FrameworkDefault::Activate)
        }
        _ => None,
    }
}

/// The innermost element of `chain` that can take focus, itself included.
pub fn nearest_focusable(
    store: &DocumentStore,
    layout: Option<&LayoutStore>,
    chain: &HitChain,
) -> Option<NodeKey> {
    chain
        .path()
        .iter()
        .rev()
        .copied()
        .find(|node| order::is_focusable(store, layout, *node))
}

/// The innermost element of `chain` that scrolls, itself included.
///
/// Whether an element scrolls is a question about the box it generated, because it is the used
/// `overflow` value that decides it — and an element that generates no box scrolls nothing however
/// its style reads.
pub fn nearest_scrollable(
    store: &DocumentStore,
    layout: &LayoutStore,
    chain: &HitChain,
) -> Option<NodeKey> {
    let _ = store;
    chain
        .path()
        .iter()
        .rev()
        .copied()
        .find(|node| scrolls(layout, *node))
}

/// Every element from `from` outwards that scrolls, `from` itself included.
///
/// This is what a wheel actually acts on. [`nearest_scrollable`] answers which container the wheel
/// is *over*, and that is only the first answer: a list that has already reached its bottom hands
/// the rest of the turn to whatever contains it, and without the chain the page under a bottomed-out
/// list simply stops scrolling. The walk is up the *element* tree, so a container reached through a
/// `display: contents` wrapper is still found.
pub fn scroll_chain(
    store: &DocumentStore,
    layout: &LayoutStore,
    from: NodeKey,
) -> smallvec::SmallVec<[NodeKey; 4]> {
    let mut chain = smallvec::SmallVec::new();
    let Some(mut index) = store.index_of(from) else {
        return chain;
    };
    loop {
        let record = store.core(index);
        let node = record.key();
        if scrolls(layout, node) {
            chain.push(node);
        }
        match record.parent() {
            Some(parent) => index = parent,
            None => return chain,
        }
    }
}

/// Whether one element generated a box that scrolls.
fn scrolls(layout: &LayoutStore, node: NodeKey) -> bool {
    layout.boxes_of(node).iter().any(|key| {
        layout
            .get(*key)
            .is_some_and(|box_| zgui_layout::scroll_region::is_scroll_container(&box_.style))
    })
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeIndex};
    use zgui_interned::{AttrName, ElementName};
    use zgui_vocab::{
        Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey, ScrollDelta, SharedString,
        UiState,
    };

    use super::{FrameworkDefault, on_key, on_press, on_release};
    use crate::focus::order::FocusDirection;
    use crate::hit::HitChain;
    use crate::state::focus::FocusSource;

    /// `root > (surface > label)` where the surface is a control.
    fn document() -> (Document, [NodeIndex; 3]) {
        let document = Document::new();
        let nodes = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let control = edit.create_element(ElementName::new("control"));
                edit.insert_before(root, control, None);
                let label = edit.create_element(ElementName::new("label"));
                edit.insert_before(control, label, None);
                [root, control, label]
            })
            .expect("not poisoned");
        (document, nodes)
    }

    #[test]
    fn pressing_the_text_inside_a_control_focuses_the_control() {
        let (document, [_, control, label]) = document();
        let chain = HitChain::to_root(document.store(), document.store().key_of(label));
        assert_eq!(
            on_press(document.store(), None, &chain),
            FrameworkDefault::Focus {
                node: Some(document.store().key_of(control)),
                source: FocusSource::Pointer,
            }
        );
    }

    #[test]
    fn pressing_something_unfocusable_takes_focus_away() {
        let (document, [root, control, _]) = document();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_state(control, UiState::DISABLED, true);
            })
            .expect("not poisoned");
        let chain = HitChain::to_root(document.store(), document.store().key_of(control));
        assert_eq!(
            on_press(document.store(), None, &chain),
            FrameworkDefault::Focus {
                node: None,
                source: FocusSource::Pointer,
            },
            "the disabled control is not focusable and neither is the root above it"
        );
        let _ = root;
    }

    #[test]
    fn a_release_over_the_pressed_element_activates_it_and_one_that_slid_off_does_not() {
        let (document, [root, control, label]) = document();
        let store = document.store();
        let over_label = HitChain::to_root(store, store.key_of(label));
        let over_root = HitChain::to_root(store, store.key_of(root));

        assert_eq!(
            on_release(&over_label, Some(store.key_of(control))),
            Some(FrameworkDefault::Activate(store.key_of(control))),
            "the release is inside the control that was pressed"
        );
        assert_eq!(
            on_release(&over_root, Some(store.key_of(control))),
            None,
            "and sliding off it before letting go is not an activation"
        );
        assert_eq!(on_release(&over_label, None), None);
    }

    #[test]
    fn tab_moves_focus_and_shift_tab_moves_it_the_other_way() {
        let event = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
        assert_eq!(
            on_key(&event, Modifiers::NONE, None),
            Some(FrameworkDefault::MoveFocus(FocusDirection::Next))
        );
        assert_eq!(
            on_key(&event, Modifiers::SHIFT, None),
            Some(FrameworkDefault::MoveFocus(FocusDirection::Prev))
        );
    }

    #[test]
    fn enter_activates_what_has_focus_and_a_repeat_activates_nothing() {
        let (document, [_, control, _]) = document();
        let focused = document.store().key_of(control);
        let mut event = KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
        assert_eq!(
            on_key(&event, Modifiers::NONE, Some(focused)),
            Some(FrameworkDefault::Activate(focused))
        );

        event.repeat = true;
        assert_eq!(
            on_key(&event, Modifiers::NONE, Some(focused)),
            None,
            "holding the key must not activate the control again and again"
        );
    }

    #[test]
    fn a_key_the_framework_has_no_behaviour_for_produces_no_default() {
        let mut event = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
        event.key = Key::Character(SharedString::from("x"));
        assert_eq!(on_key(&event, Modifiers::NONE, None), None);
    }

    #[test]
    fn a_declared_tabindex_makes_an_ordinary_element_the_press_target() {
        let (document, [_, _, label]) = document();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_attribute(
                    label,
                    AttrName::new("tabindex"),
                    Some(SharedString::from("0")),
                );
            })
            .expect("not poisoned");
        let chain = HitChain::to_root(document.store(), document.store().key_of(label));
        assert_eq!(
            on_press(document.store(), None, &chain),
            FrameworkDefault::Focus {
                node: Some(document.store().key_of(label)),
                source: FocusSource::Pointer,
            }
        );
    }

    #[test]
    fn a_wheel_over_a_document_with_nothing_scrollable_scrolls_nothing() {
        use zgui_layout::LayoutStore;
        let (document, [_, _, label]) = document();
        let layout = LayoutStore::new(document.store().document());
        let chain = HitChain::to_root(document.store(), document.store().key_of(label));
        assert_eq!(
            super::on_wheel(
                document.store(),
                &layout,
                &chain,
                ScrollDelta::Lines { x: 0.0, y: -1.0 },
                zgui_vocab::ScrollPhase::Discrete
            ),
            None
        );
    }
}
