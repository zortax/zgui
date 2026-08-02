//! Making a surface answer badly on purpose.

use std::env;

use crate::target::acquire::Acquisition;

/// The variable that primes the injector from the environment.
const VARIABLE: &str = "ZGUI_SURFACE_FAULT";

/// Answers to give instead of asking the surface.
///
/// Six of the seven answers a surface can give are ordinary events in a window's life — the
/// compositor is busy, the window was minimised, it was resized underneath a frame — and every one
/// of them is rare enough that nothing exercises it by accident. That is exactly the shape of a
/// path that stays wrong for months: the code is written, it is never taken, and the first time it
/// is taken the window stops painting.
///
/// So the answers can be asked for. `ZGUI_SURFACE_FAULT=<answer>[,n]` primes the next `n`
/// acquisitions from the environment, and [`FaultInjector::inject`] does the same from a test —
/// the same mechanism either way, so what a test drives is what a run with the variable set does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaultInjector {
    /// What to answer, and how many more times.
    queued: Option<(Acquisition, u32)>,
}

impl FaultInjector {
    /// The injector the environment asks for.
    ///
    /// Absent, malformed, or naming an answer that does not exist all mean the same thing: the
    /// surface answers for itself.
    pub fn from_environment() -> Self {
        Self {
            queued: parse(env::var(VARIABLE).ok().as_deref()),
        }
    }

    /// Answers the next `times` acquisitions with `answer`, replacing anything already queued.
    pub fn inject(&mut self, answer: Acquisition, times: u32) {
        self.queued = (times > 0).then_some((answer, times));
    }

    /// The answer to give this time, if there is one queued.
    pub fn take(&mut self) -> Option<Acquisition> {
        let (answer, remaining) = self.queued?;
        self.queued = remaining
            .checked_sub(1)
            .and_then(|left| (left > 0).then_some((answer, left)));
        Some(answer)
    }

    /// How many injected answers are left.
    pub fn remaining(&self) -> u32 {
        self.queued.map_or(0, |(_, remaining)| remaining)
    }
}

/// The answer and repeat count a setting names.
fn parse(setting: Option<&str>) -> Option<(Acquisition, u32)> {
    let setting = setting?;
    let (name, count) = match setting.split_once(',') {
        Some((name, count)) => (name, count.trim().parse().ok()?),
        None => (setting, 1),
    };
    (count > 0).then_some((named(name.trim())?, count))
}

/// The answer a name refers to.
fn named(name: &str) -> Option<Acquisition> {
    Acquisition::ALL
        .into_iter()
        .find(|arm| arm.name().eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::{FaultInjector, named, parse};
    use crate::target::acquire::Acquisition;

    #[test]
    fn every_answer_a_surface_can_give_can_be_asked_for_by_name() {
        for arm in Acquisition::ALL {
            assert_eq!(named(arm.name()), Some(arm), "{arm:?} cannot be injected");
        }
    }

    #[test]
    fn a_count_makes_the_answer_repeat_and_its_absence_means_once() {
        assert_eq!(parse(Some("outdated")), Some((Acquisition::Outdated, 1)));
        assert_eq!(parse(Some("Outdated,4")), Some((Acquisition::Outdated, 4)));
        assert_eq!(parse(Some(" lost , 2 ")), Some((Acquisition::Lost, 2)));
    }

    #[test]
    fn a_setting_that_names_nothing_leaves_the_surface_to_answer_for_itself() {
        assert_eq!(parse(None), None);
        assert_eq!(parse(Some("")), None);
        assert_eq!(parse(Some("sideways")), None);
        assert_eq!(parse(Some("timeout,not-a-number")), None);
        assert_eq!(parse(Some("timeout,0")), None);
    }

    #[test]
    fn an_injection_runs_out_and_then_the_surface_answers_again() {
        let mut injector = FaultInjector::default();
        assert_eq!(injector.take(), None);
        injector.inject(Acquisition::Timeout, 2);
        assert_eq!(injector.remaining(), 2);
        assert_eq!(injector.take(), Some(Acquisition::Timeout));
        assert_eq!(injector.take(), Some(Acquisition::Timeout));
        assert_eq!(injector.take(), None, "an exhausted injection is over");
        assert_eq!(injector.remaining(), 0);
    }
}
