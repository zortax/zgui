//! The instrument component tests are written with: a tree that writes down what it was asked to
//! do, a host whose answers a test declares, input a test synthesises, and a clock a test moves.
//!
//! A component is a function from state to a tree of nodes and a set of listeners. Testing one
//! therefore means three things, and this crate is the three:
//!
//! * **what did it build** — [`RecordingDom`] keeps the tree and appends every change to a
//!   [`Transcript`], so an assertion can be about the operations rather than about the final shape
//!   (the two differ, and "it set the class twice" is exactly the kind of defect the final shape
//!   cannot show);
//! * **what did it ask of the engine** — [`ScriptedHost`] answers geometry, focus and time from
//!   what the test declared, and records every command into the *same* transcript, so a claim
//!   about order across the two is answerable;
//! * **what happens when someone uses it** — [`Dispatcher`] aims a real event at a point, resolves
//!   the path down to whatever is there and back up, runs the handlers, and honours what each one
//!   says. The order it runs them in comes from the same rule the real document resolves against.
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//! use zgui_geom::{DevicePx, Point, Rect, Size};
//! use zgui_interned::ElementName;
//! use zgui_testkit_view::{Dispatcher, RecordingDom, ScriptedHost, Transcript};
//! use zgui_view::{DocumentId, Dom, ListenerOptions};
//! use zgui_vocab::EventKind;
//!
//! // One transcript, shared by the tree and the host.
//! let transcript = Transcript::new();
//! let dom = RecordingDom::with_transcript(DocumentId::FIRST, transcript.clone());
//! let host = ScriptedHost::with_transcript(transcript.clone());
//!
//! let root = dom.create_element(ElementName::new("root"));
//! let button = dom.create_element(ElementName::new("control"));
//! dom.insert(root, button, None);
//! host.set_border_box(
//!     button,
//!     Rect::new(
//!         Point::new(DevicePx(0.0), DevicePx(0.0)),
//!         Size::new(DevicePx(80.0), DevicePx(24.0)),
//!     ),
//! );
//!
//! let presses = Rc::new(Cell::new(0));
//! let count = Rc::clone(&presses);
//! dom.add_listener(
//!     button,
//!     EventKind::Click,
//!     ListenerOptions::DEFAULT,
//!     Rc::new(move |cx| {
//!         count.set(count.get() + 1);
//!         cx.request_focus(cx.current);
//!     }),
//! );
//!
//! transcript.clear();
//! let delivered = Dispatcher::new(&dom, &host, root)
//!     .click_at(Point::new(DevicePx(10.0), DevicePx(10.0)));
//!
//! assert_eq!(presses.get(), 1);
//! assert_eq!(delivered.target, Some(button));
//! assert_eq!(
//!     transcript.to_string(),
//!     "handler #2 click target\ncommand request-focus #2\n"
//! );
//! ```
//!
//! # What is deliberately absent
//!
//! There is no document, no style engine, no layout engine and no window here, and that is what
//! makes a test written against this crate a test of the component: nothing it asserts can be
//! failed by a cascade or a layout pass changing. A claim that needs any of those is a claim about
//! the engine, and it belongs in a test that runs one.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod dom;
pub mod fixture;
pub mod host;
pub mod input;
pub mod transcript;

pub use crate::dom::{Edited, Handler, Handlers, RecordingDom, Registration};
pub use crate::fixture::Window;
pub use crate::host::ScriptedHost;
pub use crate::input::{Command, Commands, Delivered, Dispatcher};
pub use crate::transcript::{Op, Transcript};
