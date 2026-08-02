//! The named bits, one per interaction state.

use crate::state::UiState;

/// Declares one bit per state, and builds the name table the debug formatting reads.
macro_rules! states {
    ($( $(#[$meta:meta])* $name:ident = $index:literal; )+) => {
        impl UiState {
            $(
                $(#[$meta])*
                pub const $name: Self = Self(1u64 << $index);
            )+
        }

        /// Every single-bit state paired with the name it is written with.
        pub(crate) const NAMED: &[(UiState, &str)] = &[
            $( (UiState::$name, stringify!($name)), )+
        ];
    };
}

states! {
    /// The pointer is pressed on this element: `:active`.
    ACTIVE = 0;
    /// This element has focus: `:focus`.
    FOCUS = 1;
    /// The pointer is over this element: `:hover`.
    HOVER = 2;
    /// This element can be interacted with: `:enabled`.
    ENABLED = 3;
    /// This element cannot be interacted with: `:disabled`.
    DISABLED = 4;
    /// This control is checked: `:checked`.
    CHECKED = 5;
    /// This control is neither checked nor unchecked: `:indeterminate`.
    INDETERMINATE = 6;
    /// This field is empty and showing its placeholder: `:placeholder-shown`.
    PLACEHOLDER_SHOWN = 7;
    /// This element is the target of the document's fragment: `:target`.
    URL_TARGET = 8;
    /// This element is presented full screen: `:fullscreen`.
    FULLSCREEN = 9;
    /// This control's value satisfies its constraints: `:valid`.
    VALID = 10;
    /// This control's value violates a constraint: `:invalid`.
    INVALID = 11;
    /// The user has interacted with this control and its value is valid: `:user-valid`.
    USER_VALID = 12;
    /// The user has interacted with this control and its value is invalid: `:user-invalid`.
    USER_INVALID = 13;
    /// This replaced element failed to load its resource.
    BROKEN = 14;
    /// This control must have a value: `:required`.
    REQUIRED = 15;
    /// This control need not have a value: `:optional`.
    OPTIONAL = 16;
    /// This element's behaviour is defined rather than that of an unknown name: `:defined`.
    DEFINED = 17;
    /// This link has been followed: `:visited`.
    VISITED = 18;
    /// This link has not been followed: `:link`.
    UNVISITED = 19;
    /// A drag is currently hovering this element.
    DRAG_OVER = 20;
    /// This control's value is inside its range: `:in-range`.
    IN_RANGE = 21;
    /// This control's value is outside its range: `:out-of-range`.
    OUT_OF_RANGE = 22;
    /// This control's value cannot be edited: `:read-only`.
    READ_ONLY = 23;
    /// This control's value can be edited: `:read-write`.
    READ_WRITE = 24;
    /// This control is the default of its group: `:default`.
    DEFAULT = 25;
    /// This gauge's value is in its optimum band.
    OPTIMUM = 26;
    /// This gauge's value is one band away from optimum.
    SUB_OPTIMUM = 27;
    /// This gauge's value is two bands away from optimum.
    SUB_SUB_OPTIMUM = 28;
    /// This element raises the mathematical script level of its contents.
    INCREMENT_SCRIPT_LEVEL = 29;
    /// Focus arrived by a route that should show a focus ring: `:focus-visible`.
    FOCUS_RING = 30;
    /// This element contains the focused element: `:focus-within`.
    FOCUS_WITHIN = 31;
    /// This element's resolved direction is left to right: `:dir(ltr)`.
    LTR = 32;
    /// This element's resolved direction is right to left: `:dir(rtl)`.
    RTL = 33;
    /// This element carries an explicit direction declaration.
    HAS_DIR_ATTR = 34;
    /// This element's explicit direction declaration says left to right.
    HAS_DIR_ATTR_LTR = 35;
    /// This element's explicit direction declaration says right to left.
    HAS_DIR_ATTR_RTL = 36;
    /// This element's explicit direction declaration defers to its content.
    HAS_DIR_ATTR_LIKE_AUTO = 37;
    /// This field was filled in automatically.
    AUTOFILL = 38;
    /// This field is showing a preview of an automatic fill.
    AUTOFILL_PREVIEW = 39;
    /// This element is modal: `:modal`.
    MODAL = 40;
    /// This element and its subtree do not respond to interaction.
    INERT = 41;
    /// This element is the topmost modal in the top layer.
    TOPMOST_MODAL = 42;
    /// A developer tool is highlighting this element.
    DEVTOOLS_HIGHLIGHTED = 43;
    /// A developer tool is animating a style change on this element.
    STYLE_EDITOR_TRANSITIONING = 44;
    /// This field's value is empty, which is what reveals its clear button.
    VALUE_EMPTY = 45;
    /// This field's obscured value is currently revealed.
    REVEALED = 46;
    /// This popover is showing: `:popover-open`.
    POPOVER_OPEN = 47;
    /// This slot has assigned content: `:has-slotted`.
    HAS_SLOTTED = 48;
    /// This openable element is open: `:open`.
    OPEN = 49;
    /// This element is the subject of a running view transition.
    ACTIVE_VIEW_TRANSITION = 50;
    /// This element is suppressed from a print selection.
    SUPPRESS_FOR_PRINT_SELECTION = 51;
    /// This media element is paused: `:paused`.
    PAUSED = 52;
    /// This media element is seeking: `:seeking`.
    SEEKING = 53;
    /// This media element is buffering: `:buffering`.
    BUFFERING = 54;
    /// This media element has stalled: `:stalled`.
    STALLED = 55;
    /// This media element is muted: `:muted`.
    MUTED = 56;
    /// This full-screen element requested the keyboard lock.
    ///
    /// This bit is the low bit of [`UiState::HEADING_LEVEL`] as well, which is a property of the
    /// layout being mirrored rather than a choice made here. Nothing sets both.
    FULLSCREEN_KEYBOARD_LOCK = 57;
    /// This element is playing picture in picture.
    PICTURE_IN_PICTURE = 61;
}

impl UiState {
    /// The four bits that pack a heading's level, one through nine.
    ///
    /// The level is a small integer rather than a flag, so it occupies a field inside the same
    /// word instead of a bit of its own. Read it with [`UiState::heading_level`].
    pub const HEADING_LEVEL: Self = Self(0b1111u64 << Self::HEADING_LEVEL_OFFSET);

    /// How far the heading-level field is shifted from the low bit of the word.
    pub const HEADING_LEVEL_OFFSET: u32 = 57;

    /// The states a view may assert about its own element.
    ///
    /// Every other bit is computed by the framework from what actually happened, so a view cannot
    /// claim it. The one control state that is *not* here is selection: no selector and no state
    /// bit expresses it, so a selected item carries a custom state instead.
    pub const AUTHOR_SETTABLE: Self = Self(
        Self::CHECKED.0
            | Self::DISABLED.0
            | Self::OPEN.0
            | Self::INDETERMINATE.0
            | Self::PLACEHOLDER_SHOWN.0
            | Self::READ_ONLY.0
            | Self::REQUIRED.0
            | Self::INVALID.0,
    );

    /// Every validity state at once.
    pub const VALIDITY: Self =
        Self(Self::VALID.0 | Self::INVALID.0 | Self::USER_VALID.0 | Self::USER_INVALID.0);

    /// Both resolved-direction states at once.
    pub const DIRECTION: Self = Self(Self::LTR.0 | Self::RTL.0);

    /// Every explicit-direction-declaration state at once.
    pub const DIRECTION_ATTR: Self = Self(
        Self::HAS_DIR_ATTR.0
            | Self::HAS_DIR_ATTR_LTR.0
            | Self::HAS_DIR_ATTR_RTL.0
            | Self::HAS_DIR_ATTR_LIKE_AUTO.0,
    );

    /// Both link states at once.
    pub const LINK: Self = Self(Self::VISITED.0 | Self::UNVISITED.0);

    /// Every gauge band at once.
    pub const GAUGE: Self = Self(Self::OPTIMUM.0 | Self::SUB_OPTIMUM.0 | Self::SUB_SUB_OPTIMUM.0);

    /// This element's heading level, or `None` when it is not a heading.
    ///
    /// ```
    /// use zgui_vocab::UiState;
    ///
    /// assert_eq!(UiState::EMPTY.heading_level(), None);
    /// assert_eq!(UiState::with_heading_level(3).heading_level(), Some(3));
    /// ```
    pub const fn heading_level(self) -> Option<u8> {
        let level = (self.0 & Self::HEADING_LEVEL.0) >> Self::HEADING_LEVEL_OFFSET;
        if level == 0 { None } else { Some(level as u8) }
    }

    /// The state of a heading at `level`, which is clamped to the one-to-nine range.
    pub const fn with_heading_level(level: u8) -> Self {
        let clamped = if level > 9 { 9 } else { level } as u64;
        Self(clamped << Self::HEADING_LEVEL_OFFSET)
    }
}
