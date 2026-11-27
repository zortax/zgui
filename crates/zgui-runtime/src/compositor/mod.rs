//! The compositor windows draw through.
//!
//! One driver of a compositor serves every window of an application. Each window is a client of
//! it with an output of its own, and the driver's renderer draws that output's frames.

mod client;
mod events;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use iceblink::CompositorClient;

pub(crate) use self::client::Client;
pub(crate) use self::events::{Answers, Drawn};

/// The driver every window of an application draws through.
#[derive(Clone)]
pub(crate) struct Shared {
    driver: Rc<RefCell<Box<dyn CompositorClient>>>,
    /// The steps the windows gave the driver.
    steps: Rc<Cell<u64>>,
}

impl Shared {
    /// Shares `driver` between the windows of an application.
    pub(crate) fn new(driver: Box<dyn CompositorClient>) -> Self {
        Self {
            driver: Rc::new(RefCell::new(driver)),
            steps: Rc::new(Cell::new(0)),
        }
    }
}
