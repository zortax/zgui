//! What a handler asked for while it ran.

use zgui_view::{EventSink, NodeId};
use zgui_vocab::EventKind;

use crate::transcript::{Op, Transcript};

/// Something a handler asked the framework to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Route every later pointer event to this node until the button comes up.
    CapturePointer(NodeId),
    /// End that capture.
    ReleasePointer(NodeId),
    /// Move focus here.
    RequestFocus(NodeId),
    /// Dispatch another event on this node.
    Synthesize(NodeId, EventKind),
}

/// The commands one dispatch produced, in order.
///
/// They are collected rather than carried out, exactly as the real runtime collects them: a
/// handler runs while the tree is mid-change, so anything it asks for happens after the dispatch
/// it was asked in has finished. A test asserts on the list.
#[derive(Debug, Default)]
pub struct Commands {
    /// What was asked, in order.
    issued: Vec<Command>,
    /// Where a copy of each is written, so that a claim about order against the tree's own
    /// operations is answerable.
    transcript: Option<Transcript>,
}

impl Commands {
    /// A collector that records nowhere else.
    pub fn new() -> Self {
        Self::default()
    }

    /// A collector that also writes each command into `transcript`.
    pub fn with_transcript(transcript: Transcript) -> Self {
        Self {
            issued: Vec::new(),
            transcript: Some(transcript),
        }
    }

    /// What was asked, in order.
    pub fn issued(&self) -> &[Command] {
        &self.issued
    }

    /// Records one.
    fn record(&mut self, what: &str, node: NodeId, command: Command) {
        if let Some(transcript) = &self.transcript {
            transcript.push(Op::Command {
                what: what.to_owned(),
                node,
            });
        }
        self.issued.push(command);
    }
}

impl EventSink for Commands {
    fn capture_pointer(&mut self, node: NodeId) {
        self.record("capture-pointer", node, Command::CapturePointer(node));
    }

    fn release_pointer(&mut self, node: NodeId) {
        self.record("release-pointer", node, Command::ReleasePointer(node));
    }

    fn request_focus(&mut self, node: NodeId) {
        self.record("request-focus", node, Command::RequestFocus(node));
    }

    fn synthesize(&mut self, node: NodeId, event: EventKind) {
        self.record("synthesize", node, Command::Synthesize(node, event));
    }
}
