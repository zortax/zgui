//! Where a stylesheet sits in the cascade, and the set of those a change disturbed.

use style::stylesheets::{Origin, OriginSet};

/// Which of the three cascade origins a stylesheet belongs to.
///
/// The order is the cascade's own: for declarations of equal specificity a later origin wins, and
/// for `!important` declarations the order reverses, so a user-agent `!important` rule beats an
/// author one. Nothing here decides that — the engine does — but the ordering is the reason the
/// distinction exists at all.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum SheetOrigin {
    /// This framework's own sheet: the element vocabulary's display defaults, box sizing, the
    /// focus ring, selection colours and scrollbar metrics.
    UserAgent,
    /// Overrides supplied by whoever is running the application, including end-user themes.
    User,
    /// The component library's sheets and then the application's, in registration order.
    Author,
}

impl SheetOrigin {
    /// The engine's spelling of this origin.
    pub(crate) fn to_engine(self) -> Origin {
        match self {
            Self::UserAgent => Origin::UserAgent,
            Self::User => Origin::User,
            Self::Author => Origin::Author,
        }
    }
}

/// A set of cascade origins, as returned by a change that may have disturbed some of them.
///
/// Empty is the interesting value: a resize that crosses no media-query boundary disturbs nothing,
/// and the whole point of asking is to be able to do nothing in that case.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct OriginMask {
    /// The user-agent origin.
    user_agent: bool,
    /// The user origin.
    user: bool,
    /// The author origin.
    author: bool,
}

impl OriginMask {
    /// The empty set.
    pub const EMPTY: Self = Self {
        user_agent: false,
        user: false,
        author: false,
    };

    /// Whether no origin is in the set.
    pub fn is_empty(self) -> bool {
        self == Self::EMPTY
    }

    /// Whether `origin` is in the set.
    pub fn contains(self, origin: SheetOrigin) -> bool {
        match origin {
            SheetOrigin::UserAgent => self.user_agent,
            SheetOrigin::User => self.user,
            SheetOrigin::Author => self.author,
        }
    }

    /// The engine's answer in this framework's shape.
    pub(crate) fn from_engine(origins: OriginSet) -> Self {
        Self {
            user_agent: origins.contains(OriginSet::ORIGIN_USER_AGENT),
            user: origins.contains(OriginSet::ORIGIN_USER),
            author: origins.contains(OriginSet::ORIGIN_AUTHOR),
        }
    }
}

#[cfg(test)]
mod tests {
    use style::stylesheets::{Origin, OriginSet};

    use super::{OriginMask, SheetOrigin};

    #[test]
    fn the_three_origins_survive_the_round_trip_the_engine_makes_of_them() {
        for (ours, theirs) in [
            (SheetOrigin::UserAgent, Origin::UserAgent),
            (SheetOrigin::User, Origin::User),
            (SheetOrigin::Author, Origin::Author),
        ] {
            assert_eq!(ours.to_engine(), theirs);
            let mask = OriginMask::from_engine(OriginSet::from(theirs));
            assert!(mask.contains(ours));
            assert!(!mask.is_empty());
        }
    }

    #[test]
    fn nothing_disturbed_is_the_empty_set() {
        assert!(OriginMask::from_engine(OriginSet::empty()).is_empty());
        assert!(OriginMask::EMPTY.is_empty());
    }
}
