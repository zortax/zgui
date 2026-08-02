//! Two claims this crate makes about itself, checked against the engine it makes them about.
//!
//! The first is the property register: every longhand this stage reads is declared beside the code
//! that reads it, and a declaration is prose until something can prove it wrong. The engine, as
//! built and configured right now, is that something.
//!
//! The second is the relationship between the two keys. The key that decides whether an element has
//! to be *painted again* must name every group a lowering *reads*, or a change to one of those
//! groups would produce no damage and no repaint — silently, and only for some properties.

use zgui_css::StyleDraft;
use zgui_css::parity::Registry;
use zgui_paint::LoweringKey;

/// Every property declaration this crate makes, with the engine configured as it is at run time.
///
/// The feature flags are read at *parse* time and are flipped once per process, so a check taken
/// before they are on reads a build nothing in this framework ever runs against.
fn registry() -> Registry {
    zgui_css::prefs::enable_css_features();
    let mut registry = Registry::new();
    for rows in [
        zgui_paint::lower::REGISTERED,
        zgui_paint::lower::background::REGISTERED,
        zgui_paint::lower::border::REGISTERED,
        zgui_paint::lower::clip::REGISTERED,
        zgui_paint::lower::filter::REGISTERED,
        zgui_paint::lower::outline::REGISTERED,
        zgui_paint::lower::shadow::REGISTERED,
        zgui_paint::lower::transform::REGISTERED,
        zgui_paint::emit::text::REGISTERED,
    ] {
        registry.extend(rows).expect("no row is declared twice");
    }
    registry
}

#[test]
fn every_declaration_still_describes_the_engine_it_was_written_against() {
    let registry = registry();
    assert!(
        registry.len() > 40,
        "the paint stage reads more than {} longhands; a registry this small is not the one",
        registry.len()
    );
    let stale = registry.check();
    assert!(stale.is_empty(), "{stale:?}");
}

#[test]
fn no_property_is_declared_twice_across_the_modules_that_read_it() {
    // `Registry::extend` refuses a duplicate, so building it at all is the assertion — but stating
    // it separately is what makes the failure legible when two modules start reading one property.
    let _ = registry();
}

#[test]
fn the_damage_key_names_every_group_a_lowering_reads() {
    // The invariant that makes a repaint fire at all. It is checked by address rather than by name:
    // the two keys are computed by different crates, so comparing what they *say* would compare two
    // spellings, while comparing the groups they point at compares the thing itself.
    let style = StyleDraft::initial().build();
    let damage = zgui_style::damage::paint_key::paint_key(&style, [0, 0]);
    let named = [
        damage.background,
        damage.border,
        damage.effects,
        damage.outline,
        damage.svg,
        damage.inherited_ui,
        damage.inherited_box,
        damage.text,
        damage.box_,
        damage.position,
        damage.inherited_text,
    ];

    for group in LoweringKey::of(&style).group_identities() {
        assert!(
            named.contains(&group.0),
            "a lowering reads a group at {:#x} that the repaint key does not name, so a change to \
             it would produce no damage at all",
            group.0
        );
    }
}

#[test]
fn the_two_keys_agree_about_the_custom_properties_in_scope() {
    let style = StyleDraft::initial().build();
    let damage = zgui_style::damage::paint_key::paint_key(&style, [0, 0]);
    let lowering = LoweringKey::of(&style);
    assert_eq!(
        (lowering.custom.0.0, lowering.custom.1.0),
        damage.custom,
        "a theme that changes only a custom property has to reach both keys"
    );
}
