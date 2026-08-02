//! How fast a restyle runs, recorded rather than asserted.

use crate::support::Harness;
use zgui_style::StylePool;

#[test]
fn throughput_is_recorded_rather_than_asserted() {
    /// How many rows the measured document holds.
    const ROWS: usize = 4000;

    // A timing is a property of the machine, so nothing here is a threshold. What *is* asserted is
    // the shape: that the pool was used, and that the sequential and parallel runs agree on what
    // they styled. The numbers are printed so that a change in them is visible in a log.
    //
    // Both the engine's own time and the whole call are recorded, because they are different
    // quantities and only the second one is what a frame pays. A figure quoted from the first
    // alone describes the traversal and says nothing about turning what it computed into
    // obligations — which is where a cost that grows with the size of the document rather than
    // with the size of the change would hide.
    fn measure(workers: Option<usize>) -> (usize, std::time::Duration, std::time::Duration) {
        let mut harness = Harness::new();
        let column = harness.append(harness.root, "column");
        for _ in 0..ROWS {
            harness.append(column, "box");
        }
        harness.add_author(
            "box { color: rgb(1, 1, 1) }\n\
             box:nth-child(odd) { background-color: rgb(2, 0, 0) }\n\
             .lit + box { border-top-left-radius: 4px }",
        );
        let started = std::time::Instant::now();
        match workers {
            Some(width) => {
                let pool = StylePool::new(width);
                let pass = harness.frame_on(&pool);
                assert!(
                    pass.workers > 1,
                    "a pool that was handed over and never used would make the figure meaningless"
                );
                (pass.styled, pass.engine_time, started.elapsed())
            }
            None => {
                let pass = harness.frame();
                assert_eq!(pass.workers, 1);
                (pass.styled, pass.engine_time, started.elapsed())
            }
        }
    }

    let (sequential_styled, sequential, sequential_frame) = measure(None);
    let (parallel_styled, parallel, parallel_frame) = measure(Some(6));
    assert_eq!(
        sequential_styled, parallel_styled,
        "the two runs styled the same document"
    );

    let rate = |elapsed: std::time::Duration| sequential_styled as f64 / elapsed.as_secs_f64();
    eprintln!(
        "style throughput over {sequential_styled} elements: \
         {:.0} styles/s at one worker ({sequential:?}), \
         {:.0} styles/s at six ({parallel:?}); \
         the whole restyle call, damage translation included, took {sequential_frame:?} \
         sequentially and {parallel_frame:?} across six",
        rate(sequential),
        rate(parallel),
    );
}
