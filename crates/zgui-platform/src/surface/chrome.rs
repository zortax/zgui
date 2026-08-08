//! The surface's own furniture: cursors, edges, full screen.

/// What the pointer should look like over a surface.
///
/// The set is the one every desktop agrees on. A cursor a platform does not have falls back to
/// [`CursorStyle::Default`] rather than failing, because a missing cursor is a cosmetic problem
/// and an error return here would be checked by nobody.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CursorStyle {
    /// The ordinary arrow.
    #[default]
    Default,
    /// The pointing hand, over something that activates.
    Pointer,
    /// The text bar, over something that can be selected or typed into.
    Text,
    /// The vertical text bar, over vertical text.
    VerticalText,
    /// The crosshair, over something being aimed at.
    Crosshair,
    /// The open hand, over something that can be grabbed.
    Grab,
    /// The closed hand, over something being dragged.
    Grabbing,
    /// The busy indicator, over something that is not accepting input.
    Wait,
    /// The busy indicator that still accepts input.
    Progress,
    /// The no-entry sign, over a drop target that will refuse.
    NotAllowed,
    /// The four-way arrow, over something being moved.
    Move,
    /// The horizontal resize arrow.
    ResizeColumn,
    /// The vertical resize arrow.
    ResizeRow,
    /// The east-west resize arrow.
    ResizeEastWest,
    /// The north-south resize arrow.
    ResizeNorthSouth,
    /// The north-east-south-west resize arrow.
    ResizeNorthEastSouthWest,
    /// The north-west-south-east resize arrow.
    ResizeNorthWestSouthEast,
    /// No cursor at all.
    None,
}

/// Which edge or corner a resize drag was started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResizeEdge {
    /// The top edge.
    North,
    /// The bottom edge.
    South,
    /// The right edge.
    East,
    /// The left edge.
    West,
    /// The top-right corner.
    NorthEast,
    /// The top-left corner.
    NorthWest,
    /// The bottom-right corner.
    SouthEast,
    /// The bottom-left corner.
    SouthWest,
}

/// How a surface should fill the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FullscreenMode {
    /// Fill the screen without changing its mode, keeping other windows composited beneath.
    Borderless,
    /// Take the screen exclusively, which may change its mode.
    Exclusive,
}

/// Where a surface sits in the desktop's stacking order.
///
/// A preference, not a guarantee. A desktop that does not let an application place itself in the
/// stack keeps every window [`WindowLevel::Normal`], and asking for another level there changes
/// nothing rather than failing — which is what lets a palette window ask to float without the
/// application growing a branch per desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WindowLevel {
    /// Below ordinary windows, where a desktop widget sits.
    AlwaysOnBottom,
    /// With the other windows.
    #[default]
    Normal,
    /// Above ordinary windows, where a palette or a picture-in-picture sits.
    AlwaysOnTop,
}

/// What frame a surface asks the platform for.
///
/// A platform carries out as much of the request as it can. [`Decorations::NoTitleBar`] reaches
/// macOS in full, where the frame stays and the title bar goes. The other platforms draw the whole
/// frame or none of it, and they read that request as [`Decorations::None`].
///
/// The application owes the user the affordances it turned off: something to drag the window by,
/// edges to resize from, and a way to close it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Decorations {
    /// The platform draws the title bar and the frame.
    #[default]
    Full,
    /// The platform draws nothing, and the application draws its own frame.
    None,
    /// The platform draws the frame, and leaves the title bar out.
    ///
    /// The window keeps its corners, its shadow and its resize edges. macOS also keeps the window
    /// buttons, which sit over the top left of the content.
    NoTitleBar,
}

impl Decorations {
    /// Whether the platform draws a frame.
    pub const fn is_platform_drawn(self) -> bool {
        matches!(self, Self::Full | Self::NoTitleBar)
    }

    /// Whether the application draws its own title bar.
    pub const fn needs_own_title_bar(self) -> bool {
        matches!(self, Self::None | Self::NoTitleBar)
    }
}

/// Who draws the title bar and the frame.
///
/// This is a capability rather than a preference: on a desktop that leaves it to the application,
/// an application that does not draw one has no way to be moved or closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DecorationSource {
    /// The platform draws it.
    #[default]
    Platform,
    /// The application must draw it, and must provide its own move and resize affordances.
    Application,
}

impl DecorationSource {
    /// Whether the application has to draw its own frame.
    pub const fn is_application(self) -> bool {
        matches!(self, Self::Application)
    }
}

#[cfg(test)]
mod tests {
    use super::{CursorStyle, DecorationSource, Decorations};

    #[test]
    fn the_ordinary_arrow_is_the_default_cursor() {
        assert_eq!(CursorStyle::default(), CursorStyle::Default);
    }

    #[test]
    fn decorations_default_to_the_platform_drawing_them() {
        assert!(!DecorationSource::default().is_application());
        assert!(DecorationSource::Application.is_application());
    }

    #[test]
    fn a_surface_asks_for_a_full_frame_by_default() {
        assert_eq!(Decorations::default(), Decorations::Full);
        assert!(Decorations::default().is_platform_drawn());
    }

    #[test]
    fn a_frame_without_a_title_bar_is_still_the_platforms() {
        // The corners, the shadow and the resize edges all come with it.
        assert!(Decorations::NoTitleBar.is_platform_drawn());
        assert!(!Decorations::None.is_platform_drawn());
    }

    #[test]
    fn both_frames_that_drop_the_title_bar_need_one_drawn() {
        assert!(Decorations::None.needs_own_title_bar());
        assert!(Decorations::NoTitleBar.needs_own_title_bar());
        assert!(!Decorations::Full.needs_own_title_bar());
    }
}
