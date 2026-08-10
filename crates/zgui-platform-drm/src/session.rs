//! Where a run's devices come from: a session daemon, or this process itself.
//!
//! A program that draws on a console opens a graphics card, and the card belongs to the session
//! that owns the screen. A program that opens it itself therefore needs root, or a virtual terminal
//! reserved in advance. libseat asks the session daemon for it instead: the daemon opens the card
//! and hands the descriptor over, and a program that asks this way runs from an ordinary login
//! shell.
//!
//! [`Session::open`] answers one of two shapes, and it never fails.
//!
//! **Seated.** libseat opened a seat and the seat enabled. [`Session::card`] asks the daemon for
//! the card, and the console is already in graphics mode. logind sets DRM master on a card before
//! it answers the client, so a card from it arrives with master on it; libseat's noop backend opens
//! the path with a plain `open(2)` and grants none of it.
//!
//! **Direct.** This process opens the card and takes master itself, as this backend did before this
//! module existed. It is the answer where libseat is absent, where the seat was refused, and where
//! a seat opened and never enabled. This path needs root or a free virtual terminal, and it is
//! allowed to be worse.
//!
//! # Switching away
//!
//! A seated run takes the seat and reads nothing back from it. [`zgui_seat::Seat::dispatch`] is
//! called by nothing here, so a session that loses its devices to another terminal is a session
//! that carries on drawing: the commits fail, the input descriptors answer `ENODEV`, and the
//! application is told none of it. The recovery is a later milestone.
//!
//! Taking the seat also takes the terminal. logind puts the terminal into `K_OFF` and
//! `KD_GRAPHICS` when it grants control, so the console keyboard stops answering and `Ctrl+Alt+F2`
//! stops switching for as long as the seat is held. logind gives the terminal back when the
//! controlling process **exits**, so a seated program that stops answering leaves a machine that
//! draws nothing and answers no key until it is killed from elsewhere.

use tracing::{info, warn};
use zgui_platform::PlatformError;

use crate::output::backend;

/// Where the devices come from, for one run.
///
/// [`Session::card`] answers the display device this backend drives. A seated session keeps every
/// device the seat opened for it, so the session is held for as long as anything that came out of
/// it is used.
#[derive(Debug)]
pub struct Session {
    /// Which of the two shapes this run got.
    shape: Shape,
}

/// The two shapes, and what each one holds.
#[derive(Debug)]
enum Shape {
    /// A session daemon owns the devices and hands each one over.
    Seated {
        /// The seat every device is asked for, and given back to.
        seat: zgui_seat::Seat,
        /// Every device the seat opened for this session.
        ///
        /// A [`zgui_seat::Device`] owns the descriptor libseat wrote, so it has to outlive
        /// everything built over that descriptor. Holding it here keeps it alive, and it is why
        /// [`Session::card`] takes `&mut self`.
        held: Vec<zgui_seat::Device>,
    },
    /// This process opens the devices itself, and takes what they need.
    Direct,
}

impl Session {
    /// Opens the session this run is in.
    ///
    /// libseat is tried first. Every failure — no library, a seat that was refused, a seat that
    /// opened and never enabled — answers the direct shape, and one line at the crate's log says
    /// which shape this run got and why.
    ///
    /// A run started inside a desktop's own session lands on the direct shape: logind refuses
    /// control of a session that already has a controlling client, and libseat's builtin backend
    /// then hands back a seat that never enables. Such a run fails at DRM master in
    /// [`Session::card`], which is the interlock this backend has always had.
    pub fn open() -> Self {
        match zgui_seat::Seat::open() {
            Ok(seat) => {
                info!(
                    target: "zgui::platform",
                    "the devices come from the session daemon, on seat {}, so this run needs no \
                     privilege of its own",
                    seat.name()
                );
                Self {
                    shape: Shape::Seated {
                        seat,
                        held: Vec::new(),
                    },
                }
            }
            Err(error) => {
                warn!(
                    target: "zgui::platform",
                    "this run opens the devices itself, so it needs root or a virtual terminal \
                     nothing else holds, and a terminal switch leaves it holding the display: \
                     {error}"
                );
                Self {
                    shape: Shape::Direct,
                }
            }
        }
    }

    /// The display device this backend drives.
    ///
    /// **Seated.** The `card*` devices [`zgui_drm::cards`] lists are asked for in turn, and the
    /// first the seat opens is the one that is used. The device is built over a duplicate of the
    /// descriptor the seat handed over, and the seat's own device is held by this session. Master
    /// is asked for by nothing: logind sets it on the card before it answers the client, so it is
    /// already held when this returns.
    ///
    /// **Direct.** The first card that opens, and DRM master taken on it.
    ///
    /// # One card, three names for one open file description
    ///
    /// `F_DUPFD_CLOEXEC` makes a second descriptor onto **one open file description**, and the
    /// kernel records DRM master and the client capabilities on that description. logind's own
    /// descriptor is already a third name for it, because a descriptor sent over a socket names the
    /// description rather than a copy of it. So the capabilities [`zgui_drm::Device::over`] sets on
    /// the duplicate and the master logind granted on its own are one state seen through three
    /// names.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when no card could be opened, and, on the direct path,
    /// when this process cannot become DRM master — which is what a compositor holding the device
    /// looks like.
    pub fn card(&mut self) -> Result<zgui_drm::Device, PlatformError> {
        match &mut self.shape {
            Shape::Seated { seat, held } => seated_card(seat, held),
            Shape::Direct => {
                let card = zgui_drm::Device::open_first().map_err(backend)?;
                card.become_master().map_err(backend)?;
                Ok(card)
            }
        }
    }

    /// Whether this session puts the console into graphics mode itself.
    ///
    /// True on the direct path alone. logind puts the terminal into `KD_GRAPHICS` when it grants
    /// control, so a seated run has the screen already; a [`ConsoleScreen::taken`] there would look
    /// for a console it may not open and report a warning that is false.
    ///
    /// [`ConsoleScreen::taken`]: crate::console::ConsoleScreen::taken
    pub fn takes_the_console(&self) -> bool {
        matches!(self.shape, Shape::Direct)
    }

    /// Whether this session takes DRM master itself, and therefore owes it back.
    ///
    /// True on the direct path alone, and it is the same answer [`Session::card`] acts on: what
    /// this process took is what this process gives back.
    ///
    /// A seated run gives up nothing. Master sits on the open file description logind granted it
    /// on, which this run holds a duplicate of, so a `DROP_MASTER` here would take the master away
    /// from the daemon's own descriptor as well.
    pub fn takes_the_master(&self) -> bool {
        matches!(self.shape, Shape::Direct)
    }
}

/// Gives every device back, and then closes the seat.
///
/// `libseat_close_seat` releases the devices as well, so this loop releases each one at a moment
/// this session chooses. The devices are taken out of the seated shape here, and the seat is closed
/// by its own `Drop` after this body.
impl Drop for Session {
    fn drop(&mut self) {
        let Shape::Seated { seat, held } = &mut self.shape else {
            return;
        };
        for device in std::mem::take(held) {
            give_back(seat, device);
        }
    }
}

/// Asks the seat for a card, and keeps the device it answered.
///
/// A card the seat opened and `zgui-drm` refused goes straight back, and the walk carries on. A
/// seat hands out input devices over the same call it hands out cards, so a descriptor onto
/// something that is not a card is an ordinary answer.
fn seated_card(
    seat: &zgui_seat::Seat,
    held: &mut Vec<zgui_seat::Device>,
) -> Result<zgui_drm::Device, PlatformError> {
    let cards = zgui_drm::cards().map_err(backend)?;
    let mut refused = Vec::new();

    for path in &cards {
        let device = match seat.open_device(path) {
            Ok(device) => device,
            Err(error) => {
                refused.push(error.to_string());
                continue;
            }
        };

        // The seat lends its descriptor and surrenders it to nobody, so what `zgui-drm` is given is
        // a duplicate and the seat's own device stays here. Both name one open file description.
        let duplicate = match device.descriptor().try_clone_to_owned() {
            Ok(duplicate) => duplicate,
            Err(error) => {
                give_back(seat, device);
                refused.push(format!(
                    "the descriptor the seat opened {} on cannot be copied: {error}",
                    path.display()
                ));
                continue;
            }
        };

        match zgui_drm::Device::over(duplicate, path) {
            Ok(card) => {
                info!(
                    target: "zgui::platform",
                    "the session daemon opened {}", path.display()
                );
                held.push(device);
                return Ok(card);
            }
            Err(error) => {
                give_back(seat, device);
                refused.push(error.to_string());
            }
        }
    }

    Err(PlatformError::Backend(if refused.is_empty() {
        "the seat has no display device to open: this machine lists no `card*` under /dev/dri"
            .to_owned()
    } else {
        format!(
            "the seat opened no display device on this machine: {}",
            refused.join("; ")
        )
    }))
}

/// Gives one device back to the seat that opened it.
///
/// `libseat_close_device` releases the daemon's record of the device. Dropping the device closes
/// the descriptor and leaves that record standing until the seat closes, so the way out is through
/// the seat.
///
/// A refusal is reported through the log. The descriptor goes back either way, and there is nothing
/// a caller could do about the record.
fn give_back(seat: &zgui_seat::Seat, device: zgui_seat::Device) {
    if let Err(error) = seat.close_device(device) {
        warn!(
            target: "zgui::platform",
            "a device could not be given back to the seat, so the session daemon holds its record \
             of it until this process exits: {error}"
        );
    }
}
