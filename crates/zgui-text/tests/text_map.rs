//! The way back from a shaped offset to the document.

use zgui_text::{SourcePos, TextMap};

/// A direction control at the front of the generated string belongs to no source position.
#[test]
fn the_direction_control_maps_to_nothing() {
    // "\u{200f}hello" — three bytes of control, then the source text.
    let mut map = TextMap::new();
    map.push(3..8, 0, 0);

    assert_eq!(map.to_source(0), None, "the control has no source");
    assert_eq!(map.to_source(1), None);
    assert_eq!(map.to_source(3), Some(SourcePos { run: 0, offset: 0 }));

    // But a hit on it is still a hit, and snaps to the first real position.
    assert_eq!(
        map.to_source_snapped(0),
        Some(SourcePos { run: 0, offset: 0 }),
    );
}

/// Collapsed white space: only the first byte of a run survives, and the rest map to nothing.
#[test]
fn collapsed_white_space_keeps_one_byte() {
    // "a   b" in the source became "a b": the space at generated offset 1 is the first of three.
    let mut map = TextMap::new();
    map.push(0..2, 0, 0);
    map.push(2..3, 0, 4);

    assert_eq!(map.to_source(0), Some(SourcePos { run: 0, offset: 0 }));
    assert_eq!(map.to_source(1), Some(SourcePos { run: 0, offset: 1 }));
    assert_eq!(map.to_source(2), Some(SourcePos { run: 0, offset: 4 }));
    assert_eq!(map.to_source(3), None, "past the end");

    // The two swallowed spaces have no generated offset of their own.
    assert_eq!(map.to_generated(SourcePos { run: 0, offset: 2 }), None);
    assert_eq!(map.to_generated(SourcePos { run: 0, offset: 3 }), None);
}

/// Contiguous stretches merge, so untouched text costs one entry however long it is.
#[test]
fn contiguous_stretches_merge() {
    let mut map = TextMap::new();
    for offset in 0..64 {
        map.push(offset..offset + 1, 0, offset);
    }
    assert_eq!(map.segments().len(), 1, "one entry for sixty-four bytes");
    assert_eq!(map.to_source(63), Some(SourcePos { run: 0, offset: 63 }));
}

/// A stretch that is contiguous in the generated string but not in the source stays separate.
#[test]
fn a_jump_in_the_source_is_not_merged() {
    let mut map = TextMap::new();
    map.push(0..4, 0, 0);
    map.push(4..8, 0, 100);
    assert_eq!(map.segments().len(), 2);
    assert_eq!(
        map.to_source(4),
        Some(SourcePos {
            run: 0,
            offset: 100
        })
    );
}

/// Several runs keep their own offsets.
#[test]
fn offsets_are_per_run() {
    let mut map = TextMap::new();
    map.push(0..5, 0, 0);
    map.push(5..9, 1, 0);

    assert_eq!(map.to_source(6), Some(SourcePos { run: 1, offset: 1 }));
    assert_eq!(map.to_generated(SourcePos { run: 1, offset: 0 }), Some(5));
    assert_eq!(map.to_generated(SourcePos { run: 0, offset: 0 }), Some(0));
}

/// An empty stretch records nothing, so an empty run cannot swallow a lookup.
#[test]
fn empty_stretches_are_ignored() {
    let mut map = TextMap::new();
    map.push(0..0, 0, 0);
    assert!(map.is_empty());
    assert_eq!(map.to_source(0), None);
}
