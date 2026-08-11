//! Which keyboard this machine is set to, read from what the machine states.
//!
//! libxkbcommon compiles a keymap from five names: the rules, the model, the layout, the variant
//! and the options. A name a caller leaves unset is one the library fills in for itself — it reads
//! `XKB_DEFAULT_RULES` and its four siblings, and where the environment holds none of them it uses
//! the names it was built with, which are `evdev`, `pc105` and **`us`**.
//!
//! Setting those variables is a convention a session manager follows. A getty follows no such
//! convention, so a program on a bare virtual terminal that states nothing types American on every
//! keyboard in the world, while the terminal beside it types what the machine is set to. What the
//! machine is set to is written in files, and this is where they are read.
//!
//! # The cascade
//!
//! Four sources, and the first that states a layout answers with every name it states:
//!
//! 1. **The environment** ([`Origin::Environment`]). libxkbcommon's own convention, and what a
//!    session manager sets. Only `XKB_DEFAULT_LAYOUT` is looked at, and only to see whether the
//!    environment says anything at all: the answer is empty names, so the library reads all five
//!    variables itself and one this module has never heard of keeps working.
//! 2. **`/etc/vconsole.conf`** ([`Origin::VirtualConsole`]), which states `XKBLAYOUT`, `XKBMODEL`,
//!    `XKBVARIANT` and `XKBOPTIONS`. This is systemd's canonical place for them.
//! 3. **`/etc/default/keyboard`** ([`Origin::Debian`]), which states the same four names.
//!    `keyboard(5)` describes it, and it is where Debian, Ubuntu and their derivatives keep what
//!    the machine's keyboard is. Those machines are the largest family of Linux desktops and most
//!    of them hold no `/etc/vconsole.conf` at all.
//! 4. **`/etc/X11/xorg.conf.d/00-keyboard.conf`** ([`Origin::Xorg`]), which `systemd-localed`
//!    writes when a person runs `localectl set-x11-keymap`.
//!
//! A machine that states none of them is [`Origin::Nowhere`], and
//! [`layout::find`](super::layout::find) reads the kernel's own console keymap there rather than
//! letting libxkbcommon fall back to `us`.
//!
//! **A file answers on its layout alone.** A file that states options and no layout states no
//! keyboard, so the cascade goes on to the next source and that file's other names go with it.
//! Every name the answer leaves unset is still libxkbcommon's to fill in, so `XKB_DEFAULT_MODEL`
//! set beside a layout stated in a file reaches the keymap.
//!
//! # Why the files sit in that order
//!
//! **The generated file is last on every machine.** `00-keyboard.conf` is written by a tool, and
//! each of the three sources above it is what a person sets. A machine whose settings were changed
//! once through `localectl` and again by hand holds a stale `00-keyboard.conf`, and the file the
//! person edited is the answer.
//!
//! **The two `KEY=value` files belong to different families**, and one machine rarely holds both:
//! `/etc/vconsole.conf` is systemd's and `/etc/default/keyboard` is `console-setup`'s. So the order
//! between them settles one machine — a Debian machine somebody ran `localectl` on, because
//! `localectl` writes an `XKBLAYOUT` into `/etc/vconsole.conf` there. That statement came after the
//! one the installer wrote into `/etc/default/keyboard`, so `/etc/vconsole.conf` answers first.
//!
//! # How much of each file is read
//!
//! Each is read for a handful of names. `/etc/vconsole.conf` and `/etc/default/keyboard` are both
//! `KEY=value` lines, which is all systemd writes into the first and all a shell needs of the
//! second, so one reader serves both. `00-keyboard.conf` is read for its `Option "XkbLayout" "de"`
//! lines and for nothing else — no section, no `MatchIsKeyboard`, no include — because the file
//! this reads is the small fixed one `systemd-localed` writes. A machine whose X server is
//! configured by hand can hold anything in that directory, and what this reads there is one file
//! that a program wrote.
//!
//! # The console keymap name
//!
//! `/etc/vconsole.conf` also states `KEYMAP`, which names a keymap the kernel's console driver
//! loads: `de-latin1`. Nothing here reads it. The correspondence between those names and xkb layout
//! names is a table systemd keeps, and a guess at it is how a keyboard ends up on a layout nobody
//! set. A machine that states `KEYMAP` and no xkb layout has that keymap in the kernel already,
//! which [`layout::Source::Console`](super::layout::Source::Console) reads.

use std::fmt;

use zgui_xkb::RuleNames;

/// systemd's canonical place for what a machine's keyboard is.
const VCONSOLE: &str = "/etc/vconsole.conf";

/// Where Debian and its derivatives keep it. See `keyboard(5)`.
const DEBIAN_KEYBOARD: &str = "/etc/default/keyboard";

/// The file `systemd-localed` writes the X11 keyboard settings into.
const XORG_KEYBOARD: &str = "/etc/X11/xorg.conf.d/00-keyboard.conf";

/// The variable that states a layout to libxkbcommon.
const LAYOUT_VARIABLE: &str = "XKB_DEFAULT_LAYOUT";

/// The keyword an xorg option line begins with.
const OPTION: &str = "Option";

/// Where a machine states which keyboard it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// `XKB_DEFAULT_LAYOUT` and its siblings, which libxkbcommon reads for itself.
    Environment,
    /// `/etc/vconsole.conf`.
    VirtualConsole,
    /// `/etc/default/keyboard`.
    Debian,
    /// `/etc/X11/xorg.conf.d/00-keyboard.conf`.
    Xorg,
    /// Nothing. The names are libxkbcommon's own, which is `us`.
    Nowhere,
}

/// Names the source rather than the setting, so a line reads "stated by `<this>`".
impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let source = match self {
            Self::Environment => "XKB_DEFAULT_LAYOUT and its siblings",
            Self::VirtualConsole => VCONSOLE,
            Self::Debian => DEBIAN_KEYBOARD,
            Self::Xorg => XORG_KEYBOARD,
            Self::Nowhere => "nothing on this machine",
        };
        f.write_str(source)
    }
}

/// The names a keymap is compiled from, and where they were stated.
///
/// Both halves reach the line a person reads at start-up, because a machine typing the wrong
/// letters is diagnosed from the source that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    /// The names. A name left unset is one libxkbcommon fills in for itself.
    pub(crate) names: RuleNames,
    /// Which source stated them.
    pub(crate) from: Origin,
}

/// One line: the names, and where they came from.
impl fmt::Display for Asked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, stated by {}", self.names, self.from)
    }
}

/// What a machine states about its keyboard, as the text it states it in.
///
/// Every field is content rather than a path, so [`of`] is arithmetic over strings and runs with no
/// machine state at all. [`Machine::read`] is the one thing in this module that reads anything.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Machine {
    /// What `XKB_DEFAULT_LAYOUT` holds.
    ///
    /// The four siblings are read by nothing here. See the module documentation.
    pub(crate) environment: Option<String>,
    /// What `/etc/vconsole.conf` holds.
    pub(crate) vconsole: Option<String>,
    /// What `/etc/default/keyboard` holds.
    pub(crate) debian: Option<String>,
    /// What `/etc/X11/xorg.conf.d/00-keyboard.conf` holds.
    pub(crate) xorg: Option<String>,
}

impl Machine {
    /// Reads the environment and all three files.
    ///
    /// A file this process cannot read states nothing, which is the ordinary answer on a machine
    /// that has none of them.
    pub(crate) fn read() -> Self {
        Self {
            environment: std::env::var(LAYOUT_VARIABLE).ok(),
            vconsole: std::fs::read_to_string(VCONSOLE).ok(),
            debian: std::fs::read_to_string(DEBIAN_KEYBOARD).ok(),
            xorg: std::fs::read_to_string(XORG_KEYBOARD).ok(),
        }
    }
}

/// Returns the names this machine states, and which source stated them.
///
/// The cascade the module documentation describes, and the whole of the decision.
pub(crate) fn of(machine: &Machine) -> Asked {
    // The answer is empty names, so libxkbcommon reads every variable for itself. What it reads
    // there is more than the one name this looked at. Empty is the library's own test for an absent
    // name, so a variable set to nothing states nothing here either.
    if machine
        .environment
        .as_deref()
        .is_some_and(|layout| !layout.is_empty())
    {
        return Asked {
            names: RuleNames::default(),
            from: Origin::Environment,
        };
    }
    if let Some(names) = machine.vconsole.as_deref().and_then(xkb_variables) {
        return Asked {
            names,
            from: Origin::VirtualConsole,
        };
    }
    if let Some(names) = machine.debian.as_deref().and_then(xkb_variables) {
        return Asked {
            names,
            from: Origin::Debian,
        };
    }
    if let Some(names) = machine.xorg.as_deref().and_then(xorg) {
        return Asked {
            names,
            from: Origin::Xorg,
        };
    }
    Asked {
        names: RuleNames::default(),
        from: Origin::Nowhere,
    }
}

/// Returns `true` if the kernel's own console keymap is the better source for a machine that
/// states this.
///
/// A machine that states no keyboard is one libxkbcommon compiles `us` for, and `us` is the wrong
/// keyboard everywhere outside the United States. The console keymap is read from the kernel, and
/// it is the table the terminal beside this program is already typing with — so it is right on the
/// machine it is read from, and it was measured typing German where libxkbcommon typed US.
///
/// Where a source did state a keyboard, libxkbcommon is the better one and this answers false:
/// what a console keymap gives up is in [`layout`](super::layout)'s own documentation.
pub(crate) fn reads_the_console_first(from: Origin) -> bool {
    matches!(from, Origin::Nowhere)
}

/// Returns the names a `KEY=value` file states, where it states a layout.
///
/// `/etc/vconsole.conf` and `/etc/default/keyboard` both state these four names, so one reader
/// serves both.
///
/// `rules` is left unset, because no file here names a rules file and libxkbcommon's own answer for
/// it — `evdev` — is the one every Linux machine uses.
fn xkb_variables(text: &str) -> Option<RuleNames> {
    let layout = variable(text, "XKBLAYOUT")?;
    Some(RuleNames {
        rules: None,
        model: variable(text, "XKBMODEL").map(str::to_owned),
        layout: Some(layout.to_owned()),
        variant: variable(text, "XKBVARIANT").map(str::to_owned),
        options: variable(text, "XKBOPTIONS").map(str::to_owned),
    })
}

/// Returns the names `/etc/X11/xorg.conf.d/00-keyboard.conf` states, where it states a layout.
fn xorg(text: &str) -> Option<RuleNames> {
    let layout = option(text, "XkbLayout")?;
    Some(RuleNames {
        rules: None,
        model: option(text, "XkbModel").map(str::to_owned),
        layout: Some(layout.to_owned()),
        variant: option(text, "XkbVariant").map(str::to_owned),
        options: option(text, "XkbOptions").map(str::to_owned),
    })
}

/// Returns what `key` holds in a `KEY=value` file.
///
/// The last statement of a name wins, which is how systemd reads these files, and a name stated as
/// nothing is a name the file leaves unset.
fn variable<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(statement)
        .filter(|(name, _)| *name == key)
        .map(|(_, value)| value)
        .next_back()
        .filter(|value| !value.is_empty())
}

/// Returns the name and the value one line of a `KEY=value` file states.
///
/// A line that begins with `#` is a comment, and so is the tail of an unquoted value. A quoted
/// value arrives without its quotes and keeps every `#` inside them. A quote nothing closes states
/// nothing at all, because what such a line means is the one thing this cannot work out.
fn statement(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (name, value) = line.split_once('=')?;
    Some((name.trim(), unquoted(value.trim())?))
}

/// Returns the value with its quotes taken off and the comment after it dropped.
fn unquoted(value: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            return rest.split_once(quote).map(|(inside, _)| inside);
        }
    }
    Some(value.split('#').next().unwrap_or(value).trim_end())
}

/// Returns what an `Option "XkbLayout" "de"` line states for `name`.
///
/// The name is matched whatever its case, which is how an X server reads one. The last statement
/// wins, for the same reason it does in a `KEY=value` file.
fn option<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines()
        .filter_map(stated_option)
        .filter(|(stated, _)| stated.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
        .next_back()
        .filter(|value| !value.is_empty())
}

/// Returns the name and the value one `Option` line states.
///
/// Two quoted words after the keyword, with whitespace between them and nothing else. A line
/// missing either quoted word states nothing, and so does a line whose first word merely begins
/// with `Option`.
fn stated_option(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    if !line.get(..OPTION.len())?.eq_ignore_ascii_case(OPTION) {
        return None;
    }
    let mut quoted = line[OPTION.len()..].split('"');
    if !is_a_gap(quoted.next()) {
        return None;
    }
    let name = quoted.next()?;
    if !is_a_gap(quoted.next()) {
        return None;
    }
    let value = quoted.next()?;
    // What follows the value's closing quote, which is how a quote nothing closed is refused.
    quoted.next()?;
    Some((name, value))
}

/// Returns `true` if what sits between two quoted words is whitespace and nothing else.
fn is_a_gap(between: Option<&str>) -> bool {
    between.is_some_and(|between| between.trim().is_empty())
}

#[cfg(test)]
mod tests {
    //! The cascade, over files written here.
    //!
    //! Nothing is read from the machine these run on: which keyboard a machine is set to is not
    //! something a test may choose, and a test that read the real files would assert whatever the
    //! machine it ran on happens to say.

    use super::{Asked, Machine, Origin, of, reads_the_console_first};
    use zgui_xkb::RuleNames;

    /// What `/etc/vconsole.conf` holds on the machine this cascade was written for.
    ///
    /// It states which keymap the kernel's console driver loads and no xkb name at all, which is
    /// the ordinary shape of this file.
    const VCONSOLE_HERE: &str = "KEYMAP=de-latin1\n";

    /// What `/etc/default/keyboard` holds on a Debian machine.
    ///
    /// Written by `dpkg-reconfigure keyboard-configuration`, down to the header, the blank lines
    /// and `BACKSPACE` — which nothing here reads, because it says what the backspace key sends
    /// rather than which keyboard this is.
    const DEBIAN_HERE: &str = "# KEYBOARD CONFIGURATION FILE\n\
        \n\
        # Consult the keyboard(5) manual page.\n\
        \n\
        XKBMODEL=\"pc105\"\n\
        XKBLAYOUT=\"de\"\n\
        XKBVARIANT=\"nodeadkeys\"\n\
        XKBOPTIONS=\"terminate:ctrl_alt_bksp\"\n\
        \n\
        BACKSPACE=\"guess\"\n";

    /// What `/etc/X11/xorg.conf.d/00-keyboard.conf` holds on that machine.
    ///
    /// Written by `systemd-localed`, down to the two-space indent and the line of trailing
    /// whitespace before `EndSection`.
    const XORG_HERE: &str = "Section \"InputClass\"\n  \
        Identifier \"Keyboard catchall\"\n  \
        MatchIsKeyboard \"on\"\n  \
        Option \"XkbModel\" \"pc104\"\n  \
        Option \"XkbLayout\" \"de\"\n  \
        Option \"XkbOptions\" \"terminate:ctrl_alt_bksp\"\n  \n\
        EndSection\n";

    /// A machine whose `/etc/vconsole.conf` holds `text`.
    fn vconsole(text: &str) -> Machine {
        Machine {
            vconsole: Some(text.to_owned()),
            ..Machine::default()
        }
    }

    /// A machine whose `/etc/default/keyboard` holds `text`.
    fn debian(text: &str) -> Machine {
        Machine {
            debian: Some(text.to_owned()),
            ..Machine::default()
        }
    }

    /// An xorg keyboard file stating `layout` and nothing else.
    fn xorg_stating(layout: &str) -> String {
        format!("Section \"InputClass\"\n  Option \"XkbLayout\" \"{layout}\"\nEndSection\n")
    }

    /// The layout an answer states.
    fn layout(asked: &Asked) -> Option<&str> {
        asked.names.layout.as_deref()
    }

    #[test]
    fn the_environment_answers_before_every_file() {
        // libxkbcommon's own convention, and what a session manager sets. A machine that states a
        // layout there is a machine somebody has already answered this question for.
        let machine = Machine {
            environment: Some("fr".to_owned()),
            vconsole: Some("XKBLAYOUT=gb\n".to_owned()),
            debian: Some(DEBIAN_HERE.to_owned()),
            xorg: Some(xorg_stating("de")),
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Environment);
        // And nothing was read out of it. Empty names are how libxkbcommon is told to read all
        // five variables itself, so `XKB_DEFAULT_OPTIONS` — which nothing here looks at — still
        // reaches the keymap.
        assert_eq!(
            asked.names,
            RuleNames::default(),
            "the environment is answered by leaving every name to the library"
        );
    }

    #[test]
    fn a_variable_that_states_nothing_is_no_answer() {
        // A session that exported the name and left it empty. Reading that as an answer would put
        // the machine back on `us` with its own files sitting right there.
        let machine = Machine {
            environment: Some(String::new()),
            vconsole: Some("XKBLAYOUT=gb\n".to_owned()),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::VirtualConsole);
        assert_eq!(layout(&asked), Some("gb"));
    }

    #[test]
    fn the_virtual_console_answers_before_the_xorg_file() {
        // systemd's canonical place wins over the file `localectl` writes for an X server, because
        // the first is what the machine is set to and the second is what its display server is.
        let machine = Machine {
            vconsole: Some("XKBLAYOUT=gb\n".to_owned()),
            xorg: Some(xorg_stating("de")),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::VirtualConsole);
        assert_eq!(layout(&asked), Some("gb"));
    }

    #[test]
    fn a_debian_machine_states_its_keyboard_where_debian_keeps_it() {
        // `/etc/default/keyboard` as `dpkg-reconfigure keyboard-configuration` writes it, and
        // nothing else on the machine. That is the ordinary Debian and Ubuntu desktop: no
        // `XKB_DEFAULT_*`, no `/etc/vconsole.conf`, and every xkb name in this one file.
        let asked = of(&debian(DEBIAN_HERE));

        assert_eq!(asked.from, Origin::Debian);
        assert_eq!(
            asked.names,
            RuleNames {
                rules: None,
                model: Some("pc105".to_owned()),
                layout: Some("de".to_owned()),
                variant: Some("nodeadkeys".to_owned()),
                options: Some("terminate:ctrl_alt_bksp".to_owned()),
            },
            "each of the four names reaches the field it belongs to, and `BACKSPACE` reaches none"
        );
    }

    #[test]
    fn the_debian_file_answers_before_the_xorg_file() {
        // The file a person edits wins over the one a tool wrote. A Debian machine whose settings
        // were changed once through `localectl` and again through `dpkg-reconfigure` holds a
        // `00-keyboard.conf` from the first, and the keyboard it has is what the second states.
        let machine = Machine {
            debian: Some(DEBIAN_HERE.to_owned()),
            xorg: Some(xorg_stating("gb")),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Debian);
        assert_eq!(layout(&asked), Some("de"));
    }

    #[test]
    fn the_virtual_console_answers_before_the_debian_file() {
        // One machine rarely holds both. The one that does is a Debian machine somebody ran
        // `localectl` on, because `localectl` writes an `XKBLAYOUT` into `/etc/vconsole.conf`
        // there — and that is the later statement, because the installer wrote the other one.
        let machine = Machine {
            vconsole: Some("XKBLAYOUT=gb\n".to_owned()),
            debian: Some(DEBIAN_HERE.to_owned()),
            xorg: Some(xorg_stating("fr")),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::VirtualConsole);
        assert_eq!(layout(&asked), Some("gb"));
    }

    #[test]
    fn every_name_the_virtual_console_states_reaches_the_field_it_belongs_to() {
        // Four names of the same type. A pair that swapped would compile a keymap that works and
        // types the wrong letters.
        let asked = of(&vconsole(
            "XKBLAYOUT=de\nXKBMODEL=pc105\nXKBVARIANT=nodeadkeys\nXKBOPTIONS=caps:swapescape\n",
        ));

        assert_eq!(
            asked.names,
            RuleNames {
                rules: None,
                model: Some("pc105".to_owned()),
                layout: Some("de".to_owned()),
                variant: Some("nodeadkeys".to_owned()),
                options: Some("caps:swapescape".to_owned()),
            }
        );
    }

    #[test]
    fn this_machines_own_files_state_german() {
        // The machine this was found on, both files as they are on it, and no `XKB_DEFAULT_*`
        // anywhere. Before the cascade this compiled a `us` keymap while the terminal beside it
        // typed German.
        let machine = Machine {
            environment: None,
            vconsole: Some(VCONSOLE_HERE.to_owned()),
            debian: None,
            xorg: Some(XORG_HERE.to_owned()),
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Xorg);
        assert_eq!(
            asked.names,
            RuleNames {
                rules: None,
                model: Some("pc104".to_owned()),
                layout: Some("de".to_owned()),
                variant: None,
                options: Some("terminate:ctrl_alt_bksp".to_owned()),
            }
        );
    }

    #[test]
    fn a_console_keymap_name_is_no_layout_name() {
        // `KEYMAP=de-latin1` names a keymap the kernel loads, and the table that turns one of those
        // into an xkb layout name is systemd's. A guess at it is how a keyboard reaches a layout
        // nobody set, so this file states no xkb name at all.
        let asked = of(&vconsole(VCONSOLE_HERE));

        assert_eq!(asked.from, Origin::Nowhere);
        assert_eq!(asked.names, RuleNames::default());
        assert!(
            !asked.to_string().contains("de-latin1"),
            "the console keymap's name reached the names: {asked}"
        );
    }

    #[test]
    fn a_machine_that_states_nothing_says_so() {
        // Why the console keymap is read first here: libxkbcommon answers this machine with `us`,
        // and the kernel answers it with the keymap the terminal is using.
        let asked = of(&Machine::default());

        assert_eq!(asked.from, Origin::Nowhere);
        assert_eq!(asked.names, RuleNames::default());
    }

    #[test]
    fn quoting_and_comments_are_read_the_way_the_file_is_written() {
        let asked = of(&vconsole(
            "# what this machine types\n\
             \n\
             KEYMAP=\"de-latin1\"\n\
             XKBLAYOUT=\"de\"   # set by localectl\n\
             XKBMODEL = 'pc105'\n\
             # XKBVARIANT = dvorak\n\
             XKBOPTIONS=grp:alt_shift_toggle  # and this one as well\n",
        ));

        assert_eq!(
            layout(&asked),
            Some("de"),
            "quoted, with a comment after it"
        );
        assert_eq!(
            asked.names.model.as_deref(),
            Some("pc105"),
            "in single quotes, with spaces around the `=`"
        );
        assert_eq!(
            asked.names.variant, None,
            "a whole line of comment states nothing"
        );
        assert_eq!(
            asked.names.options.as_deref(),
            Some("grp:alt_shift_toggle"),
            "unquoted, ending where the comment begins"
        );
    }

    #[test]
    fn a_name_stated_twice_is_what_it_was_stated_last() {
        // How systemd reads these files, and what a machine whose settings were edited twice holds.
        let asked = of(&vconsole("XKBLAYOUT=us\nXKBLAYOUT=de\n"));

        assert_eq!(layout(&asked), Some("de"));
        assert_eq!(
            layout(&of(&vconsole("XKBLAYOUT=de\nXKBLAYOUT=\n"))),
            None,
            "and a name stated last as nothing is stated as nothing"
        );
    }

    #[test]
    fn a_file_that_states_no_layout_states_no_keyboard() {
        // Options with no layout name no keyboard, so the cascade goes on and the options go with
        // it. Carrying them to the next source would compile one machine's layout with another
        // one's settings.
        let machine = Machine {
            vconsole: Some("XKBOPTIONS=caps:swapescape\n".to_owned()),
            xorg: Some(XORG_HERE.to_owned()),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Xorg);
        assert_eq!(
            asked.names.options.as_deref(),
            Some("terminate:ctrl_alt_bksp")
        );
    }

    #[test]
    fn a_layout_stated_as_nothing_is_no_answer() {
        let machine = Machine {
            vconsole: Some("XKBLAYOUT=\"\"\n".to_owned()),
            xorg: Some(xorg_stating("de")),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Xorg);
        assert_eq!(layout(&asked), Some("de"));
    }

    #[test]
    fn a_file_that_states_nothing_this_reads_is_a_file_that_answers_nothing() {
        // Every shape a file can arrive in that this cannot read: a file of something else
        // entirely, a quote nothing closes, and a word that merely begins with the keyword. Each
        // has to answer nothing rather than answer wrongly, because a layout read out of one is a
        // keyboard nobody set.
        let malformed = [
            Machine {
                vconsole: Some("\u{0}\u{1}\u{2} not a settings file at all".to_owned()),
                ..Machine::default()
            },
            Machine {
                vconsole: Some("XKBLAYOUT=\"de\n".to_owned()),
                ..Machine::default()
            },
            Machine {
                debian: Some("XKBLAYOUT='de\n".to_owned()),
                ..Machine::default()
            },
            Machine {
                xorg: Some(
                    "Section \"InputClass\"\n  Option \"XkbLayout\" \"de\nEndSection\n".to_owned(),
                ),
                ..Machine::default()
            },
            Machine {
                xorg: Some("  Optional \"XkbLayout\" \"de\"\n".to_owned()),
                ..Machine::default()
            },
            Machine {
                xorg: Some("  Option \"XkbLayout\"\n".to_owned()),
                ..Machine::default()
            },
        ];

        for machine in malformed {
            let asked = of(&machine);
            assert_eq!(asked.from, Origin::Nowhere, "{machine:?}");
            assert_eq!(asked.names, RuleNames::default(), "{machine:?}");
        }
    }

    #[test]
    fn an_option_line_is_read_whatever_case_it_is_written_in() {
        // An X server reads a keyword and an option name whatever their case, and a file written by
        // hand is written the way its author writes.
        let machine = Machine {
            xorg: Some("OPTION \"xkblayout\" \"de\"\noption \"XKBMODEL\" \"pc104\"\n".to_owned()),
            ..Machine::default()
        };

        let asked = of(&machine);

        assert_eq!(asked.from, Origin::Xorg);
        assert_eq!(layout(&asked), Some("de"));
        assert_eq!(asked.names.model.as_deref(), Some("pc104"));
    }

    #[test]
    fn the_console_is_read_first_only_where_the_machine_states_nothing() {
        // The one thing this decides for the search above it. libxkbcommon guesses `us` where
        // nothing states a keyboard, and the kernel holds the keymap the terminal is typing with —
        // so that machine reads the kernel. A machine that states a keyboard keeps libxkbcommon,
        // which carries dead keys, caps lock and every character past Latin-1.
        assert!(reads_the_console_first(Origin::Nowhere));
        for from in [
            Origin::Environment,
            Origin::VirtualConsole,
            Origin::Debian,
            Origin::Xorg,
        ] {
            assert!(!reads_the_console_first(from), "{from:?}");
        }
    }

    #[test]
    fn the_line_a_person_reads_says_which_source_answered() {
        // The whole reason the source is a value. A machine typing the wrong letters is diagnosed
        // from a log, and a line that names the names alone leaves a reader to guess which file to
        // look in.
        let stated = of(&Machine {
            vconsole: Some(VCONSOLE_HERE.to_owned()),
            xorg: Some(XORG_HERE.to_owned()),
            ..Machine::default()
        })
        .to_string();

        assert!(
            stated.contains("/etc/X11/xorg.conf.d/00-keyboard.conf"),
            "{stated}"
        );
        assert!(stated.contains("layout=de"), "{stated}");
        assert!(
            of(&debian(DEBIAN_HERE))
                .to_string()
                .contains("/etc/default/keyboard"),
            "and a Debian machine is sent to the file Debian keeps it in"
        );
        assert!(
            of(&Machine::default())
                .to_string()
                .contains("nothing on this machine"),
            "and a machine that states none says that"
        );
    }
}
