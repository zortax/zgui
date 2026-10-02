//! The application menu, rebuilt whenever what it was built from changes.

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::AppMenu;
use zgui_reactive::RenderEffect;

/// The menu the application builds, and whether the desktop has seen its last version.
pub(crate) struct MenuSlot {
    /// The effect that builds the menu, kept for as long as the application runs.
    _effect: RenderEffect<()>,
    /// The version the desktop has not been given yet.
    pending: Rc<RefCell<Option<AppMenu>>>,
}

impl MenuSlot {
    /// Builds the menu with `build` now, and again whenever a signal it read changes.
    pub(crate) fn new(build: Box<dyn Fn() -> AppMenu>) -> Self {
        let pending = Rc::new(RefCell::new(None));
        let effect = {
            let pending = Rc::clone(&pending);
            RenderEffect::new(move |_| {
                let menu = build();
                *pending.borrow_mut() = Some(menu);
            })
        };
        Self {
            _effect: effect,
            pending,
        }
    }

    /// The version the desktop has not been given yet.
    pub(crate) fn take(&self) -> Option<AppMenu> {
        self.pending.borrow_mut().take()
    }
}
