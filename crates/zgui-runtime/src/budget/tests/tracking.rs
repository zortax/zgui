//! Turning a cache's running lookup total into the frame it was last read in.

use crate::budget::manager::Tracked;
use crate::budget::tests::at;

#[test]
fn a_frame_that_read_the_cache_moves_its_stamp() {
    let mut tracked = Tracked::default();
    tracked.note(at(1), 0, false);
    tracked.note(at(2), 7, false);

    assert_eq!(tracked.last_used(), at(2));
}

#[test]
fn a_frame_that_read_nothing_leaves_the_stamp_where_it_was() {
    let mut tracked = Tracked::default();
    tracked.note(at(1), 7, false);
    tracked.note(at(2), 7, false);
    tracked.note(at(3), 7, false);

    assert_eq!(
        tracked.last_used(),
        at(1),
        "three frames passed and the total did not move, so the cache has been cold for two of them"
    );
}

/// The half that exists because of the replay path.
///
/// A window drawing a static label replays the range that holds it and asks the atlas for none of
/// the glyphs in it — the lookup total does not move for as long as the label is on the screen. The
/// tiles are held by a live record all the while, and that is what has to keep the atlas warm; a
/// budget reading lookups alone would decide the hottest cache in the window was the coldest.
#[test]
fn content_held_by_a_live_record_keeps_the_cache_warm_without_a_single_lookup() {
    let mut tracked = Tracked::default();
    tracked.note(at(1), 7, false);
    tracked.note(at(2), 7, true);

    assert_eq!(tracked.last_used(), at(2));
}
