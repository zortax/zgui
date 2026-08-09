//! The kernel's code vocabulary, as values that cannot be mixed up.
//!
//! Every event the kernel reports is a type, a code and a value, and all three are integers. A
//! code means nothing without its type — `1` is `KEY_ESC` under `EV_KEY` and `REL_Y` under
//! `EV_REL` — so a code is a bare integer only for as long as it takes to reach a type here.
//!
//! # Names
//!
//! Every constant below keeps the name `input-event-codes.h` gives it, prefix and all. So
//! [`Key::KEY_A`] is `KEY_A`, [`EventType::EV_REL`] is `EV_REL`, and a code read in a kernel
//! source, a `libinput` log or an `evtest` dump is the same word here. The numbers come from the
//! vendored header through `sys`, so no value in this file is transcribed.

/// A code the kernel numbers, and the vocabulary it is drawn from.
///
/// Every code type in this module implements it and nothing else does. It lets a *set* of codes
/// say which vocabulary it holds: [`Bitmap<C>`](crate::device::Bitmap) is the set, and `C` stops a
/// map of relative axes being read as a map of keys. See [`Bitmap`](crate::device::Bitmap) for a
/// worked example.
pub trait Code: Copy {
    /// The event type these codes arrive under.
    ///
    /// [`EventType`]'s own is `EV_SYN`, which is the kernel's own arrangement: `EVIOCGBIT(0, len)`
    /// asks which event types a device has at all, packed into the slot `EV_SYN` would occupy.
    const KIND: EventType;

    /// How many codes of this vocabulary the kernel names.
    ///
    /// A bitmap of them is this many bits, so it is also how much of one to ask a device for.
    const COUNT: u16;

    /// The code the kernel numbers `raw`.
    fn new(raw: u16) -> Self;

    /// The kernel's number for this code.
    fn raw(self) -> u16;
}

/// Declares a code type and the kernel constants that belong to it.
macro_rules! codes {
    (
        $(#[$doc:meta])*
        $name:ident, kind = $kind:expr, count = $count:expr; { $($code:ident),* $(,)? }
    ) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u16);

        impl Code for $name {
            const KIND: EventType = $kind;
            const COUNT: u16 = $count as u16;

            fn new(raw: u16) -> Self {
                Self(raw)
            }

            fn raw(self) -> u16 {
                self.0
            }
        }

        impl $name {
            $(
                #[doc = concat!("The kernel's `", stringify!($code), "`.")]
                pub const $code: Self = Self(crate::sys::$code as u16);
            )*

            /// The code the kernel numbers `raw`.
            ///
            /// Any number is a code. A kernel newer than this crate reports codes it has never
            /// heard of, and dropping those would lose the events a caller most needs to see.
            pub const fn new(raw: u16) -> Self {
                Self(raw)
            }

            /// The kernel's number for this code.
            pub const fn raw(self) -> u16 {
                self.0
            }
        }

        impl From<u16> for $name {
            fn from(raw: u16) -> Self {
                Self(raw)
            }
        }
    };
}

codes! {
    /// Which vocabulary an event's code is drawn from.
    ///
    /// A device reports the types it emits, and a code is read against the type it arrived
    /// under.
    EventType, kind = EventType::EV_SYN, count = crate::sys::EV_CNT; {
        EV_SYN, EV_KEY, EV_REL, EV_ABS, EV_MSC, EV_SW, EV_LED, EV_SND, EV_REP, EV_FF, EV_PWR,
        EV_FF_STATUS,
    }
}

codes! {
    /// What a synchronisation event says.
    ///
    /// [`Synchronisation::SYN_REPORT`] ends a batch: everything before it happened at once.
    /// [`Synchronisation::SYN_DROPPED`] says the kernel's queue overflowed and the events
    /// around it are incomplete.
    Synchronisation, kind = EventType::EV_SYN, count = crate::sys::SYN_CNT; {
        SYN_REPORT, SYN_CONFIG, SYN_MT_REPORT, SYN_DROPPED,
    }
}

codes! {
    /// A key or a button.
    ///
    /// The kernel gives both one code space, laid out in alternating blocks: the keys a keyboard
    /// sends, then the buttons, then two more blocks of keys behind those. So no single boundary
    /// divides the space. [`Key::is_key`] is the three ranges that hold the keys, and it tells a
    /// keyboard from a mouse.
    Key, kind = EventType::EV_KEY, count = crate::sys::KEY_CNT; {
        KEY_RESERVED, KEY_ESC, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9,
        KEY_0, KEY_MINUS, KEY_EQUAL, KEY_BACKSPACE, KEY_TAB, KEY_Q, KEY_W, KEY_E, KEY_R, KEY_T,
        KEY_Y, KEY_U, KEY_I, KEY_O, KEY_P, KEY_LEFTBRACE, KEY_RIGHTBRACE, KEY_ENTER,
        KEY_LEFTCTRL, KEY_A, KEY_S, KEY_D, KEY_F, KEY_G, KEY_H, KEY_J, KEY_K, KEY_L,
        KEY_SEMICOLON, KEY_APOSTROPHE, KEY_GRAVE, KEY_LEFTSHIFT, KEY_BACKSLASH, KEY_Z, KEY_X,
        KEY_C, KEY_V, KEY_B, KEY_N, KEY_M, KEY_COMMA, KEY_DOT, KEY_SLASH, KEY_RIGHTSHIFT,
        KEY_KPASTERISK, KEY_LEFTALT, KEY_SPACE, KEY_CAPSLOCK, KEY_F1, KEY_F2, KEY_F3, KEY_F4,
        KEY_F5, KEY_F6, KEY_F7, KEY_F8, KEY_F9, KEY_F10, KEY_NUMLOCK, KEY_SCROLLLOCK, KEY_KP7,
        KEY_KP8, KEY_KP9, KEY_KPMINUS, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KPPLUS, KEY_KP1, KEY_KP2,
        KEY_KP3, KEY_KP0, KEY_KPDOT, KEY_ZENKAKUHANKAKU, KEY_102ND, KEY_F11, KEY_F12, KEY_RO,
        KEY_KATAKANA, KEY_HIRAGANA, KEY_HENKAN, KEY_KATAKANAHIRAGANA, KEY_MUHENKAN,
        KEY_KPJPCOMMA, KEY_KPENTER, KEY_RIGHTCTRL, KEY_KPSLASH, KEY_SYSRQ, KEY_RIGHTALT,
        KEY_LINEFEED, KEY_HOME, KEY_UP, KEY_PAGEUP, KEY_LEFT, KEY_RIGHT, KEY_END, KEY_DOWN,
        KEY_PAGEDOWN, KEY_INSERT, KEY_DELETE, KEY_MACRO, KEY_MUTE, KEY_VOLUMEDOWN, KEY_VOLUMEUP,
        KEY_POWER, KEY_KPEQUAL, KEY_KPPLUSMINUS, KEY_PAUSE, KEY_SCALE, KEY_KPCOMMA, KEY_HANGEUL,
        KEY_HANGUEL, KEY_HANJA, KEY_YEN, KEY_LEFTMETA, KEY_RIGHTMETA, KEY_COMPOSE, KEY_STOP,
        KEY_AGAIN, KEY_PROPS, KEY_UNDO, KEY_FRONT, KEY_COPY, KEY_OPEN, KEY_PASTE, KEY_FIND,
        KEY_CUT, KEY_HELP, KEY_MENU, KEY_CALC, KEY_SETUP, KEY_SLEEP, KEY_WAKEUP, KEY_FILE,
        KEY_SENDFILE, KEY_DELETEFILE, KEY_XFER, KEY_PROG1, KEY_PROG2, KEY_WWW, KEY_MSDOS,
        KEY_COFFEE, KEY_SCREENLOCK, KEY_ROTATE_DISPLAY, KEY_DIRECTION, KEY_CYCLEWINDOWS,
        KEY_MAIL, KEY_BOOKMARKS, KEY_COMPUTER, KEY_BACK, KEY_FORWARD, KEY_CLOSECD, KEY_EJECTCD,
        KEY_EJECTCLOSECD, KEY_NEXTSONG, KEY_PLAYPAUSE, KEY_PREVIOUSSONG, KEY_STOPCD, KEY_RECORD,
        KEY_REWIND, KEY_PHONE, KEY_ISO, KEY_CONFIG, KEY_HOMEPAGE, KEY_REFRESH, KEY_EXIT,
        KEY_MOVE, KEY_EDIT, KEY_SCROLLUP, KEY_SCROLLDOWN, KEY_KPLEFTPAREN, KEY_KPRIGHTPAREN,
        KEY_NEW, KEY_REDO, KEY_F13, KEY_F14, KEY_F15, KEY_F16, KEY_F17, KEY_F18, KEY_F19,
        KEY_F20, KEY_F21, KEY_F22, KEY_F23, KEY_F24, KEY_PLAYCD, KEY_PAUSECD, KEY_PROG3,
        KEY_PROG4, KEY_ALL_APPLICATIONS, KEY_DASHBOARD, KEY_SUSPEND, KEY_CLOSE, KEY_PLAY,
        KEY_FASTFORWARD, KEY_BASSBOOST, KEY_PRINT, KEY_HP, KEY_CAMERA, KEY_SOUND, KEY_QUESTION,
        KEY_EMAIL, KEY_CHAT, KEY_SEARCH, KEY_CONNECT, KEY_FINANCE, KEY_SPORT, KEY_SHOP,
        KEY_ALTERASE, KEY_CANCEL, KEY_BRIGHTNESSDOWN, KEY_BRIGHTNESSUP, KEY_MEDIA,
        KEY_SWITCHVIDEOMODE, KEY_KBDILLUMTOGGLE, KEY_KBDILLUMDOWN, KEY_KBDILLUMUP, KEY_SEND,
        KEY_REPLY, KEY_FORWARDMAIL, KEY_SAVE, KEY_DOCUMENTS, KEY_BATTERY, KEY_BLUETOOTH,
        KEY_WLAN, KEY_UWB, KEY_UNKNOWN, KEY_VIDEO_NEXT, KEY_VIDEO_PREV, KEY_BRIGHTNESS_CYCLE,
        KEY_BRIGHTNESS_AUTO, KEY_BRIGHTNESS_ZERO, KEY_DISPLAY_OFF, KEY_WWAN, KEY_WIMAX,
        KEY_RFKILL, KEY_MICMUTE, BTN_MISC, BTN_0, BTN_1, BTN_2, BTN_3, BTN_4, BTN_5, BTN_6,
        BTN_7, BTN_8, BTN_9, BTN_MOUSE, BTN_LEFT, BTN_RIGHT, BTN_MIDDLE, BTN_SIDE, BTN_EXTRA,
        BTN_FORWARD, BTN_BACK, BTN_TASK, BTN_JOYSTICK, BTN_TRIGGER, BTN_THUMB, BTN_THUMB2,
        BTN_TOP, BTN_TOP2, BTN_PINKIE, BTN_BASE, BTN_BASE2, BTN_BASE3, BTN_BASE4, BTN_BASE5,
        BTN_BASE6, BTN_DEAD, BTN_GAMEPAD, BTN_SOUTH, BTN_A, BTN_EAST, BTN_B, BTN_C, BTN_NORTH,
        BTN_X, BTN_WEST, BTN_Y, BTN_Z, BTN_TL, BTN_TR, BTN_TL2, BTN_TR2, BTN_SELECT, BTN_START,
        BTN_MODE, BTN_THUMBL, BTN_THUMBR, BTN_DIGI, BTN_TOOL_PEN, BTN_TOOL_RUBBER,
        BTN_TOOL_BRUSH, BTN_TOOL_PENCIL, BTN_TOOL_AIRBRUSH, BTN_TOOL_FINGER, BTN_TOOL_MOUSE,
        BTN_TOOL_LENS, BTN_TOOL_QUINTTAP, BTN_STYLUS3, BTN_TOUCH, BTN_STYLUS, BTN_STYLUS2,
        BTN_TOOL_DOUBLETAP, BTN_TOOL_TRIPLETAP, BTN_TOOL_QUADTAP, BTN_WHEEL, BTN_GEAR_DOWN,
        BTN_GEAR_UP, KEY_OK, KEY_SELECT, KEY_GOTO, KEY_CLEAR, KEY_POWER2, KEY_OPTION, KEY_INFO,
        KEY_TIME, KEY_VENDOR, KEY_ARCHIVE, KEY_PROGRAM, KEY_CHANNEL, KEY_FAVORITES, KEY_EPG,
        KEY_PVR, KEY_MHP, KEY_LANGUAGE, KEY_TITLE, KEY_SUBTITLE, KEY_ANGLE, KEY_FULL_SCREEN,
        KEY_ZOOM, KEY_MODE, KEY_KEYBOARD, KEY_ASPECT_RATIO, KEY_SCREEN, KEY_PC, KEY_TV, KEY_TV2,
        KEY_VCR, KEY_VCR2, KEY_SAT, KEY_SAT2, KEY_CD, KEY_TAPE, KEY_RADIO, KEY_TUNER, KEY_PLAYER,
        KEY_TEXT, KEY_DVD, KEY_AUX, KEY_MP3, KEY_AUDIO, KEY_VIDEO, KEY_DIRECTORY, KEY_LIST,
        KEY_MEMO, KEY_CALENDAR, KEY_RED, KEY_GREEN, KEY_YELLOW, KEY_BLUE, KEY_CHANNELUP,
        KEY_CHANNELDOWN, KEY_FIRST, KEY_LAST, KEY_AB, KEY_NEXT, KEY_RESTART, KEY_SLOW,
        KEY_SHUFFLE, KEY_BREAK, KEY_PREVIOUS, KEY_DIGITS, KEY_TEEN, KEY_TWEN, KEY_VIDEOPHONE,
        KEY_GAMES, KEY_ZOOMIN, KEY_ZOOMOUT, KEY_ZOOMRESET, KEY_WORDPROCESSOR, KEY_EDITOR,
        KEY_SPREADSHEET, KEY_GRAPHICSEDITOR, KEY_PRESENTATION, KEY_DATABASE, KEY_NEWS,
        KEY_VOICEMAIL, KEY_ADDRESSBOOK, KEY_MESSENGER, KEY_DISPLAYTOGGLE, KEY_BRIGHTNESS_TOGGLE,
        KEY_SPELLCHECK, KEY_LOGOFF, KEY_DOLLAR, KEY_EURO, KEY_FRAMEBACK, KEY_FRAMEFORWARD,
        KEY_CONTEXT_MENU, KEY_MEDIA_REPEAT, KEY_10CHANNELSUP, KEY_10CHANNELSDOWN, KEY_IMAGES,
        KEY_NOTIFICATION_CENTER, KEY_PICKUP_PHONE, KEY_HANGUP_PHONE, KEY_LINK_PHONE, KEY_DEL_EOL,
        KEY_DEL_EOS, KEY_INS_LINE, KEY_DEL_LINE, KEY_FN, KEY_FN_ESC, KEY_FN_F1, KEY_FN_F2,
        KEY_FN_F3, KEY_FN_F4, KEY_FN_F5, KEY_FN_F6, KEY_FN_F7, KEY_FN_F8, KEY_FN_F9, KEY_FN_F10,
        KEY_FN_F11, KEY_FN_F12, KEY_FN_1, KEY_FN_2, KEY_FN_D, KEY_FN_E, KEY_FN_F, KEY_FN_S,
        KEY_FN_B, KEY_FN_RIGHT_SHIFT, KEY_BRL_DOT1, KEY_BRL_DOT2, KEY_BRL_DOT3, KEY_BRL_DOT4,
        KEY_BRL_DOT5, KEY_BRL_DOT6, KEY_BRL_DOT7, KEY_BRL_DOT8, KEY_BRL_DOT9, KEY_BRL_DOT10,
        KEY_NUMERIC_0, KEY_NUMERIC_1, KEY_NUMERIC_2, KEY_NUMERIC_3, KEY_NUMERIC_4, KEY_NUMERIC_5,
        KEY_NUMERIC_6, KEY_NUMERIC_7, KEY_NUMERIC_8, KEY_NUMERIC_9, KEY_NUMERIC_STAR,
        KEY_NUMERIC_POUND, KEY_NUMERIC_A, KEY_NUMERIC_B, KEY_NUMERIC_C, KEY_NUMERIC_D,
        KEY_CAMERA_FOCUS, KEY_WPS_BUTTON, KEY_TOUCHPAD_TOGGLE, KEY_TOUCHPAD_ON, KEY_TOUCHPAD_OFF,
        KEY_CAMERA_ZOOMIN, KEY_CAMERA_ZOOMOUT, KEY_CAMERA_UP, KEY_CAMERA_DOWN, KEY_CAMERA_LEFT,
        KEY_CAMERA_RIGHT, KEY_ATTENDANT_ON, KEY_ATTENDANT_OFF, KEY_ATTENDANT_TOGGLE,
        KEY_LIGHTS_TOGGLE, BTN_DPAD_UP, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT, BTN_GRIPL,
        BTN_GRIPR, BTN_GRIPL2, BTN_GRIPR2, KEY_ALS_TOGGLE, KEY_ROTATE_LOCK_TOGGLE,
        KEY_REFRESH_RATE_TOGGLE, KEY_BUTTONCONFIG, KEY_TASKMANAGER, KEY_JOURNAL,
        KEY_CONTROLPANEL, KEY_APPSELECT, KEY_SCREENSAVER, KEY_VOICECOMMAND, KEY_ASSISTANT,
        KEY_KBD_LAYOUT_NEXT, KEY_EMOJI_PICKER, KEY_DICTATE, KEY_CAMERA_ACCESS_ENABLE,
        KEY_CAMERA_ACCESS_DISABLE, KEY_CAMERA_ACCESS_TOGGLE, KEY_ACCESSIBILITY,
        KEY_DO_NOT_DISTURB, KEY_BRIGHTNESS_MIN, KEY_BRIGHTNESS_MAX, KEY_EPRIVACY_SCREEN_ON,
        KEY_EPRIVACY_SCREEN_OFF, KEY_ACTION_ON_SELECTION, KEY_CONTEXTUAL_INSERT,
        KEY_CONTEXTUAL_QUERY, KEY_KBDINPUTASSIST_PREV, KEY_KBDINPUTASSIST_NEXT,
        KEY_KBDINPUTASSIST_PREVGROUP, KEY_KBDINPUTASSIST_NEXTGROUP, KEY_KBDINPUTASSIST_ACCEPT,
        KEY_KBDINPUTASSIST_CANCEL, KEY_RIGHT_UP, KEY_RIGHT_DOWN, KEY_LEFT_UP, KEY_LEFT_DOWN,
        KEY_ROOT_MENU, KEY_MEDIA_TOP_MENU, KEY_NUMERIC_11, KEY_NUMERIC_12, KEY_AUDIO_DESC,
        KEY_3D_MODE, KEY_NEXT_FAVORITE, KEY_STOP_RECORD, KEY_PAUSE_RECORD, KEY_VOD, KEY_UNMUTE,
        KEY_FASTREVERSE, KEY_SLOWREVERSE, KEY_DATA, KEY_ONSCREEN_KEYBOARD,
        KEY_PRIVACY_SCREEN_TOGGLE, KEY_SELECTIVE_SCREENSHOT, KEY_NEXT_ELEMENT,
        KEY_PREVIOUS_ELEMENT, KEY_AUTOPILOT_ENGAGE_TOGGLE, KEY_MARK_WAYPOINT, KEY_SOS,
        KEY_NAV_CHART, KEY_FISHING_CHART, KEY_SINGLE_RANGE_RADAR, KEY_DUAL_RANGE_RADAR,
        KEY_RADAR_OVERLAY, KEY_TRADITIONAL_SONAR, KEY_CLEARVU_SONAR, KEY_SIDEVU_SONAR,
        KEY_NAV_INFO, KEY_BRIGHTNESS_MENU, KEY_MACRO1, KEY_MACRO2, KEY_MACRO3, KEY_MACRO4,
        KEY_MACRO5, KEY_MACRO6, KEY_MACRO7, KEY_MACRO8, KEY_MACRO9, KEY_MACRO10, KEY_MACRO11,
        KEY_MACRO12, KEY_MACRO13, KEY_MACRO14, KEY_MACRO15, KEY_MACRO16, KEY_MACRO17,
        KEY_MACRO18, KEY_MACRO19, KEY_MACRO20, KEY_MACRO21, KEY_MACRO22, KEY_MACRO23,
        KEY_MACRO24, KEY_MACRO25, KEY_MACRO26, KEY_MACRO27, KEY_MACRO28, KEY_MACRO29,
        KEY_MACRO30, KEY_MACRO_RECORD_START, KEY_MACRO_RECORD_STOP, KEY_MACRO_PRESET_CYCLE,
        KEY_MACRO_PRESET1, KEY_MACRO_PRESET2, KEY_MACRO_PRESET3, KEY_KBD_LCD_MENU1,
        KEY_KBD_LCD_MENU2, KEY_KBD_LCD_MENU3, KEY_KBD_LCD_MENU4, KEY_KBD_LCD_MENU5,
        KEY_PERFORMANCE, BTN_TRIGGER_HAPPY, BTN_TRIGGER_HAPPY1, BTN_TRIGGER_HAPPY2,
        BTN_TRIGGER_HAPPY3, BTN_TRIGGER_HAPPY4, BTN_TRIGGER_HAPPY5, BTN_TRIGGER_HAPPY6,
        BTN_TRIGGER_HAPPY7, BTN_TRIGGER_HAPPY8, BTN_TRIGGER_HAPPY9, BTN_TRIGGER_HAPPY10,
        BTN_TRIGGER_HAPPY11, BTN_TRIGGER_HAPPY12, BTN_TRIGGER_HAPPY13, BTN_TRIGGER_HAPPY14,
        BTN_TRIGGER_HAPPY15, BTN_TRIGGER_HAPPY16, BTN_TRIGGER_HAPPY17, BTN_TRIGGER_HAPPY18,
        BTN_TRIGGER_HAPPY19, BTN_TRIGGER_HAPPY20, BTN_TRIGGER_HAPPY21, BTN_TRIGGER_HAPPY22,
        BTN_TRIGGER_HAPPY23, BTN_TRIGGER_HAPPY24, BTN_TRIGGER_HAPPY25, BTN_TRIGGER_HAPPY26,
        BTN_TRIGGER_HAPPY27, BTN_TRIGGER_HAPPY28, BTN_TRIGGER_HAPPY29, BTN_TRIGGER_HAPPY30,
        BTN_TRIGGER_HAPPY31, BTN_TRIGGER_HAPPY32, BTN_TRIGGER_HAPPY33, BTN_TRIGGER_HAPPY34,
        BTN_TRIGGER_HAPPY35, BTN_TRIGGER_HAPPY36, BTN_TRIGGER_HAPPY37, BTN_TRIGGER_HAPPY38,
        BTN_TRIGGER_HAPPY39, BTN_TRIGGER_HAPPY40,
    }
}

codes! {
    /// An axis that reports a change.
    ///
    /// A mouse says how far it moved, never where it is.
    Relative, kind = EventType::EV_REL, count = crate::sys::REL_CNT; {
        REL_X, REL_Y, REL_Z, REL_RX, REL_RY, REL_RZ, REL_HWHEEL, REL_DIAL, REL_WHEEL, REL_MISC,
        REL_RESERVED, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES,
    }
}

codes! {
    /// An axis that reports a position.
    ///
    /// A touchscreen, a tablet and a joystick say where they are, inside a range the
    /// device reports through `EVIOCGABS`.
    Absolute, kind = EventType::EV_ABS, count = crate::sys::ABS_CNT; {
        ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_THROTTLE, ABS_RUDDER, ABS_WHEEL,
        ABS_GAS, ABS_BRAKE, ABS_HAT0X, ABS_HAT0Y, ABS_HAT1X, ABS_HAT1Y, ABS_HAT2X, ABS_HAT2Y,
        ABS_HAT3X, ABS_HAT3Y, ABS_PRESSURE, ABS_DISTANCE, ABS_TILT_X, ABS_TILT_Y, ABS_TOOL_WIDTH,
        ABS_VOLUME, ABS_PROFILE, ABS_SND_PROFILE, ABS_MISC, ABS_RESERVED, ABS_MT_SLOT,
        ABS_MT_TOUCH_MAJOR, ABS_MT_TOUCH_MINOR, ABS_MT_WIDTH_MAJOR, ABS_MT_WIDTH_MINOR,
        ABS_MT_ORIENTATION, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TOOL_TYPE,
        ABS_MT_BLOB_ID, ABS_MT_TRACKING_ID, ABS_MT_PRESSURE, ABS_MT_DISTANCE, ABS_MT_TOOL_X,
        ABS_MT_TOOL_Y,
    }
}

impl Key {
    /// Returns `true` for a key and `false` for a button.
    ///
    /// The kernel lays the two out in alternating blocks, so the answer is three ranges. Under
    /// `BTN_MISC` are the keys a keyboard sends: the letters, the digits, the modifiers, the
    /// function keys and the media keys. The buttons follow. Then the kernel added two more blocks
    /// of keys behind them — `KEY_OK` at `0x160` and `KEY_ALS_TOGGLE` at `0x230` — each ending
    /// where a block of buttons begins.
    ///
    /// ```
    /// use zgui_evdev::Key;
    ///
    /// assert!(Key::KEY_A.is_key());
    /// assert!(!Key::BTN_LEFT.is_key());
    /// // A remote control reports only the block behind the buttons, and it is still a keyboard.
    /// assert!(Key::KEY_CHANNELUP.is_key());
    /// ```
    ///
    /// The three ranges are udev's own, from the `input_id` builtin that decides `ID_INPUT_KEY`,
    /// and they are exact against the vendored header: every `KEY_*` it names falls inside one of
    /// them, and no `BTN_*` does.
    ///
    /// `KEY_RESERVED` is the one exclusion. It is code zero, it sends nothing, and a driver that
    /// sets its bit says nothing by doing so. udev counts it and this does not, because
    /// [`Roles`](crate::device::Roles) asks the answer whether there is a keyboard here.
    pub const fn is_key(self) -> bool {
        let code = self.0;
        (code > Self::KEY_RESERVED.0 && code < Self::BTN_MISC.0)
            || (code >= Self::KEY_OK.0 && code < Self::BTN_DPAD_UP.0)
            || (code >= Self::KEY_ALS_TOGGLE.0 && code < Self::BTN_TRIGGER_HAPPY.0)
    }
}

#[cfg(test)]
mod tests {
    //! The numbers, against the ones the headers name.
    //!
    //! The tables are generated from `sys`, so what is worth asserting is that the right constant
    //! reached the right type. A code under the wrong type is the defect these newtypes exist to
    //! refuse, and a sample of the values shows that the tables were not shifted by one.

    use super::*;

    #[test]
    fn the_codes_carry_the_numbers_the_headers_name() {
        assert_eq!(EventType::EV_SYN.raw(), 0x00);
        assert_eq!(EventType::EV_KEY.raw(), 0x01);
        assert_eq!(EventType::EV_REL.raw(), 0x02);
        assert_eq!(EventType::EV_ABS.raw(), 0x03);
        assert_eq!(Synchronisation::SYN_REPORT.raw(), 0);
        assert_eq!(Synchronisation::SYN_DROPPED.raw(), 3);
        assert_eq!(Key::KEY_ESC.raw(), 1);
        assert_eq!(Key::KEY_A.raw(), 30);
        assert_eq!(Key::KEY_LEFTSHIFT.raw(), 42);
        assert_eq!(Key::BTN_LEFT.raw(), 0x110);
        assert_eq!(Relative::REL_X.raw(), 0x00);
        assert_eq!(Relative::REL_WHEEL.raw(), 0x08);
        assert_eq!(Absolute::ABS_X.raw(), 0x00);
        assert_eq!(Absolute::ABS_MT_POSITION_X.raw(), 0x35);
    }

    #[test]
    fn the_same_number_under_two_types_is_two_different_values() {
        // `1` is `KEY_ESC` and it is `REL_Y`. Nothing in this crate can compare the two: a code
        // carries its type before anything looks at it.
        assert_eq!(Key::KEY_ESC.raw(), Relative::REL_Y.raw());
    }

    #[test]
    fn a_key_is_a_key_in_any_of_the_blocks_the_kernel_put_them_in() {
        assert!(Key::KEY_A.is_key(), "a letter is in the first block");
        assert!(
            Key::KEY_MICMUTE.is_key(),
            "and so is the last code before the buttons start"
        );
        // The two blocks the kernel added behind the buttons. A remote control or an infrared
        // receiver reports only these, and reading the first block alone would leave one
        // classified as nothing at all, so a consumer would never open it.
        assert!(Key::KEY_OK.is_key(), "the first block behind the buttons");
        assert!(Key::KEY_CHANNELUP.is_key());
        assert!(Key::KEY_SUBTITLE.is_key());
        assert!(Key::KEY_ALS_TOGGLE.is_key(), "the second such block");
        assert!(Key::KEY_KBD_LCD_MENU1.is_key());
    }

    #[test]
    fn a_button_is_not_a_key_in_any_of_the_blocks_between_them() {
        assert!(!Key::BTN_MISC.is_key(), "the first block of buttons");
        assert!(!Key::BTN_LEFT.is_key(), "a mouse button");
        assert!(!Key::BTN_DPAD_UP.is_key(), "the block that ends the first");
        assert!(
            !Key::BTN_TRIGGER_HAPPY.is_key(),
            "and the one that ends the second"
        );
    }

    #[test]
    fn the_code_that_sends_nothing_is_not_a_key() {
        // `KEY_RESERVED` is code zero. A driver that sets its bit has said nothing, and counting
        // it would make a device with one meaningless code into a keyboard.
        assert!(!Key::KEY_RESERVED.is_key());
        assert!(
            Key::new(1).is_key(),
            "the code above it is where the block really starts"
        );
    }

    #[test]
    fn a_code_the_kernel_has_not_named_yet_is_still_a_code() {
        // A kernel newer than this crate reports codes it has never heard of. Keeping the number
        // is what lets a caller act on one.
        assert_eq!(Key::new(0x2ff).raw(), 0x2ff);
        assert_eq!(Absolute::from(0x3f).raw(), 0x3f);
    }
}
