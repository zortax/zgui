//! What a component asked of its backend, in the order it asked.
//!
//! One transcript per test, shared by everything that records into it: the node tree, the host and
//! the scripted input all append to the same list. That sharing is the point — a component test is
//! usually a claim about *order* ("the dialog traps focus before it moves it", "the press is seen
//! before the class changes"), and three separate logs cannot answer an ordering question at all.

mod op;

use std::cell::RefCell;
use std::fmt::{self, Display};
use std::rc::Rc;

pub use crate::transcript::op::Op;

/// A shared, ordered record of everything a view asked its backend to do.
///
/// Cheap to clone: every clone appends to the same list, which is how one transcript is handed to
/// a tree, a host and a dispatcher at once.
///
/// ```
/// use zgui_testkit_view::{Op, Transcript};
/// use zgui_view::{DocumentId, NodeId};
///
/// let transcript = Transcript::new();
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
/// transcript.push(Op::Detach { node });
///
/// assert_eq!(transcript.len(), 1);
/// assert!(transcript.to_string().contains("detach"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Transcript {
    /// The list, shared with every clone.
    ops: Rc<RefCell<Vec<Op>>>,
}

impl Transcript {
    /// An empty transcript.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one operation.
    pub fn push(&self, op: Op) {
        self.ops.borrow_mut().push(op);
    }

    /// Everything recorded so far, in order.
    pub fn ops(&self) -> Vec<Op> {
        self.ops.borrow().clone()
    }

    /// How many operations have been recorded.
    pub fn len(&self) -> usize {
        self.ops.borrow().len()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.ops.borrow().is_empty()
    }

    /// Forgets everything recorded so far.
    ///
    /// What a test calls after building a view, so that what it asserts on is what the interaction
    /// did rather than what the mount did.
    pub fn clear(&self) {
        self.ops.borrow_mut().clear();
    }

    /// Compares the rendered transcript against a golden file, or writes one when blessing.
    ///
    /// The comparison and the blessing are the shared ones, so a transcript golden behaves exactly
    /// as every other golden in this workspace: it fails when the file is missing rather than
    /// creating it, and a blessing run never also claims to have checked anything.
    ///
    /// # Panics
    ///
    /// Panics when the rendered transcript differs from the golden, or when the golden is missing.
    pub fn assert_matches(&self, path: impl AsRef<std::path::Path>) {
        zgui_testkit_scene::dump::golden::assert_matches(path.as_ref(), &self.to_string());
    }
}

impl Display for Transcript {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for op in self.ops.borrow().iter() {
            writeln!(formatter, "{}", line(op))?;
        }
        Ok(())
    }
}

/// One operation, as one line.
///
/// Nodes are written by their number alone. A transcript is compared against a golden, so every
/// part of a line has to be stable across runs — and the number a backend mints is, while its
/// address is not.
fn line(op: &Op) -> String {
    match op {
        Op::Create { node, what } => format!("create {} {what}", id(*node)),
        Op::Insert {
            parent,
            child,
            before,
        } => match before {
            Some(before) => format!(
                "insert {} into {} before {}",
                id(*child),
                id(*parent),
                id(*before)
            ),
            None => format!("insert {} into {}", id(*child), id(*parent)),
        },
        Op::Detach { node } => format!("detach {}", id(*node)),
        Op::SetText { node, text } => format!("text {} {text:?}", id(*node)),
        Op::SetAttribute { node, name, value } => match value {
            Some(value) => format!("attr {} {name}={value:?}", id(*node)),
            None => format!("attr {} {name} removed", id(*node)),
        },
        Op::SetClasses { node, classes } => {
            format!("classes {} [{}]", id(*node), classes.join(" "))
        }
        Op::ToggleClass { node, class, on } => {
            format!("class {} {class} {}", id(*node), on_off(*on))
        }
        Op::SetStyle {
            node,
            property,
            value,
        } => match value {
            Some(value) => format!("style {} {property}={value:?}", id(*node)),
            None => format!("style {} {property} removed", id(*node)),
        },
        Op::SetUiState { node, state, on } => {
            format!("state {} {state} {}", id(*node), on_off(*on))
        }
        Op::SetProperty {
            node,
            property,
            value,
        } => match value {
            Some(value) => format!("prop {} {property}={value:?}", id(*node)),
            None => format!("prop {} {property} removed", id(*node)),
        },
        Op::SetCustomState { node, name, on } => {
            format!("custom-state {} {name} {}", id(*node), on_off(*on))
        }
        Op::SetSemantics { node, role } => match role {
            Some(role) => format!("semantics {} {role}", id(*node)),
            None => format!("semantics {} cleared", id(*node)),
        },
        Op::AddListener {
            node,
            event,
            capture,
        } => format!(
            "listen {} {event}{}",
            id(*node),
            if *capture { " capture" } else { "" }
        ),
        Op::RemoveListener { node } => format!("unlisten {}", id(*node)),
        Op::Observe { node, what } => format!("observe {} {what}", id(*node)),
        Op::Focus { node } => format!("focus {}", id(*node)),
        Op::Scroll { node } => format!("scroll {}", id(*node)),
        Op::PushFocusTrap { node } => format!("trap {}", id(*node)),
        Op::PopFocusTrap => "untrap".to_owned(),
        Op::SetSelection { node, start, end } => {
            format!("selection {} {start}..{end}", id(*node))
        }
        Op::SelectAll { node } => format!("select-all {}", id(*node)),
        Op::SetValue { node, text } => format!("value {} {text:?}", id(*node)),
        Op::Handler { node, event, phase } => {
            format!("handler {} {event} {phase}", id(*node))
        }
        Op::InstallStylesheet { name } => format!("sheet {name}"),
        Op::RemoveStylesheet { name } => format!("unsheet {name}"),
        Op::Command { what, node } => format!("command {what} {}", id(*node)),
    }
}

/// A node, as it is written in a transcript.
fn id(node: zgui_view::NodeId) -> String {
    format!("#{}", node.backend_bits())
}

/// A boolean, as it is written in a transcript.
fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use zgui_view::{DocumentId, NodeId};

    use super::{Op, Transcript};

    fn node(raw: u64) -> NodeId {
        NodeId::new(DocumentId::FIRST, raw).expect("in range")
    }

    #[test]
    fn every_clone_appends_to_the_same_list() {
        let transcript = Transcript::new();
        let other = transcript.clone();
        transcript.push(Op::Detach { node: node(1) });
        other.push(Op::Detach { node: node(2) });

        assert_eq!(transcript.len(), 2);
        assert_eq!(other.ops(), transcript.ops());
    }

    #[test]
    fn the_rendering_is_one_line_per_operation_and_names_nodes_by_number() {
        let transcript = Transcript::new();
        transcript.push(Op::Create {
            node: node(1),
            what: "control".to_owned(),
        });
        transcript.push(Op::Insert {
            parent: node(1),
            child: node(2),
            before: Some(node(3)),
        });
        transcript.push(Op::PopFocusTrap);

        assert_eq!(
            transcript.to_string(),
            "create #1 control\ninsert #2 into #1 before #3\nuntrap\n"
        );
    }

    #[test]
    fn clearing_forgets_what_the_mount_did_and_keeps_recording_afterwards() {
        let transcript = Transcript::new();
        transcript.push(Op::Detach { node: node(1) });
        transcript.clear();
        assert!(transcript.is_empty());
        transcript.push(Op::Detach { node: node(2) });
        assert_eq!(transcript.len(), 1);
    }
}
