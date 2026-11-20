//! The compositor windows draw through.
//!
//! One compositor serves every window of an application. Each window is a client of it with an
//! output of its own, and the window's renderer draws that output's frames.

mod client;
mod draw;

use std::cell::RefCell;
use std::rc::Rc;

use compositor_api::Compositor;

pub(crate) use self::client::Client;

/// The compositor every window of an application draws through.
pub(crate) type Shared = Rc<RefCell<Box<dyn Compositor>>>;
