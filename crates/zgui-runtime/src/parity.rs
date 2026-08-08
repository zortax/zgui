//! Which CSS longhands this crate reads the value of.
//!
//! Three, and all for the same reason: they describe something a *window* has rather than
//! something a document is. The cursor belongs to the pointer that is over the window, the caret is
//! drawn where the window's focus is, and what may be selected is a question about a pointer
//! gesture — none of the three exists in a laid-out document, so none can be read anywhere a
//! laid-out document is all there is.
//!
//! That is also why these are the only rows in the register that no probe can settle. The evidence
//! harness lays a document out; it has no window, nothing is over it, nothing has focus in it and
//! nothing is pressed on it. All three therefore name themselves in the escape list, with that
//! reason.
//!
//! ```
//! use zgui_css::parity::Registry;
//!
//! zgui_css::enable_css_features();
//! let mut registry = Registry::new();
//! registry.extend(zgui_runtime::parity::REGISTERED).expect("no row declared twice");
//! assert_eq!(registry.counts().implemented, 3);
//! ```

use zgui_css::parity::Support;

zgui_css::register_properties! {
    cursor => Support::Implemented("zgui-runtime::window::cursor"),
    caret_color => Support::Implemented("zgui-runtime::window::caret"),
    user_select => Support::Implemented("zgui-runtime::window::select"),
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::Registry;

    /// Every row is a claim about the engine, and the engine is asked whether it is still true.
    #[test]
    fn every_declaration_still_matches_what_the_engine_says() {
        zgui_css::enable_css_features();
        let mut registry = Registry::new();
        registry
            .extend(super::REGISTERED)
            .expect("no row is declared twice");
        assert_eq!(registry.len(), super::REGISTERED.len());
        assert!(
            registry.check().is_empty(),
            "a declaration here contradicts the engine as it is built: {:?}",
            registry.check(),
        );
    }
}
