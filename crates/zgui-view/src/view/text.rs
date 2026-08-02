//! Text, and everything that renders as text.

use zgui_vocab::SharedString;

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::view::View;

/// One text node, and what was last written to it.
///
/// The remembered text is what turns "a signal was written" into "the text actually changed":
/// signals have no equality gate of their own, so without this comparison every write would reach
/// the backend and mark the node for reshaping.
pub struct TextState {
    /// The node.
    node: NodeId,
    /// What was last written to it.
    last: String,
}

impl TextState {
    /// Creates a text node holding `text`.
    pub fn new(dom: &DomHandle, text: &str) -> Self {
        Self {
            node: dom.create_text(text),
            last: text.to_owned(),
        }
    }

    /// The node.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Writes `text`, unless it is what is already there.
    ///
    /// Returns whether the backend was touched.
    pub fn set(&mut self, dom: &DomHandle, text: &str) -> bool {
        if self.last == text {
            return false;
        }
        self.last.clear();
        self.last.push_str(text);
        dom.set_text(self.node, text);
        true
    }
}

impl Anchor for TextState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.node, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        dom.detach(self.node);
    }

    fn first_node(&self) -> Option<NodeId> {
        Some(self.node)
    }
}

/// Declares a text view for a type that already knows how to write itself.
macro_rules! text_view {
    ($($type:ty => $text:expr;)+) => {
        $(
            impl View for $type {
                type State = TextState;

                fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
                    let text = $text(&self);
                    TextState::new(cx.dom(), &text)
                }

                fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
                    let text = $text(&self);
                    state.set(cx.dom(), &text);
                }
            }
        )+
    };
}

text_view! {
    &'static str => |value: &&'static str| (*value).to_owned();
    String => |value: &String| value.clone();
    std::rc::Rc<str> => |value: &std::rc::Rc<str>| value.to_string();
    SharedString => |value: &SharedString| value.to_string();
    bool => |value: &bool| value.to_string();
    char => |value: &char| value.to_string();
    u8 => |value: &u8| value.to_string();
    u16 => |value: &u16| value.to_string();
    u32 => |value: &u32| value.to_string();
    u64 => |value: &u64| value.to_string();
    usize => |value: &usize| value.to_string();
    i8 => |value: &i8| value.to_string();
    i16 => |value: &i16| value.to_string();
    i32 => |value: &i32| value.to_string();
    i64 => |value: &i64| value.to_string();
    isize => |value: &isize| value.to_string();
    f32 => |value: &f32| value.to_string();
    f64 => |value: &f64| value.to_string();
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_interned::ElementName;

    use super::TextState;
    use crate::DocumentId;
    use crate::dom::DomHandle;
    use crate::stub::StubDom;
    use crate::view::anchor::Anchor;

    #[test]
    fn writing_the_same_text_twice_touches_the_backend_once() {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let root = dom.create_element(ElementName::new("box"));

        let mut state = TextState::new(&dom, "a");
        state.mount(&dom, root, None);

        assert!(!state.set(&dom, "a"), "an unchanged value writes nothing");
        assert!(state.set(&dom, "b"));
        assert_eq!(backend.text_content(root), "b");
    }
}
