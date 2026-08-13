//! Forked shapers on worker threads agree with the shaper they were forked from.
//!
//! The pre-shape prepass shapes cache misses on forks and lets the frame's own shaper break the
//! results, so two things have to hold: a fork's shaped output equals the original's, and a
//! paragraph shaped by one shaper breaks identically under another. Both are asserted here, and
//! the whole file is a thread-sanitiser subject because the forks share one font system.

mod support;

use support::Fixture;
use zgui_geom::CssPx;
use zgui_text::{BreakRequest, ParagraphKey, ParagraphShaper};
use zgui_text_parley::Controls;
use zgui_text_style::Direction;

/// One paragraph's shaped identity: its key, and its content widths by their bits.
type Shape = (ParagraphKey, u32, u32);

#[test]
fn forked_shapers_shape_equal_results_across_threads() {
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let texts: Vec<String> = (0..24)
        .map(|index| format!("paragraph number {index} with words enough to shape and to break"))
        .collect();
    let fixtures: Vec<Fixture> = texts
        .iter()
        .map(|text| Fixture::new(text, Direction::LeftToRight))
        .collect();

    let serial: Vec<Shape> = fixtures
        .iter()
        .map(|fixture| {
            let content = fixture.content();
            let shaped = shaper.shape(&content);
            let widths = shaped.content_widths();
            (shaped.key(), widths.min.0.to_bits(), widths.max.0.to_bits())
        })
        .collect();

    let forks: Vec<_> = (0..4)
        .map(|_| shaper.fork().expect("the parley shaper forks"))
        .collect();
    let per_chunk = fixtures.len().div_ceil(forks.len());
    let parallel: Vec<Shape> = std::thread::scope(|scope| {
        let handles: Vec<_> = forks
            .into_iter()
            .zip(fixtures.chunks(per_chunk))
            .map(|(mut fork, chunk)| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|fixture| {
                            let content = fixture.content();
                            let shaped = fork.shape(&content);
                            let widths = shaped.content_widths();
                            (shaped.key(), widths.min.0.to_bits(), widths.max.0.to_bits())
                        })
                        .collect::<Vec<Shape>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a fork's thread finished"))
            .collect()
    });

    assert_eq!(parallel, serial, "a fork shaped something differently");
}

#[test]
fn a_fork_shaped_paragraph_breaks_identically_under_the_original_shaper() {
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let fixture = Fixture::new(
        "the quick brown fox jumps over the lazy dog while the dog sleeps on",
        Direction::LeftToRight,
    );
    let content = fixture.content();

    let mut serial = shaper.shape(&content);
    let serial_broken = shaper.break_lines(
        &mut serial,
        &BreakRequest::new(&content, Some(CssPx(200.0))),
    );

    let mut fork = shaper.fork().expect("the parley shaper forks");
    let mut forked = std::thread::scope(|scope| {
        scope
            .spawn(|| fork.shape(&content))
            .join()
            .expect("the fork's thread finished")
    });
    // The frame's own shaper breaks what the fork shaped, exactly as the pre-shape cache is used.
    let forked_broken = shaper.break_lines(
        &mut forked,
        &BreakRequest::new(&content, Some(CssPx(200.0))),
    );

    assert_eq!(
        forked_broken.geometry.lines.len(),
        serial_broken.geometry.lines.len()
    );
    assert_eq!(
        forked_broken.geometry.size.width.0.to_bits(),
        serial_broken.geometry.size.width.0.to_bits()
    );
    assert_eq!(
        forked_broken.geometry.size.height.0.to_bits(),
        serial_broken.geometry.size.height.0.to_bits()
    );
}
