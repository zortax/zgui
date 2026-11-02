//! Tests for the allocation tags.

use std::collections::BTreeSet;

use super::{TAGS_ENABLED, Tag, current, enter, phase_of_mark, scope};
use crate::Phase;

#[test]
fn every_tag_has_a_distinct_label_and_its_own_index() {
    let labels: BTreeSet<&str> = Tag::ALL.iter().map(|tag| tag.label()).collect();
    assert_eq!(labels.len(), Tag::COUNT);
    for (index, tag) in Tag::ALL.into_iter().enumerate() {
        assert_eq!(tag.index(), index);
        assert_eq!(Tag::from_index(index), tag);
    }
    assert_eq!(Tag::from_index(Tag::COUNT), Tag::Other);
}

#[test]
fn a_scope_sets_the_tag_and_puts_the_previous_one_back() {
    let (outer, inner) = scope(Tag::LayoutEngine, || {
        let inner = scope(Tag::Shaping, current);
        (current(), inner)
    });
    if TAGS_ENABLED {
        assert_eq!((outer, inner), (Tag::LayoutEngine, Tag::Shaping));
    } else {
        assert_eq!((outer, inner), (Tag::Other, Tag::Other));
    }
    assert_eq!(current(), Tag::Other);
}

#[test]
fn a_scope_puts_the_tag_back_when_its_body_unwinds() {
    let unwound = std::panic::catch_unwind(|| scope(Tag::Dom, || panic!("the body unwinds")));
    assert!(unwound.is_err());
    assert_eq!(current(), Tag::Other);
}

#[test]
fn a_guard_holds_the_tag_while_it_lives() {
    let guard = enter(Tag::A11y);
    let expected = if TAGS_ENABLED { Tag::A11y } else { Tag::Other };
    assert_eq!(current(), expected);
    drop(guard);
    assert_eq!(current(), Tag::Other);
}

#[test]
fn the_frame_marks_name_their_stages() {
    assert_eq!(phase_of_mark("f.restyle"), Some(Phase::Restyle));
    assert_eq!(phase_of_mark("f.layout"), Some(Phase::Layout));
    assert_eq!(phase_of_mark("r.record"), Some(Phase::Render));
    assert_eq!(phase_of_mark("f.end"), Some(Phase::Recycle));
    assert_eq!(phase_of_mark("d.boxes"), None);
    assert_eq!(phase_of_mark("w.resize"), None);
    assert_eq!(Phase::Render.alloc_tag(), Tag::Renderer);
    assert_eq!(Phase::Restyle.alloc_tag(), Tag::Style);
}

#[test]
fn a_mark_sets_the_tag_only_when_tags_are_compiled_in() {
    let _scope = enter(Tag::Other);
    crate::latency::mark("f.boxes");
    let expected = if TAGS_ENABLED {
        Tag::BoxTree
    } else {
        Tag::Other
    };
    assert_eq!(current(), expected);
    crate::latency::mark("d.boxes");
    assert_eq!(current(), expected, "a damage note begins no stage");
}

#[test]
fn an_entered_stage_tags_what_it_allocates() {
    let staged = Phase::Emit.enter();
    let expected = if TAGS_ENABLED {
        Tag::PaintRecord
    } else {
        Tag::Other
    };
    assert_eq!(current(), expected);
    drop(staged);
    assert_eq!(current(), Tag::Other);
}
