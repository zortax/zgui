//! Effect declarations, found by the name a style sheet writes.

use zgui_css::shader::{ShaderField, ShaderMode, ShaderReads, declare, named};

const FIELDS: [ShaderField; 1] = [ShaderField {
    name: "amount",
    offset: 0,
    size: 4,
}];

#[test]
fn a_declaration_is_found_by_its_name() {
    let id = declare(
        "test-found",
        ShaderMode::Coverage,
        ShaderReads::NOTHING,
        &FIELDS,
        0.0,
    );

    let found = named("test-found").expect("the declaration is found");
    assert_eq!(found.id, id);
    assert_eq!(found.mode, ShaderMode::Coverage);
    assert_eq!(found.field("amount").map(|field| field.offset), Some(0));
    assert_eq!(found.field("missing"), None);
}

#[test]
fn a_reach_below_zero_is_stored_as_zero() {
    declare(
        "test-negative-reach",
        ShaderMode::Filter,
        ShaderReads::NOTHING,
        &[],
        -3.0,
    );

    assert_eq!(
        named("test-negative-reach").map(|held| held.reach),
        Some(0.0)
    );
}

#[test]
fn a_name_nothing_declared_finds_nothing() {
    assert!(named("test-never-declared").is_none());
}

#[test]
fn handles_are_never_reused() {
    let first = declare(
        "test-one",
        ShaderMode::Paint,
        ShaderReads::NOTHING,
        &[],
        0.0,
    );
    let second = declare(
        "test-two",
        ShaderMode::Paint,
        ShaderReads::NOTHING,
        &[],
        0.0,
    );

    assert_ne!(first, second);
}

#[test]
fn the_last_declaration_of_a_name_wins() {
    declare(
        "test-twice",
        ShaderMode::Paint,
        ShaderReads::NOTHING,
        &[],
        0.0,
    );
    let second = declare(
        "test-twice",
        ShaderMode::Coverage,
        ShaderReads::NOTHING,
        &[],
        0.0,
    );

    assert_eq!(named("test-twice").map(|held| held.id), Some(second));
}
