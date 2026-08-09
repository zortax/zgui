//! Telling the kernel's console driver that something else is drawing.
//!
//! A program here takes the display through DRM and draws into its own buffers. The framebuffer
//! console knows nothing about that: it goes on writing text and a blinking cursor into the same
//! screen, and it never redraws when the program stops. Both halves of that are one ioctl —
//! `KDSETMODE` — and [`ConsoleScreen`] is where this backend issues it.
//!
//! # What the two calls are for
//!
//! **While the program runs**, the console driver may draw over the picture. A login prompt printed
//! on another terminal, a kernel message, or the cursor blinking on the line the program was
//! started from all reach the same framebuffer.
//!
//! **When the program stops**, the screen stays dark. Handing DRM master back does not put the
//! kernel's own picture up: `drm_drop_master` releases the master and restores nothing, and the
//! restore the kernel does have runs when the *last* handle on the device closes. The console holds
//! the text it had and repaints from it, re-applying its own display mode, when it is told the
//! screen is its own again, and nothing else tells it.
//!
//! # Where this backend stops
//!
//! At the mode. Switching away from a program that holds the display needs `VT_SETMODE` with
//! `VT_PROCESS`, a pair of signals, DRM master handed back and taken again around each switch, and
//! on an ordinary desktop a session daemon that owns the devices. This backend has none of that, so
//! `Ctrl+Alt+F2` while a program on it runs leaves it holding the display. [`zgui_evdev::Screen`]
//! states the same boundary from the other side.

use tracing::{info, warn};
use zgui_evdev::{Console, Screen};

/// The console's screen, taken for graphics for as long as this lives.
///
/// It holds one console open across both calls. A second open could answer differently from the
/// first — a terminal that was hung up, a path that became unreadable — and a screen left in
/// graphics mode stays dark until something redraws it.
///
/// A machine where no console answered holds nothing and does nothing. That is the ordinary
/// answer under a terminal emulator and over a network connection, where the process's own
/// terminal is a pseudo-terminal and the virtual consoles belong to root; a program run there is
/// drawing on a screen whose console it may not speak for.
#[derive(Debug)]
pub struct ConsoleScreen {
    /// The console that answered, and that owes a text mode.
    ///
    /// `None` where none answered, or where the one that did refused the graphics mode. Both mean
    /// the same thing on the way out: there is nothing to give back.
    console: Option<Console>,
}

impl ConsoleScreen {
    /// Takes the screen away from the console's text.
    ///
    /// **Call this after DRM master has been taken.** A run on a machine where a compositor holds
    /// the device fails at the master and returns, so it never blanks a console it was not going to
    /// draw on. [`Seat::open`](crate::input::seat::Seat::open) keeps the same ordering for the
    /// grab, for the same reason.
    ///
    /// Nothing here fails. A console that cannot be found or will not take the mode leaves the
    /// program drawing under a console driver that is still drawing too. The refusal is reported
    /// through the crate's log.
    pub fn taken() -> Self {
        let found = Console::find();
        let Some(console) = found.console else {
            for refusal in &found.refused {
                info!(target: "zgui::platform", "no console at {}: {}", refusal.path.display(), refusal.reason);
            }
            warn!(
                target: "zgui::platform",
                "no console on this machine answered, so the kernel's own text console is not told \
                 that something else is drawing: it may write over the picture, and the screen is \
                 left as this program leaves it when it exits"
            );
            return Self { console: None };
        };
        if let Err(error) = console.set_screen(Screen::Graphics) {
            warn!(
                target: "zgui::platform",
                "{} will not be put into graphics mode, so the text console may draw over the \
                 picture and will not redraw when this program exits: {error}",
                console.path().display()
            );
            return Self { console: None };
        }
        Self {
            console: Some(console),
        }
    }

    /// Gives the screen back to the console's text.
    ///
    /// Taken by value, because the screen is the console's again afterwards and there is nothing
    /// left to give back.
    ///
    /// **Call this before DRM master is handed back.** The kernel carries an exception for that
    /// order: the framebuffer console answers the text mode by committing its own modeset, and
    /// `drm_fb_helper_set_par` forces that commit while another client is still master. The
    /// kernel's own comment names Xorg, which sets the terminal back to text mode and drops master
    /// after it. So this one call puts the picture back. [`zgui_evdev::Screen`] states the
    /// mechanism.
    ///
    /// A machine where the graphics mode was never taken restores nothing. There is no console
    /// held, so there is nothing to give back.
    ///
    /// A refusal is reported through the log. This runs while a program is shutting down, there is
    /// nothing a caller could do about it, and a return value here is one every call site would
    /// discard.
    pub fn restore(self) {
        let Some(console) = self.console else {
            return;
        };
        if let Err(error) = console.set_screen(Screen::Text) {
            warn!(
                target: "zgui::platform",
                "{} could not be put back into text mode, so this console stays dark until \
                 something redraws it: {error}",
                console.path().display()
            );
        }
    }
}
