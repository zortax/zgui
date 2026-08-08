//! Choosing which theme goes in each of the provider's two slots, through the gallery's own chooser.
//!
//! The mechanism underneath — writing a theme into a slot re-declares that slot's tokens and leaves
//! the other alone — is held by `crates/zgui-ui-tokens/tests/theme.rs`, against the provider itself.
//! What this adds is the half a user actually touches: a control in the masthead, driven by a
//! pointer, that has to end up writing the preset it was pressed on into the signal the provider is
//! reading. A chooser that shows five names and writes none of them looks exactly like a working
//! one from the outside.
//!
//! The gallery is mounted from its own source, so what is driven is what is shipped.

#[path = "../examples/gallery/app.rs"]
#[allow(
    dead_code,
    reason = "the gallery names the window size it ships at; this fixture takes the stage's"
)]
mod app;
#[path = "../examples/gallery/section/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "the gallery's sections are one module; this one exercises the chooser alone"
)]
mod section;
#[path = "../examples/gallery/shell.rs"]
#[allow(
    dead_code,
    reason = "the shell is one module; this uses the chooser it carries"
)]
mod shell;

mod desktop;

use zgui::view;
use zgui_ui_tokens::Preset;

use crate::app::GalleryProps;
use crate::desktop::stage::Stage;

/// The gallery, settled, on a window big enough for the masthead to be laid out across.
fn gallery() -> Stage {
    let mut stage = Stage::open(crate::shell::SHEET, || view! { Gallery() });
    stage.settle();
    stage
}

#[test]
fn the_masthead_offers_the_themes_the_library_ships() {
    let mut stage = gallery();
    // Both slots start on the same one, so pressing either control opens a list of all of them.
    stage.click_saying(Preset::default().label());
    stage.settle();

    // Every one of them has a row with a box of its own. Asked through the census rather than
    // through `shows`, which is a question about what a person can read on the page: the rows sit
    // on a floating surface whose own box the page never gets, and the answer there is about the
    // surface rather than about the list.
    for preset in Preset::ALL.iter().copied() {
        assert!(
            stage.census().control(preset.label()).is_some(),
            "the chooser does not offer {}",
            preset.name()
        );
    }
}

#[test]
fn picking_a_theme_puts_it_in_the_slot_that_was_asked() {
    let mut stage = gallery();
    assert!(stage.shows(Preset::Base.label()), "it starts on the base");

    stage.click_saying(Preset::Base.label());
    stage.settle();
    stage.click_saying(Preset::Ember.label());
    stage.settle();

    // The control shows what is in the slot, and what is in the slot is what the provider declares.
    assert!(
        stage.shows(Preset::Ember.label()),
        "pressing a name in the list left the slot on the one it started with"
    );
    assert!(
        !stage.shows(Preset::Ocean.label()),
        "the list is still open over the page after a choice was made"
    );
}
