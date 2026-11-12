//! Style sheets a view installed for itself, applied to this window's rule set.
//!
//! An application's sheet is installed once, when the window opens. A component's sheet cannot be:
//! a component is a function anyone may call, it carries its own rules, and the program using it
//! never wrote them down. So a view asks, the ask is queued as a command, and this is where the
//! queue meets the style engine — after the reactive flush and before the restyle, so a component
//! that mounted this frame is styled by its own sheet in the frame it appeared in.

use zgui_geom::{DevicePx, Edges};
use zgui_style::{SheetOrigin, SheetSource};

use crate::window::Window;

/// Sheet holding the environment custom properties.
const ENVIRONMENT_SHEET: &str = "zgui-environment";

impl Window {
    /// Records system insets. A keyboard that grows scrolls the focused element back into view.
    pub(crate) fn insets_changed(
        &mut self,
        safe_area: Edges<DevicePx>,
        keyboard: DevicePx,
    ) -> bool {
        if self.insets == safe_area && self.keyboard == keyboard {
            return false;
        }
        let grew = keyboard.0 > self.keyboard.0;
        self.insets = safe_area;
        self.keyboard = keyboard;
        self.publish_environment();
        if grew && let Some(node) = self.router.interaction().focus.focused() {
            self.owed_scrolls.push((
                node,
                zgui_view::ScrollTarget::IntoView,
                zgui_view::ScrollBehavior::Instant,
            ));
        }
        true
    }

    /// Publishes the platform environment as root custom properties: insets in CSS pixels as
    /// `--zgui-safe-area-top`, `-right`, `-bottom`, `-left` and `--zgui-keyboard-height`, and the
    /// preferred text size factor as `--zgui-text-scale`. A right-to-left platform also sets
    /// `direction: rtl` on the root.
    pub(crate) fn publish_environment(&mut self) {
        let px = |length: DevicePx| length.0 / self.scale;
        let css = format!(
            ":root {{ --zgui-safe-area-top: {}px; --zgui-safe-area-right: {}px; \
             --zgui-safe-area-bottom: {}px; --zgui-safe-area-left: {}px; \
             --zgui-keyboard-height: {}px; --zgui-text-scale: {}{} }}",
            px(self.insets.top),
            px(self.insets.right),
            px(self.insets.bottom),
            px(self.insets.left),
            px(self.keyboard),
            self.text_scale,
            if self.rtl { "; direction: rtl" } else { "" },
        );
        self.install_view_sheet(ENVIRONMENT_SHEET, &css);
    }

    /// Installs a view's style sheet under `name`, or replaces it when the name is taken.
    ///
    /// Replacement keeps the sheet's place in the cascade. Removing and adding would move it to
    /// the end of the author origin, where it would begin winning against every sheet that used to
    /// beat it — so a theme that changed a colour would also, invisibly, change what wins.
    pub(crate) fn install_view_sheet(&mut self, name: &str, css: &str) {
        let document = self.document.borrow();
        let diagnostics = match self.view_sheets.get(name) {
            Some(handle) => {
                let diagnostics =
                    self.engine
                        .replace_sheet(&document, handle, SheetSource::Text(css));
                // Replaced rules may have styled any element in the tree, and rules that are gone
                // leave no selector to invalidate by.
                self.engine.force_author_rules_dirty();
                diagnostics
            }
            None => {
                // An added sheet is appended to the rule set and invalidates by its own
                // selectors. The elements the component that installs it is building have no
                // style yet, so they are styled against it in this frame whatever it invalidates.
                let (handle, diagnostics) =
                    self.engine
                        .add_sheet(&document, SheetOrigin::Author, SheetSource::Text(css));
                self.view_sheets.insert(name.to_owned(), handle);
                diagnostics
            }
        };
        drop(document);
        for report in diagnostics.iter() {
            tracing::warn!(
                target: "zgui::css",
                sheet = name,
                "{}",
                report.message
            );
        }
    }

    /// Removes the sheet installed under `name`. Removing one that is not installed does nothing.
    ///
    /// A sheet is removed by letting go of its handle: the rule set reclaims every sheet whose
    /// handle has been dropped, which is what makes "forgot to remove it" impossible to write.
    pub(crate) fn remove_view_sheet(&mut self, name: &str) {
        drop(self.view_sheets.remove(name));
    }
}
