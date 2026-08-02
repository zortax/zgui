//! Runtime checks for the two ways this crate can be miscompiled into silence.
//!
//! Both failures have the same signature: the framework builds, the window opens, input is
//! accepted, and nothing ever updates. Neither produces a compiler error, a panic or a log line,
//! and both are caused by a cargo feature resolved somewhere else in the dependency graph. So
//! both are checked here, mechanically, rather than trusted.

use crate::executor::{flush, install};
use crate::own::Mounted;
use crate::reexport::RenderEffect;
use crate::reexport::RwSignal;
use reactive_graph::traits::{Get, GetUntracked, Set};

/// Whether reactive effects actually run.
///
/// Builds a real effect, writes a signal it reads, flushes, and reports whether it re-ran. False
/// means effects were compiled out: every view builds correctly, renders once, and then ignores
/// every change to every signal for the rest of the process's life.
///
/// Cheap enough to assert at startup, and worth it — the alternative diagnosis is bisecting a
/// dependency graph while looking at a window that will not repaint.
///
/// ```
/// assert!(zgui_reactive::effects_are_enabled());
/// ```
#[must_use]
pub fn effects_are_enabled() -> bool {
    if install().is_err() {
        return false;
    }

    let scope = Mounted::new();
    let re_ran = scope.with(|| {
        let source = RwSignal::new(0);
        let observed = RwSignal::new(0);
        let effect = RenderEffect::new(move |_| observed.set(source.get()));

        source.set(1);
        flush();

        let re_ran = observed.get_untracked() == 1;
        drop(effect);
        re_ran
    });
    scope.unmount();
    re_ran
}

#[cfg(test)]
mod tests {
    use reactive_graph::IntoReactiveValue;
    use reactive_graph::traits::Get;

    use super::*;
    use crate::reexport::Signal;
    use crate::zone::enter_non_reactive_zone;

    #[test]
    fn effects_are_enabled_canary() {
        assert!(
            effects_are_enabled(),
            "reactive effects are compiled out: the whole UI would build, run and never update"
        );
    }

    /// The nightly-feature canary.
    ///
    /// The conversions that let a component property accept a closure are compiled only when the
    /// reactive engine's `nightly` feature is off. We build on nightly, so one transitive crate
    /// enabling that feature deletes every one of them at once — and the failure is a wall of
    /// trait-resolution errors in generated macro code, a long way from its cause. Constructing
    /// one here fails first, and cheaply.
    #[test]
    fn a_closure_still_converts_into_a_property() {
        install().unwrap();
        let scope = Mounted::new();
        let value: Signal<i32> = (|| 1).into_reactive_value();
        let _zone = enter_non_reactive_zone();
        assert_eq!(value.get(), 1);
        scope.unmount();
    }
}
