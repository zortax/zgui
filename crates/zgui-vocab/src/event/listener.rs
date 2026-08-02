//! How a listener is registered, and which part of a dispatch it sees.

/// Which leg of a dispatch is in progress.
///
/// An event is delivered in three legs: down from the root to the target, at the target itself,
/// and back up to the root. The first leg exists so that an ancestor can see an event *before* the
/// element it was aimed at — which is how a dismissable overlay learns about a press outside
/// itself without the element that was pressed cooperating.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// Travelling down from the root towards the target.
    Capture,
    /// At the element the event was aimed at.
    #[default]
    Target,
    /// Travelling back up from the target towards the root.
    Bubble,
}

/// How a listener was registered.
///
/// ```
/// use zgui_vocab::{ListenerOptions, Phase};
///
/// let capture = ListenerOptions::CAPTURE;
/// assert!(capture.runs_in(Phase::Capture));
/// assert!(capture.runs_in(Phase::Target));
/// assert!(!capture.runs_in(Phase::Bubble));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ListenerOptions {
    /// Whether the listener runs on the way down rather than on the way up.
    pub capture: bool,
    /// Whether the listener promises never to suppress the event's default behaviour.
    ///
    /// A promise, not a hint: it lets the framework act on the event before running the handler,
    /// which is what keeps scrolling smooth while a listener is attached to it. Breaking the
    /// promise does not crash, it simply has no effect.
    pub passive: bool,
    /// Whether the listener removes itself after running once.
    pub once: bool,
}

impl ListenerOptions {
    /// The ordinary registration: on the way up, able to cancel, and lasting.
    pub const DEFAULT: Self = Self {
        capture: false,
        passive: false,
        once: false,
    };

    /// A registration on the way down.
    pub const CAPTURE: Self = Self {
        capture: true,
        ..Self::DEFAULT
    };

    /// A registration that promises not to cancel.
    pub const PASSIVE: Self = Self {
        passive: true,
        ..Self::DEFAULT
    };

    /// A registration that lasts for one event.
    pub const ONCE: Self = Self {
        once: true,
        ..Self::DEFAULT
    };

    /// Whether a listener registered this way runs during `phase`.
    ///
    /// A listener on the element the event was aimed at runs whichever way it was registered,
    /// because at the target there is no up or down to distinguish.
    pub const fn runs_in(self, phase: Phase) -> bool {
        match phase {
            Phase::Capture => self.capture,
            Phase::Target => true,
            Phase::Bubble => !self.capture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ListenerOptions, Phase};

    #[test]
    fn a_bubbling_listener_skips_the_way_down() {
        let options = ListenerOptions::DEFAULT;
        assert!(!options.runs_in(Phase::Capture));
        assert!(options.runs_in(Phase::Target));
        assert!(options.runs_in(Phase::Bubble));
    }

    #[test]
    fn a_capturing_listener_skips_the_way_up() {
        let options = ListenerOptions::CAPTURE;
        assert!(options.runs_in(Phase::Capture));
        assert!(!options.runs_in(Phase::Bubble));
    }

    #[test]
    fn every_registration_runs_at_the_target() {
        for options in [
            ListenerOptions::DEFAULT,
            ListenerOptions::CAPTURE,
            ListenerOptions::PASSIVE,
            ListenerOptions::ONCE,
        ] {
            assert!(options.runs_in(Phase::Target));
        }
    }

    #[test]
    fn the_shorthands_change_exactly_one_thing_each() {
        assert_eq!(ListenerOptions::default(), ListenerOptions::DEFAULT);
        for (shorthand, name) in [
            (ListenerOptions::CAPTURE, "capture"),
            (ListenerOptions::PASSIVE, "passive"),
            (ListenerOptions::ONCE, "once"),
        ] {
            let changed = [
                (shorthand.capture, "capture"),
                (shorthand.passive, "passive"),
                (shorthand.once, "once"),
            ];
            let set: Vec<&str> = changed
                .iter()
                .filter(|(on, _)| *on)
                .map(|(_, field)| *field)
                .collect();
            assert_eq!(set, vec![name]);
        }
    }
}
