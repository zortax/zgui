//! What a handler can say about an event it has just seen.

/// How far an event should keep travelling after the current handler returns.
///
/// The two ways of stopping are genuinely different and confusing them produces bugs that only
/// appear when a second listener is added years later. [`Propagation::Stop`] finishes the
/// listeners already attached to the current element and then goes no further, so a control's own
/// two handlers both run. [`Propagation::StopImmediate`] abandons the rest of them as well, which
/// is what a handler that has entirely replaced the element's behaviour wants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Propagation {
    /// Carry on to the next element.
    #[default]
    Continue,
    /// Finish this element's listeners, then stop.
    Stop,
    /// Stop at once, without running this element's remaining listeners.
    StopImmediate,
}

impl Propagation {
    /// Whether the event should travel to the next element.
    pub const fn continues_to_next_element(self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Whether this element's remaining listeners should still run.
    pub const fn continues_to_next_listener(self) -> bool {
        matches!(self, Self::Continue | Self::Stop)
    }

    /// The stronger of two requests, so that combining them never weakens either.
    ///
    /// Dispatch folds each handler's answer into the running one with this, which is why a
    /// handler that asks to stop cannot be undone by a later one that does not.
    ///
    /// ```
    /// use zgui_vocab::Propagation;
    ///
    /// let running = Propagation::Stop.strongest(Propagation::Continue);
    /// assert_eq!(running, Propagation::Stop);
    /// ```
    pub const fn strongest(self, other: Self) -> Self {
        match (self, other) {
            (Self::StopImmediate, _) | (_, Self::StopImmediate) => Self::StopImmediate,
            (Self::Stop, _) | (_, Self::Stop) => Self::Stop,
            _ => Self::Continue,
        }
    }
}

/// Whether the framework's own behaviour for an event should still happen.
///
/// This is a separate answer from [`Propagation`] because the two questions are independent: a
/// handler can suppress a key's default behaviour while letting the event carry on to ancestors,
/// and it can stop the event travelling while leaving the default alone. Collapsing them into one
/// value is a mistake that only shows up in the cases where they differ.
///
/// An event whose kind is not cancelable ignores a request to prevent its default, because there
/// is nothing left to prevent by the time it is reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DefaultAction {
    /// The framework's behaviour runs as usual.
    #[default]
    Allowed,
    /// A handler has taken responsibility and the framework's behaviour is skipped.
    Prevented,
}

impl DefaultAction {
    /// Whether the framework's behaviour should run.
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// The stronger of two answers: once prevented, always prevented.
    pub const fn strongest(self, other: Self) -> Self {
        match (self, other) {
            (Self::Prevented, _) | (_, Self::Prevented) => Self::Prevented,
            _ => Self::Allowed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DefaultAction, Propagation};

    #[test]
    fn stopping_and_stopping_immediately_differ_in_exactly_one_way() {
        assert!(!Propagation::Stop.continues_to_next_element());
        assert!(Propagation::Stop.continues_to_next_listener());
        assert!(!Propagation::StopImmediate.continues_to_next_listener());
    }

    #[test]
    fn a_later_handler_cannot_undo_an_earlier_stop() {
        let running = Propagation::default()
            .strongest(Propagation::Stop)
            .strongest(Propagation::Continue);
        assert_eq!(running, Propagation::Stop);

        let escalated = running.strongest(Propagation::StopImmediate);
        assert_eq!(escalated, Propagation::StopImmediate);
        assert_eq!(
            escalated.strongest(Propagation::Continue),
            Propagation::StopImmediate
        );
    }

    #[test]
    fn preventing_the_default_is_likewise_one_way() {
        assert!(DefaultAction::default().is_allowed());
        let prevented = DefaultAction::Allowed.strongest(DefaultAction::Prevented);
        assert!(!prevented.is_allowed());
        assert!(!prevented.strongest(DefaultAction::Allowed).is_allowed());
    }
}
