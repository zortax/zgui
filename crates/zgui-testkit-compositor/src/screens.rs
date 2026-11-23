//! The screens a test's windows draw onto, readable after their renderers went to the driver.

use std::cell::RefCell;

use compositor_api::{Renderer, RendererFactory};
use compositor_types::RenderTarget;

use crate::screen::{Screen, Shown};

thread_local! {
    /// What each screen made on this thread shows, in the order they were made.
    static MADE: RefCell<Vec<Shown>> = const { RefCell::new(Vec::new()) };
}

/// Returns a factory of a [`Screen`] configured for `target`, whose frames [`screens`] reads.
pub fn screen(target: RenderTarget) -> RendererFactory {
    let shown = Shown::default();
    MADE.with(|made| made.borrow_mut().push(shown.clone()));
    Box::new(move || {
        let mut screen = Screen::new().showing_into(shown);
        screen.configure(target);
        Ok(Box::new(screen))
    })
}

/// Returns what each screen made on this thread by [`screen`] shows, in the order they were
/// made.
pub fn screens() -> Vec<Shown> {
    MADE.with(|made| made.borrow().clone())
}
