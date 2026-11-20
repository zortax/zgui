//! The atlas level of the compositor an application draws through.

use zgui::app::Fonts;
use zgui::runtime::budget::limits::ATLAS_BYTES;

#[test]
fn the_application_compositor_holds_its_atlas_to_the_runtime_level() {
    let compositor = zgui::app::compositor(&Fonts::shipped_only());
    assert_eq!(compositor.budgets().atlas_bytes, Some(ATLAS_BYTES));
}
