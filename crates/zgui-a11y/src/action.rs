//! What an assistive technology asked for, in the document's own terms.
//!
//! An action arrives from another process, names a node by identifier, and has to become something
//! the framework already knows how to do. The point of this module is that it becomes **exactly**
//! that and nothing new: activating a button is the click a pointer would have produced, on the
//! same dispatch path, reaching the same listeners in the same order. No component writes separate
//! activation logic, because there is no separate path for it to be written against.
//!
//! What this crate does *not* do is carry the intent out. Running a listener means calling a
//! view-layer handler, and a system below the view layer cannot name one. The intent is answered;
//! whatever holds the frame performs it.

use zgui_dom::NodeKey;
use zgui_vocab::EventKind;

/// What an assistive technology asked for, as it arrives from the platform.
///
/// Re-exported because the frame loop routes one and does not otherwise name the accessibility
/// interchange vocabulary at all: a request crosses from the window backend to the frame and the
/// two would otherwise need a third name for it.
pub use accesskit::{Action, ActionData, ActionRequest, Point};

use crate::id::to_document;

/// What an inbound action means for a document.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Dispatch `event` on the node, exactly as a pointer or a key would have.
    Dispatch {
        /// The node the event is aimed at.
        node: NodeKey,
        /// Which event it is.
        event: EventKind,
    },
    /// Move keyboard focus to the node.
    Focus(NodeKey),
    /// Take keyboard focus off the node.
    Blur(NodeKey),
    /// Step a measured control's value.
    Step {
        /// The node whose value moves.
        node: NodeKey,
        /// Which way it moves.
        by: Step,
    },
    /// Set a control's value to text an assistive technology supplied.
    SetValue {
        /// The node whose value is being set.
        node: NodeKey,
        /// The value to set.
        value: String,
    },
    /// Bring the node into view, which the framework does for every node without being told how.
    ScrollIntoView(NodeKey),
    /// Scroll the node to an absolute offset, in CSS pixels of its own space.
    ScrollTo {
        /// The container being scrolled.
        node: NodeKey,
        /// Where its content is to sit.
        offset: Point,
    },
    /// Scroll the node's container, which the framework likewise does generically.
    Scroll {
        /// The node whose container scrolls.
        node: NodeKey,
        /// Which way.
        by: Scroll,
    },
}

/// Which way a measured control's value moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Up by one step.
    Up,
    /// Down by one step.
    Down,
}

/// Which way a container scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    /// Towards the start of the block axis.
    Up,
    /// Towards the end of the block axis.
    Down,
    /// Towards the start of the inline axis.
    Left,
    /// Towards the end of the inline axis.
    Right,
}

/// What `request` means, or nothing when this build has no answer for it.
///
/// `None` is the honest answer for an action whose target is not a node of any document here, and
/// for one this build does not implement. Guessing would be worse: an assistive technology told
/// that an action succeeded when nothing happened is one whose user is told the application
/// responded.
///
/// ```
/// use accesskit::{Action, ActionRequest, NodeId, TreeId};
/// use zgui_a11y::Intent;
/// use zgui_vocab::EventKind;
///
/// let request = ActionRequest {
///     action: Action::Click,
///     target_tree: TreeId::ROOT,
///     target_node: NodeId(0),
///     data: None,
/// };
/// assert_eq!(zgui_a11y::intent_of(&request), None);
/// ```
pub fn intent_of(request: &ActionRequest) -> Option<Intent> {
    let node = to_document(request.target_node)?;
    match request.action {
        // The whole of the activation story: an inbound click *is* a click, so a component that
        // handles being clicked handles being activated by a screen reader for free.
        Action::Click => Some(Intent::Dispatch {
            node,
            event: EventKind::Click,
        }),
        Action::Focus => Some(Intent::Focus(node)),
        Action::Blur => Some(Intent::Blur(node)),
        Action::Increment => Some(Intent::Step { node, by: Step::Up }),
        Action::Decrement => Some(Intent::Step {
            node,
            by: Step::Down,
        }),
        Action::SetValue | Action::ReplaceSelectedText => {
            value_of(request).map(|value| Intent::SetValue { node, value })
        }
        Action::ShowContextMenu => Some(Intent::Dispatch {
            node,
            event: EventKind::ContextMenu,
        }),
        Action::ScrollIntoView | Action::SetSequentialFocusNavigationStartingPoint => {
            Some(Intent::ScrollIntoView(node))
        }
        Action::ScrollUp => Some(Intent::Scroll {
            node,
            by: Scroll::Up,
        }),
        Action::ScrollDown => Some(Intent::Scroll {
            node,
            by: Scroll::Down,
        }),
        Action::ScrollLeft => Some(Intent::Scroll {
            node,
            by: Scroll::Left,
        }),
        Action::ScrollRight => Some(Intent::Scroll {
            node,
            by: Scroll::Right,
        }),
        // An offset rather than a direction: a scrollbar dragged by an assistive technology sends
        // where the content is to sit, and answering it with a number of lines would move the
        // container somewhere else entirely.
        Action::SetScrollOffset => {
            offset_of(request).map(|offset| Intent::ScrollTo { node, offset })
        }
        _ => None,
    }
}

/// Where a measured control's value lands after one step in `by`.
///
/// The step is the control's own if it declared one and a hundredth of its range otherwise, which
/// is what a slider with no declared step has to mean: an assistive technology that cannot move a
/// value at all is one whose user cannot use the control.
///
/// `None` when the control declared no value, because there is then nothing to step.
pub fn stepped(numeric: &zgui_vocab::Numeric, by: Step) -> Option<f64> {
    let value = numeric.value?;
    let span = match (numeric.min, numeric.max) {
        (Some(min), Some(max)) if max > min => max - min,
        _ => 1.0,
    };
    let step = numeric
        .step
        .filter(|step| *step > 0.0)
        .unwrap_or(span / 100.0);
    let moved = match by {
        Step::Up => value + step,
        Step::Down => value - step,
    };
    Some(match (numeric.min, numeric.max) {
        (Some(min), Some(max)) => moved.clamp(min, max),
        (Some(min), None) => moved.max(min),
        (None, Some(max)) => moved.min(max),
        (None, None) => moved,
    })
}

/// The offset an action carried, when it carried one.
fn offset_of(request: &ActionRequest) -> Option<Point> {
    match request.data.as_ref()? {
        ActionData::SetScrollOffset(offset) => Some(*offset),
        _ => None,
    }
}

/// The text an action carried, when it carried any.
fn value_of(request: &ActionRequest) -> Option<String> {
    match request.data.as_ref()? {
        ActionData::Value(value) => Some(value.to_string()),
        ActionData::NumericValue(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Action, ActionData, ActionRequest, TreeId};
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;
    use zgui_vocab::EventKind;

    use super::{Intent, Step, intent_of, stepped};
    use crate::id::to_a11y;

    /// A request of `action` aimed at a live node of a throwaway document.
    fn request(
        action: Action,
        data: Option<ActionData>,
    ) -> (Document, zgui_dom::NodeKey, ActionRequest) {
        let mut document = Document::new();
        let node = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("control"),
        );
        let key = document.store().key_of(node);
        let request = ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: to_a11y(key),
            data,
        };
        (document, key, request)
    }

    #[test]
    fn an_inbound_click_is_the_click_a_pointer_would_have_produced() {
        let (_document, key, request) = request(Action::Click, None);
        assert_eq!(
            intent_of(&request),
            Some(Intent::Dispatch {
                node: key,
                event: EventKind::Click,
            }),
            "any other answer means every component has to implement activation twice"
        );
    }

    #[test]
    fn stepping_a_value_names_the_direction_and_not_the_amount() {
        let (_document, key, up) = request(Action::Increment, None);
        assert_eq!(
            intent_of(&up),
            Some(Intent::Step {
                node: key,
                by: Step::Up
            })
        );
    }

    #[test]
    fn setting_a_value_without_one_is_not_an_intent() {
        let (_document, _key, request) = request(Action::SetValue, None);
        assert_eq!(intent_of(&request), None);
    }

    #[test]
    fn setting_a_value_carries_the_text_through() {
        let (_document, key, request) =
            request(Action::SetValue, Some(ActionData::Value("42".into())));
        assert_eq!(
            intent_of(&request),
            Some(Intent::SetValue {
                node: key,
                value: "42".to_owned(),
            })
        );
    }

    #[test]
    fn a_control_with_no_declared_step_can_still_be_moved() {
        let numeric = zgui_vocab::Numeric {
            value: Some(0.5),
            min: Some(0.0),
            max: Some(1.0),
            ..zgui_vocab::Numeric::default()
        };
        let up = stepped(&numeric, Step::Up).expect("a value was declared");
        assert!(
            up > 0.5,
            "a slider that declared no step must still move, or its user cannot use it"
        );
        assert!(up <= 1.0);
    }

    #[test]
    fn stepping_never_leaves_the_declared_range() {
        let numeric = zgui_vocab::Numeric {
            value: Some(0.0),
            min: Some(0.0),
            max: Some(1.0),
            step: Some(0.25),
            ..zgui_vocab::Numeric::default()
        };
        assert_eq!(stepped(&numeric, Step::Down), Some(0.0));
        assert_eq!(stepped(&numeric, Step::Up), Some(0.25));
    }

    #[test]
    fn a_control_that_declared_no_value_cannot_be_stepped() {
        assert_eq!(stepped(&zgui_vocab::Numeric::default(), Step::Up), None);
    }

    #[test]
    fn an_action_this_build_cannot_perform_is_answered_with_nothing() {
        let (_document, _key, request) = request(Action::CustomAction, None);
        assert_eq!(
            intent_of(&request),
            None,
            "an assistive technology told an action succeeded when nothing happened tells its \
             user the application responded"
        );
    }
}
