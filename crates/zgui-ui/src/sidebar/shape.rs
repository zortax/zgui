//! Which edge a sidebar is on, what folding it leaves behind, and what frame it sits in.

/// Which side of the window a sidebar is on.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarSide {
    /// The leading edge.
    #[default]
    Left,
    /// The trailing edge.
    Right,
}

impl SidebarSide {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// Whether a folded sidebar leaves a rail behind or disappears.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarCollapse {
    /// Fold to a narrow rail of icons.
    #[default]
    Icon,
    /// Fold away to nothing.
    Offcanvas,
    /// Never fold at all.
    None,
}

impl SidebarCollapse {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::Offcanvas => "offcanvas",
            Self::None => "none",
        }
    }
}

/// What frame a sidebar's surface sits in.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarVariant {
    /// Flush against the window's edge, ruled off from the page.
    #[default]
    Sidebar,
    /// A rounded card inset from the window's edge, with its own border and lift.
    Floating,
    /// Flush itself, with the *page* becoming the rounded card that floats on it.
    Inset,
}

impl SidebarVariant {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Floating => "floating",
            Self::Inset => "inset",
        }
    }

    /// Whether the surface is held off the window's edge by the frame's own padding.
    #[must_use]
    pub const fn is_padded(self) -> bool {
        matches!(self, Self::Floating | Self::Inset)
    }
}

/// How tall an entry in a sidebar's menu is.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarMenuSize {
    /// The ordinary entry.
    #[default]
    Default,
    /// A shorter entry, in smaller type.
    Sm,
    /// A tall entry, for one carrying two lines or a picture.
    Lg,
}

impl SidebarMenuSize {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
        }
    }
}

/// How an entry in a sidebar's menu is drawn.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarMenuVariant {
    /// Nothing until it is hovered or current.
    #[default]
    Default,
    /// A ruled box on the page's own surface.
    Outline,
}

impl SidebarMenuVariant {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
        }
    }
}

/// How tall an entry in a nested sidebar menu is.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum SidebarSubSize {
    /// The ordinary nested entry.
    #[default]
    Md,
    /// A nested entry in smaller type.
    Sm,
}

impl SidebarSubSize {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Sm => "sm",
        }
    }
}
