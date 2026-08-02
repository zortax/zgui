//! What a checkbox can be.

use zgui::vocab::Toggled;

/// The three positions a checkbox can be in.
///
/// Three rather than two, because "some of the things below this are ticked" is a real answer and
/// a boolean cannot hold it — and because a parent checkbox that showed itself as unticked while
/// half its children were ticked would be lying to everyone who cannot see the children.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Checked {
    /// Not ticked.
    #[default]
    No,
    /// Ticked.
    Yes,
    /// Neither: some of what this stands for is ticked and some is not.
    Mixed,
}

impl Checked {
    /// What pressing it does.
    ///
    /// A mixed checkbox becomes ticked, because the useful thing to do with "some of these" is to
    /// mean "all of these"; from there it goes to unticked and back, and never returns to mixed —
    /// only the thing it stands for can put it there.
    ///
    /// ```
    /// use zgui_ui::Checked;
    ///
    /// assert_eq!(Checked::No.toggled(), Checked::Yes);
    /// assert_eq!(Checked::Yes.toggled(), Checked::No);
    /// assert_eq!(Checked::Mixed.toggled(), Checked::Yes);
    /// ```
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::No | Self::Mixed => Self::Yes,
        }
    }

    /// Whether it is ticked, which mixed is not.
    #[must_use]
    pub const fn is_checked(self) -> bool {
        matches!(self, Self::Yes)
    }

    /// Whether it is in its mixed position.
    #[must_use]
    pub const fn is_mixed(self) -> bool {
        matches!(self, Self::Mixed)
    }

    /// What a reader is told, which is the three positions and not two.
    #[must_use]
    pub const fn toggled_state(self) -> Toggled {
        match self {
            Self::No => Toggled::False,
            Self::Yes => Toggled::True,
            Self::Mixed => Toggled::Mixed,
        }
    }

    /// What the `data-state` attribute says, which is what a style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::No => "unchecked",
            Self::Yes => "checked",
            Self::Mixed => "indeterminate",
        }
    }
}

impl From<bool> for Checked {
    fn from(checked: bool) -> Self {
        if checked { Self::Yes } else { Self::No }
    }
}
