//! The compositor windows draw through.
//!
//! One compositor serves every window of an application. Each window is a client of it with an
//! output of its own, and the window's renderer draws that output's frames.

mod client;
mod draw;
mod mailbox;

use std::cell::RefCell;
use std::rc::Rc;

use compositor_api::Compositor;

pub(crate) use self::client::Client;
pub(crate) use self::draw::Drawn;

use self::mailbox::Mailbox;

/// The compositor every window of an application draws through, and the events it reported that
/// no window took yet.
#[derive(Clone)]
pub(crate) struct Shared {
    compositor: Rc<RefCell<Box<dyn Compositor>>>,
    mailbox: Rc<RefCell<Mailbox>>,
}

impl Shared {
    /// Shares `compositor` between the windows of an application.
    pub(crate) fn new(compositor: Box<dyn Compositor>) -> Self {
        Self {
            compositor: Rc::new(RefCell::new(compositor)),
            mailbox: Rc::default(),
        }
    }
}
