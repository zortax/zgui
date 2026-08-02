//! How one CSS longhand is treated, and why.

/// How this framework treats one CSS longhand.
///
/// One of these is declared beside the code that reads the property, never in a central table:
/// a table maintained by hand drifts the first time a reader moves, whereas a declaration that
/// travels with its reader cannot.
///
/// ```
/// use zgui_css::parity::{AbsentReason, Support};
///
/// let painted = Support::Implemented("zgui-paint::lower::border");
/// let parsed_but_dropped = Support::Ignored("border images are not painted yet");
/// let unavailable = Support::Absent(AbsentReason::GeckoOnly);
///
/// assert!(painted.is_consumed());
/// assert!(!parsed_but_dropped.is_consumed());
/// assert!(!unavailable.is_reachable());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Support {
    /// Parsed, cascaded and consumed. The string names the consuming module.
    Implemented(&'static str),
    /// Parsed and cascaded, deliberately ignored. The string says why.
    Ignored(&'static str),
    /// Not available from the style engine at all.
    Absent(AbsentReason),
}

impl Support {
    /// Whether some code actually reads the property's value.
    ///
    /// This is the number "CSS parity" is a claim about. The other two arms are both forms of
    /// *nothing happens*, and telling them apart matters for who fixes them, not for an author
    /// whose declaration had no effect.
    pub const fn is_consumed(self) -> bool {
        matches!(self, Self::Implemented(_))
    }

    /// Whether the property can be written in a style sheet and read back off a computed style.
    ///
    /// [`Support::Ignored`] is reachable and unread; [`Support::Absent`] is not reachable at all,
    /// which is why the two cannot be collapsed into one "unsupported" answer.
    pub const fn is_reachable(self) -> bool {
        !matches!(self, Self::Absent(_))
    }

    /// How strong an answer this is, for merging two crates' declarations about one property.
    ///
    /// *Consumed* beats *ignored* beats *unavailable*, because each is a statement about what some
    /// module does, and the strongest true statement is the true one: a property one crate only
    /// hashes and another reads is a property that is read.
    ///
    /// ```
    /// use zgui_css::parity::{AbsentReason, Support};
    ///
    /// assert!(
    ///     Support::Implemented("zgui-paint").strength()
    ///         > Support::Ignored("nothing reads it").strength()
    /// );
    /// assert!(
    ///     Support::Ignored("nothing reads it").strength()
    ///         > Support::Absent(AbsentReason::GeckoOnly).strength()
    /// );
    /// ```
    pub const fn strength(self) -> u8 {
        match self {
            Self::Implemented(_) => 2,
            Self::Ignored(_) => 1,
            Self::Absent(_) => 0,
        }
    }

    /// The prose beside the declaration: the consuming module, or the reason there is none.
    pub const fn note(self) -> &'static str {
        match self {
            Self::Implemented(note) | Self::Ignored(note) => note,
            Self::Absent(reason) => reason.note(),
        }
    }
}

/// Why a longhand is not available from the style engine.
///
/// The distinction is not editorial: each variant implies a different fix, a different owner and a
/// different cost, and two of them imply that the property is not even a name the engine's parser
/// recognises — so a declaration written against one of those is discarded silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AbsentReason {
    /// Generated, but switched off in the engine's current configuration.
    ///
    /// The property parses and cascades the moment its preference is on, so the fix is a flip and
    /// then a reader.
    PrefOff,
    /// Present in the engine's sources but built only for a different engine.
    ///
    /// No preference reaches it: the property is simply not generated, so its name is unknown to
    /// the parser and every declaration using it is dropped.
    GeckoOnly,
    /// The engine has no definition of the property at all, for any target.
    NotInStylo,
    /// Reachable only by patching the engine, because a code path is switched off at compile time
    /// rather than by a preference.
    NeedsFork,
    /// Available from the engine and deliberately not expressible by this framework's layout.
    NotInLayout,
}

impl AbsentReason {
    /// A short standing explanation, used where a declaration carries no prose of its own.
    pub const fn note(self) -> &'static str {
        match self {
            Self::PrefOff => "generated, but switched off in the engine's configuration",
            Self::GeckoOnly => {
                "built only for another engine, so the name is unknown to the parser"
            }
            Self::NotInStylo => "the engine has no definition of this property",
            Self::NeedsFork => "reachable only by patching the engine",
            Self::NotInLayout => "cascades, but this framework's layout cannot express it",
        }
    }

    /// What the engine must say about the property for this reason to be the true one.
    ///
    /// This is what turns a declaration into a claim that can be wrong. A property registered
    /// [`AbsentReason::GeckoOnly`] that the parser happily accepts is a stale declaration, and
    /// [`crate::parity::Registration::check`] is where that is caught.
    pub const fn expected(self) -> Expectation {
        match self {
            Self::PrefOff => Expectation::KnownButDisabled,
            Self::GeckoOnly | Self::NotInStylo | Self::NeedsFork => Expectation::Unknown,
            Self::NotInLayout => Expectation::Enabled,
        }
    }
}

/// What a declaration implies the engine will say about a property.
///
/// Every declaration is a claim of one of these three shapes, and the engine can be asked directly
/// which one holds — so a declaration that has gone stale is a test failure rather than a comment
/// nobody re-read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Expectation {
    /// The parser accepts the name and the property is live in the current configuration.
    Enabled,
    /// The parser knows the name, but the property is switched off, so declarations are discarded.
    KnownButDisabled,
    /// The parser does not know the name at all.
    Unknown,
}
