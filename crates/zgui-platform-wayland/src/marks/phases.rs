//! The phases of one frame, read from the latency marks of the frame pipeline.
//!
//! The runtime and the renderer write a latency mark at each stage boundary. A phase starts at
//! one mark and ends at the next boundary mark of the same frame. The frame starts at the first
//! `f.begin` mark after its redraw began. A phase whose marks are missing is left out.

use std::time::Instant;

/// One phase of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Phase {
    /// The span name in the compositor trace.
    pub(crate) name: &'static str,
    /// When the phase began.
    pub(crate) begin: Instant,
    /// When the phase ended.
    pub(crate) end: Instant,
}

/// A phase name, the mark that begins it, and the marks that can end it.
struct Boundary {
    /// The span name.
    name: &'static str,
    /// The mark that begins the phase.
    from: &'static str,
    /// The marks that end the phase. The first one after `from` wins.
    to: &'static [&'static str],
    /// The phase ends at the present when no end mark follows.
    open: bool,
}

/// The phases, in pipeline order.
const BOUNDARIES: &[Boundary] = &[
    // The queued platform events are dispatched to listeners.
    Boundary {
        name: "events",
        from: "f.drain",
        to: &["f.timers"],
        open: false,
    },
    // Timers, scrolling, animations, the reactive flush and the commands.
    Boundary {
        name: "update",
        from: "f.timers",
        to: &["f.restyle"],
        open: false,
    },
    // Selector matching and the cascade.
    Boundary {
        name: "style",
        from: "f.restyle",
        to: &["f.boxes"],
        open: false,
    },
    // The box tree, layout, fragments, observers and the hit test.
    Boundary {
        name: "layout",
        from: "f.boxes",
        to: &["f.paint"],
        open: false,
    },
    // Damage expansion, the emit walk and the upload of the paint content.
    Boundary {
        name: "paint",
        from: "f.paint",
        to: &["draw.in"],
        open: false,
    },
    // The renderer records the frame's command buffers.
    Boundary {
        name: "record",
        from: "draw.in",
        to: &["sub.held", "acq.in"],
        open: false,
    },
    // A composed frame waits for the frame callback that lets it present.
    Boundary {
        name: "held",
        from: "sub.held",
        to: &["acq.in"],
        open: false,
    },
    // The swap chain hands out the surface texture.
    Boundary {
        name: "acquire",
        from: "acq.in",
        to: &["acq.out"],
        open: false,
    },
    // The copy to the surface texture and the queue submission. The present of a held frame
    // submits without a mark, so that phase ends at the present.
    Boundary {
        name: "submit",
        from: "acq.out",
        to: &["sub.out"],
        open: true,
    },
];

/// The mark that starts a frame.
const FRAME_BEGIN: &str = "f.begin";

/// The span that covers the frame up to its present.
const FRAME: &str = "frame";

/// Reads the phases of the frame whose redraw began at `began` and that presents at `now`.
///
/// `marks` is in recording order. Marks before `began` or after `now` belong to other frames. The
/// result holds a `frame` span from the frame begin to `now`, then each phase whose marks are
/// present.
pub(crate) fn extract(
    marks: &[(&'static str, Instant)],
    began: Instant,
    now: Instant,
) -> Vec<Phase> {
    let window: Vec<(&str, Instant)> = marks
        .iter()
        .copied()
        .filter(|&(_, at)| at >= began && at <= now)
        .collect();
    let Some(start) = window.iter().position(|&(stage, _)| stage == FRAME_BEGIN) else {
        return Vec::new();
    };
    let mut phases = vec![Phase {
        name: FRAME,
        begin: window[start].1,
        end: now,
    }];
    for boundary in BOUNDARIES {
        let Some(from) = find(&window, start, |stage| stage == boundary.from) else {
            continue;
        };
        let end = match find(&window, from + 1, |stage| boundary.to.contains(&stage)) {
            Some(to) => window[to].1,
            None if boundary.open => now,
            None => continue,
        };
        phases.push(Phase {
            name: boundary.name,
            begin: window[from].1,
            end,
        });
    }
    phases
}

/// The index of the first mark at or after `from` whose stage matches.
fn find(window: &[(&str, Instant)], from: usize, matches: impl Fn(&str) -> bool) -> Option<usize> {
    window
        .get(from..)?
        .iter()
        .position(|&(stage, _)| matches(stage))
        .map(|index| index + from)
}

#[cfg(test)]
mod tests {
    use super::{Phase, extract};
    use std::time::{Duration, Instant};

    /// Marks at the given millisecond offsets from `base`.
    fn marks(base: Instant, list: &[(&'static str, u64)]) -> Vec<(&'static str, Instant)> {
        list.iter()
            .map(|&(stage, ms)| (stage, base + Duration::from_millis(ms)))
            .collect()
    }

    fn names(phases: &[Phase]) -> Vec<&'static str> {
        phases.iter().map(|phase| phase.name).collect()
    }

    fn span(phases: &[Phase], name: &str, base: Instant) -> (u64, u64) {
        let phase = phases
            .iter()
            .find(|phase| phase.name == name)
            .expect("the phase exists");
        (
            phase.begin.duration_since(base).as_millis() as u64,
            phase.end.duration_since(base).as_millis() as u64,
        )
    }

    #[test]
    fn a_presented_frame_has_every_phase_in_order() {
        let base = Instant::now();
        let list = marks(
            base,
            &[
                ("f.begin", 1),
                ("f.drain", 2),
                ("f.timers", 3),
                ("f.flush", 4),
                ("f.restyle", 5),
                ("f.boxes", 7),
                ("f.layout", 8),
                ("f.paint", 10),
                ("draw.in", 12),
                ("r.record", 13),
                ("acq.in", 15),
                ("acq.out", 16),
                ("sub.out", 18),
            ],
        );
        let phases = extract(&list, base, base + Duration::from_millis(19));
        assert_eq!(
            names(&phases),
            [
                "frame", "events", "update", "style", "layout", "paint", "record", "acquire",
                "submit"
            ]
        );
        assert_eq!(span(&phases, "frame", base), (1, 19));
        assert_eq!(span(&phases, "events", base), (2, 3));
        assert_eq!(span(&phases, "layout", base), (7, 10));
        assert_eq!(span(&phases, "record", base), (12, 15));
        assert_eq!(span(&phases, "submit", base), (16, 18));
    }

    #[test]
    fn a_held_frame_waits_between_its_record_and_its_acquire() {
        let base = Instant::now();
        let list = marks(
            base,
            &[
                ("f.begin", 1),
                ("f.paint", 2),
                ("draw.in", 3),
                ("sub.held", 5),
                ("f.end", 6),
                ("acq.in", 20),
                ("acq.out", 21),
                ("sub.out", 22),
            ],
        );
        let phases = extract(&list, base, base + Duration::from_millis(23));
        assert_eq!(span(&phases, "record", base), (3, 5));
        assert_eq!(span(&phases, "held", base), (5, 20));
        assert_eq!(span(&phases, "acquire", base), (20, 21));
        assert_eq!(span(&phases, "submit", base), (21, 22));
    }

    #[test]
    fn a_held_present_submits_until_the_present() {
        let base = Instant::now();
        let list = marks(
            base,
            &[
                ("f.begin", 1),
                ("draw.in", 2),
                ("sub.out", 3),
                ("sub.held", 4),
                ("acq.in", 20),
                ("acq.out", 21),
            ],
        );
        let phases = extract(&list, base, base + Duration::from_millis(23));
        assert_eq!(span(&phases, "record", base), (2, 4));
        assert_eq!(span(&phases, "held", base), (4, 20));
        assert_eq!(span(&phases, "submit", base), (21, 23));
    }

    #[test]
    fn marks_of_earlier_frames_are_ignored() {
        let base = Instant::now();
        let list = marks(
            base,
            &[
                ("f.begin", 1),
                ("f.drain", 2),
                ("f.timers", 3),
                ("f.begin", 10),
                ("f.drain", 11),
                ("f.timers", 14),
            ],
        );
        let phases = extract(
            &list,
            base + Duration::from_millis(9),
            base + Duration::from_millis(15),
        );
        assert_eq!(span(&phases, "frame", base), (10, 15));
        assert_eq!(span(&phases, "events", base), (11, 14));
    }

    #[test]
    fn a_window_without_a_frame_begin_has_no_phases() {
        let base = Instant::now();
        let list = marks(base, &[("acq.in", 1), ("acq.out", 2)]);
        assert!(extract(&list, base, base + Duration::from_millis(3)).is_empty());
    }

    #[test]
    fn a_phase_without_its_end_mark_is_left_out() {
        let base = Instant::now();
        let list = marks(base, &[("f.begin", 1), ("f.drain", 2), ("acq.in", 3)]);
        let phases = extract(&list, base, base + Duration::from_millis(4));
        assert_eq!(names(&phases), ["frame"]);
    }

    #[test]
    fn marks_after_the_present_are_ignored() {
        let base = Instant::now();
        let list = marks(base, &[("f.begin", 1), ("acq.in", 2), ("acq.out", 9)]);
        let phases = extract(&list, base, base + Duration::from_millis(5));
        assert_eq!(names(&phases), ["frame"]);
    }
}
