//! Style sheets a view installs for itself.
//!
//! An application's own sheet is decided before it runs. A *component's* sheet cannot be: a
//! component is a function anyone may call from anywhere, it carries its own rules, and the
//! program that uses it never wrote them down. So a view installs its sheet from its own body, by
//! name, and the name makes that idempotent — a hundred buttons install one sheet.
//!
//! ```
//! use zgui_reactive::{Mounted, install};
//! use zgui_view::stub::StubHost;
//! use zgui_view::{HostHandle, install_stylesheet, provide_host};
//! use std::rc::Rc;
//!
//! install().unwrap();
//! let stub = Rc::new(StubHost::new());
//! let window = Mounted::new();
//! window.with(|| provide_host(HostHandle::from_rc(stub.clone())));
//!
//! // What a component's body does, once per instance.
//! window.with(|| {
//!     install_stylesheet("zui-button", ".zui-button { display: inline-flex }");
//!     install_stylesheet("zui-button", ".zui-button { display: inline-flex }");
//! });
//!
//! assert_eq!(stub.stylesheet_count(), 1, "the second install was the same sheet");
//! window.unmount();
//! ```

use core::fmt::{self, Debug};

use crate::cx::current_host;
use crate::host::HostHandle;

/// A style sheet whose lifetime is a view's, removed when the guard is dropped.
///
/// [`install_stylesheet`] is for a sheet that belongs to a *component type* — a button's rules,
/// which the next button would only put back. This is for a sheet whose content is **state**: a
/// theme, a set of rules generated from data, anything that stops being true when the view that
/// generated it goes away.
///
/// The host is captured when the sheet is installed rather than resolved when it is removed,
/// because a scope's cleanups also run when the last handle to its owner is dropped — which can
/// happen with no scope current at all. A guard that looked the window up on the way out would
/// work in a test that unmounts tidily and leak in the program that does not.
///
/// ```
/// use std::rc::Rc;
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::StubHost;
/// use zgui_view::{HostHandle, Stylesheet, provide_host};
///
/// install().unwrap();
/// let stub = Rc::new(StubHost::new());
/// let window = Mounted::new();
/// window.with(|| provide_host(HostHandle::from_rc(stub.clone())));
///
/// let sheet = window
///     .with(|| Stylesheet::install("theme", ":root { --accent: red }"))
///     .expect("inside a window");
/// assert_eq!(stub.stylesheet_count(), 1);
///
/// // Replacing keeps the sheet's place in the cascade.
/// sheet.replace(":root { --accent: blue }");
/// assert_eq!(stub.stylesheet_count(), 1);
/// assert!(stub.stylesheet("theme").expect("installed").contains("blue"));
///
/// drop(sheet);
/// assert_eq!(stub.stylesheet_count(), 0);
/// window.unmount();
/// ```
#[must_use = "dropping the guard removes the sheet immediately"]
pub struct Stylesheet {
    /// The engine holding it.
    host: HostHandle,
    /// What it is installed under.
    name: String,
}

impl Stylesheet {
    /// Installs `css` under `name` in the enclosing window, and keeps it there.
    ///
    /// `None` outside a window's scope, where there is no document to install into.
    pub fn install(name: impl Into<String>, css: &str) -> Option<Self> {
        let host = current_host()?;
        let name = name.into();
        host.install_stylesheet(&name, css);
        Some(Self { host, name })
    }

    /// Replaces the sheet's text, keeping its place in the cascade.
    ///
    /// Writing the text it already has does nothing at all, so a caller may hand it whatever it
    /// last computed without checking first.
    pub fn replace(&self, css: &str) {
        self.host.install_stylesheet(&self.name, css);
    }

    /// What the sheet is installed under.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Debug for Stylesheet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Stylesheet")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Drop for Stylesheet {
    fn drop(&mut self) {
        self.host.remove_stylesheet(&self.name);
    }
}

/// Installs `css` as a style sheet of the enclosing window's document, under `name`.
///
/// The sheet lands at the author origin. Installing under a name that is already installed
/// replaces that sheet's text without moving it in the cascade, and installing text that is
/// already there does nothing at all — so a component may install its own sheet unconditionally
/// from its body, and a theme may re-install a sheet it has just regenerated.
///
/// Names are the caller's to choose and are shared across a whole document, so a library gives
/// its sheets a prefix of its own. The class a
/// [`style!`](https://docs.rs/zgui-view-macro) block generates is a good name: it is derived from
/// the sheet's own text and collides with nothing.
///
/// Called outside a window's scope this does nothing, and says so in a debug build: a sheet that
/// silently never reached a document is a component that renders unstyled with no error anywhere.
pub fn install_stylesheet(name: &str, css: &str) {
    match current_host() {
        Some(host) => host.install_stylesheet(name, css),
        None => debug_assert!(
            false,
            "install_stylesheet({name:?}) was called outside a window's scope, so no document \
             received it"
        ),
    }
}

/// Removes the sheet installed under `name` from the enclosing window's document.
///
/// Removing a name that is not installed does nothing. A component's own sheet is normally left
/// installed for the life of the window — the next instance would only put it back — so this is
/// for sheets whose *content* is state, such as a theme that is being torn down.
pub fn remove_stylesheet(name: &str) {
    match current_host() {
        Some(host) => host.remove_stylesheet(name),
        None => debug_assert!(
            false,
            "remove_stylesheet({name:?}) was called outside a window's scope"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_reactive::{Mounted, install};

    use super::{install_stylesheet, remove_stylesheet};
    use crate::cx::provide_host;
    use crate::host::HostHandle;
    use crate::stub::StubHost;

    fn window() -> (Mounted, Rc<StubHost>) {
        install().ok();
        let stub = Rc::new(StubHost::new());
        let window = Mounted::new();
        window.with(|| provide_host(HostHandle::from_rc(Rc::clone(&stub) as Rc<_>)));
        (window, stub)
    }

    #[test]
    fn a_second_install_under_one_name_replaces_the_text_rather_than_adding_a_sheet() {
        let (window, host) = window();
        window.with(|| {
            install_stylesheet("theme", ":root { --a: 1px }");
            install_stylesheet("theme", ":root { --a: 2px }");
        });

        assert_eq!(host.stylesheet_count(), 1);
        assert_eq!(
            host.stylesheet("theme").as_deref(),
            Some(":root { --a: 2px }")
        );
        assert_eq!(
            host.stylesheet_installs(),
            2,
            "both reached the engine: the second changed the text"
        );
        window.unmount();
    }

    #[test]
    fn installing_the_same_text_again_does_not_reach_the_engine() {
        let (window, host) = window();
        window.with(|| {
            install_stylesheet("button", ".b { color: red }");
            install_stylesheet("button", ".b { color: red }");
            install_stylesheet("button", ".b { color: red }");
        });

        assert_eq!(host.stylesheet_installs(), 1);
        window.unmount();
    }

    #[test]
    fn a_guard_removes_its_sheet_even_when_nothing_is_in_scope_any_more() {
        // The case the guard exists for. A scope's cleanups also run when the last handle to its
        // owner is dropped, which happens with no window's scope current — so a release that
        // resolved the window on the way out would leak here and nowhere a tidy test looks.
        let (window, host) = window();
        let sheet = window
            .with(|| super::Stylesheet::install("theme", ":root {}"))
            .expect("inside a window");
        assert_eq!(host.stylesheet_count(), 1);

        // Dropped from outside every scope, exactly as an owner's drop would.
        drop(sheet);
        assert_eq!(host.stylesheet_count(), 0);
        window.unmount();
    }

    #[test]
    fn a_removed_sheet_is_gone_and_removing_it_twice_is_not_an_error() {
        let (window, host) = window();
        window.with(|| {
            install_stylesheet("theme", ":root {}");
            remove_stylesheet("theme");
            remove_stylesheet("theme");
        });

        assert_eq!(host.stylesheet_count(), 0);
        assert_eq!(host.stylesheet("theme"), None);
        window.unmount();
    }
}
