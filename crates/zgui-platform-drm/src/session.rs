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
//! **Seated.** libseat opened a seat and the seat enabled. [`Session::card`] and
//! [`Session::open_input`] ask the daemon for the card and for each input device, and the console
//! is already in graphics mode. logind and seatd set DRM master on a card before they answer the
//! client, so a card from either arrives with master on it; libseat's noop backend opens the path
//! with a plain `open(2)` and grants none of it.
//!
//! **Direct.** This process opens the card and each input device, takes master itself, and puts the
//! console into graphics mode. It is the answer where libseat is absent, where the seat was
//! refused, and where a seat opened and never enabled. This path needs root or a free virtual
//! terminal, and it is allowed to be worse.
//!
//! **The fallback is free only where libseat is absent.** A machine that has the library and a seat
//! that never enables pays [`zgui_seat::ENABLE_WITHIN`] waiting for an enable that is never coming.
//! A seat logind granted holds the terminal for that whole wait — `K_OFF` and `KD_GRAPHICS`, so the
//! console keyboard there stops answering — and gives it back when the seat closes.
//!
//! # What `Drop` gives back
//!
//! A session holds the console's screen, the DRM master a direct run took, the card, and every
//! device the seat opened. [`Session`]'s own `Drop` gives all four back, in the order the console
//! driver and the kernel need. So the session is held for as long as anything that came out of it
//! is used: one dropped while the card is still being drawn on restores the console and hands the
//! master back under a live descriptor.
//!
//! # Switching away is not handled yet
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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};
use zgui_platform::PlatformError;

use crate::console::ConsoleScreen;
use crate::output::backend;

/// Where the devices come from, for one run.
///
/// [`Session::card`] answers the display device this backend drives, and the session keeps what
/// that card cost. A seated session also keeps every device the seat opened, so the session is held
/// for as long as anything that came out of it is used.
///
/// ```no_run
/// use std::sync::Arc;
///
/// use zgui_platform_drm::Session;
///
/// let mut session = Session::open();
/// let card = session.card()?;
///
/// assert!(
///     Arc::ptr_eq(&card, &session.card()?),
///     "a session drives one card, and every later call answers the one it took"
/// );
/// # Ok::<(), zgui_platform::PlatformError>(())
/// ```
#[derive(Debug)]
pub struct Session {
    /// Which of the two shapes this run got.
    shape: Shape,
    /// What the card cost, once one has been asked for.
    took: Option<Taken>,
}

/// The two shapes, and what each one holds.
#[derive(Debug)]
enum Shape {
    /// A session daemon owns the devices and hands each one over.
    Seated {
        /// The seat every device is asked for, and given back to.
        seat: zgui_seat::Seat,
        /// Every device the seat opened for the card.
        ///
        /// A [`zgui_seat::Device`] owns the descriptor libseat wrote, so it has to outlive
        /// everything built over that descriptor. Holding it here keeps it alive, and it is why
        /// [`Session::card`] takes `&mut self`.
        held: Vec<zgui_seat::Device>,
        /// Every device the seat opened for input, under the path it was opened at.
        ///
        /// Apart from the card, because the two have different lives. The card is taken once and
        /// kept for the whole run; an input device goes back on its own when this session lets go
        /// of it, and all of them go back together when the terminal does.
        ///
        /// The path is what names one of them, because that is what a caller holds:
        /// [`zgui_seat::Device`] carries libseat's id, which is the seat's own numbering and
        /// reaches nothing outside `zgui-seat`.
        inputs: Vec<(PathBuf, zgui_seat::Device)>,
    },
    /// This process opens the devices itself, and takes what they need.
    Direct,
}

/// One card, and everything taken along with it.
///
/// `Drop` reads which of the two constructors built this, so what is given back is what was taken.
/// The shape decides which one runs, and it decides it once.
#[derive(Debug)]
struct Taken {
    /// The card this backend drives.
    ///
    /// Kept here so that the master is handed back through the descriptor it was taken on, and so
    /// that a seated run's duplicate closes before the seat's own device goes back.
    card: Arc<zgui_drm::Device>,
    /// Whether this process took DRM master, and therefore owes it back.
    master: bool,
    /// The console's screen, where this process put it into graphics mode.
    screen: Option<ConsoleScreen>,
}

impl Taken {
    /// Returns what a card from a session daemon costs: the descriptor, and nothing else.
    ///
    /// The master is the daemon's. It sits on the open file description the daemon granted it on,
    /// and this run holds a duplicate of that description, so a `DROP_MASTER` here would take the
    /// master away from the daemon's own descriptor as well.
    ///
    /// The console is the daemon's too: it puts the terminal into `KD_GRAPHICS` when it grants
    /// control, and a [`ConsoleScreen::taken`] here would look for a console this run may not open
    /// and report a warning that is false.
    fn seated(card: zgui_drm::Device) -> Self {
        Self {
            card: Arc::new(card),
            master: false,
            screen: None,
        }
    }

    /// Returns what a card this process opened costs: the master it took, and the console it
    /// blanked.
    ///
    /// The console is taken here, which is **after** the master. A run on a machine where a
    /// compositor holds the device has already failed at `become_master`, so it never blanks a
    /// console it was not going to draw on. [`ConsoleScreen::taken`] states the same ordering from
    /// its own side.
    fn direct(card: zgui_drm::Device) -> Self {
        Self {
            card: Arc::new(card),
            master: true,
            screen: Some(ConsoleScreen::taken()),
        }
    }
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
    /// then hands back a seat that never enables. A run started on a terminal nobody is looking at
    /// lands there as well: logind reads whether the session is active before it grants control and
    /// reports an inactive one as disabled, so that seat never enables either. Both fail at DRM
    /// master in [`Session::card`], which is the interlock this backend has always had.
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
                        inputs: Vec::new(),
                    },
                    took: None,
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
                    took: None,
                }
            }
        }
    }

    /// Returns the display device this backend drives.
    ///
    /// The cards [`zgui_drm::cards`] lists are walked in turn. See [`Session::card_from`] for what
    /// each shape does with that list, and for what one card costs.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when `/dev/dri` cannot be read, when no card could be
    /// opened, and, on the direct path, when this process cannot become DRM master.
    pub fn card(&mut self) -> Result<Arc<zgui_drm::Device>, PlatformError> {
        let cards = zgui_drm::cards().map_err(backend)?;

        self.card_from(&cards)
    }

    /// Returns the display device this backend drives, out of `cards`.
    ///
    /// [`Session::card`] is this over the list [`zgui_drm::cards`] answers, and that is the list a
    /// run walks. A caller that names its own list decides the order and what is in it, so a test
    /// can put a path that is not a card in front of the real ones — a seat hands out input devices
    /// over the call it hands out cards with, so the walk meets that shape on a real machine.
    ///
    /// **Seated.** Each path is asked of the seat in turn, and the first the seat opens is the one
    /// that is used. The device is built over a duplicate of the descriptor the seat handed over,
    /// and this session keeps the seat's own device.
    ///
    /// **Direct.** The first path that opens, and DRM master taken on it.
    ///
    /// # Master on the seated path
    ///
    /// Nothing here asks for it. logind and seatd each set DRM master on the card themselves before
    /// they answer, so a session that holds the terminal has master as soon as this returns.
    ///
    /// A card can still arrive without it, two ways, and both are logind's. A session that does not
    /// hold the terminal is handed the card with master dropped. And a session that does hold it
    /// gets the same treatment when `SET_MASTER` answers `EINVAL`, which logind reads as another
    /// master already being active and retries as though the session were inactive. Either way the
    /// daemon sets master when the terminal moves to this session, and the loop reads that as the
    /// resume.
    ///
    /// # One card, and one answer
    ///
    /// The first call takes the card and every later one answers what it took. So a session drives
    /// one card, and a second call asks the daemon for nothing. logind refuses a device the session
    /// already holds, and a walk that asked twice would leave a device to give back for every call.
    ///
    /// # One card, three names for one open file description
    ///
    /// `F_DUPFD_CLOEXEC` makes a second descriptor onto **one open file description**. The kernel
    /// records DRM master and the client capabilities per open file description, on the `drm_file`
    /// it made for it, so a duplicate carries both. logind's own descriptor is a third name for the
    /// same description, because a descriptor sent over a socket names the description itself. So
    /// the capabilities [`zgui_drm::Device::over`] sets on the duplicate and the master logind
    /// granted on its own are one state seen through three names.
    ///
    /// # A seated session that gets no card
    ///
    /// [`Session::open`] falls back to the direct shape on every failure, and this falls back on
    /// none. A seat that opened is a machine whose daemon owns the devices: it holds the terminal,
    /// it decides which of them this session may have, and a run that then opened a card behind it
    /// would need the privilege the seat exists to spare it. So every reason the walk collected is
    /// reported and the run stops.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when no card could be opened, and, on the direct path,
    /// when this process cannot become DRM master — a compositor holding the device looks like
    /// that.
    pub fn card_from(&mut self, cards: &[PathBuf]) -> Result<Arc<zgui_drm::Device>, PlatformError> {
        if let Some(taken) = &self.took {
            return Ok(Arc::clone(&taken.card));
        }

        let taken = match &mut self.shape {
            Shape::Seated { seat, held, .. } => Taken::seated(seated_card(seat, held, cards)?),
            Shape::Direct => Taken::direct(direct_card(cards)?),
        };
        let card = Arc::clone(&taken.card);
        self.took = Some(taken);

        Ok(card)
    }

    /// Opens the input device at `path`.
    ///
    /// **Seated.** The seat opens the node, and the device is built over a duplicate of the
    /// descriptor it handed over, exactly as the card is. The seat's own device is kept by this
    /// session, because [`Session::close_input`] is the only thing that may give it back.
    ///
    /// **Direct.** This process opens the node itself, which needs the node's own group.
    ///
    /// Every refusal names the path it was asked for, because a caller opens these one at a time
    /// and reports each on its own.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the node cannot be opened, and when the descriptor
    /// that came back names something other than an evdev node. Neither is unusual: on the seated
    /// path a daemon decides which devices this session may have, and on the direct path most
    /// nodes belong to a group.
    pub fn open_input(&mut self, path: &Path) -> Result<zgui_evdev::Device, PlatformError> {
        match &mut self.shape {
            Shape::Seated { seat, inputs, .. } => seated_input(seat, inputs, path),
            Shape::Direct => zgui_evdev::Device::open(path).map_err(|error| backend_error(&error)),
        }
    }

    /// Gives the input device at `path` back to the seat.
    ///
    /// A caller that lets go of a device calls this, because dropping the [`zgui_evdev::Device`]
    /// closes a descriptor and tells the daemon nothing: its record of the device stands until the
    /// seat closes, and a second open of the same path meets it. So a node this seat declined, and
    /// a device that stopped answering, come back here.
    ///
    /// A path this session holds no device at is nothing to answer for, which is every path on the
    /// direct shape.
    ///
    /// # Dropping the caller's own device first
    ///
    /// A [`zgui_evdev::Device`] built by [`Session::open_input`] holds a duplicate of the seat's
    /// descriptor, so the two name one open file description. A caller that still held one here
    /// would keep that description — and the grab on it — alive after the daemon released the
    /// device, and the next open of the same node would then be refused the grab.
    pub fn close_input(&mut self, path: &Path) {
        let Shape::Seated { seat, inputs, .. } = &mut self.shape else {
            return;
        };
        let Some(at) = inputs.iter().position(|(held, _)| held == path) else {
            return;
        };
        let (_, device) = inputs.remove(at);
        give_back(seat, device);
    }

    /// Gives every input device this session opened back to the seat.
    ///
    /// All of them at once, because that is what a session that loses its terminal has to do.
    /// `EVIOCREVOKE` cannot be undone, so a descriptor another session took is dead: what comes back
    /// is a set of devices opened again.
    ///
    /// The same rule [`Session::close_input`] states holds here: the caller drops its own devices
    /// first.
    pub fn close_every_input(&mut self) {
        let Shape::Seated { seat, inputs, .. } = &mut self.shape else {
            return;
        };
        for (_, device) in std::mem::take(inputs) {
            give_back(seat, device);
        }
    }

    /// Returns `true` if the devices come from a session daemon.
    ///
    /// [`Session::open`] answered this, and it decides everything else: where a card comes from,
    /// whether the console and the master are this run's to take, and therefore what `Drop` gives
    /// back.
    pub fn is_seated(&self) -> bool {
        matches!(self.shape, Shape::Seated { .. })
    }

    /// Returns the direct shape, asked for by name.
    ///
    /// [`Session::open`] reads the machine, and on the seated path that takes the terminal. A unit
    /// test needs neither, so it is given this: the shape a machine with no libseat answers, built
    /// without asking libseat anything.
    #[cfg(test)]
    pub(crate) fn direct() -> Self {
        Self {
            shape: Shape::Direct,
            took: None,
        }
    }
}

/// Gives back everything this run took, in the order the machine needs it back in.
///
/// **The console first.** Telling it the screen is its own is what puts the kernel's own picture
/// back — handing the device over does not — and it is the order Xorg established and the kernel
/// carries an exception for.
///
/// **Then the master**, and only if this process took it. A process that kept master
/// would leave the console with no way to draw for anybody else until it exits.
///
/// **Then the card.** A caller that has let go of its own name on it closes the descriptor here,
/// which is while the seat's own device is still open.
///
/// **Then every device** this session still holds — the card's, and every input device a caller did
/// not already give back through [`Session::close_input`] — through the seat that opened it.
/// `libseat_close_seat` releases the devices as well — logind's `ReleaseControl` frees every
/// session device it holds, closes its own descriptor onto each and restores the terminal — so this
/// loop is what releases each device at a moment this session chooses, over the same route a
/// session that stays open gives a device back on. What survives either way is this process's own
/// descriptor, which goes when the device goes. The seat itself is closed by its own `Drop` after
/// this body.
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(taken) = self.took.take() {
            if let Some(screen) = taken.screen {
                screen.restore();
            }
            if taken.master
                && let Err(error) = taken.card.drop_master()
            {
                warn!(
                    target: "zgui::platform",
                    "the device could not be handed back before this process exits: {error}"
                );
            }
            drop(taken.card);
        }

        let Shape::Seated { seat, held, inputs } = &mut self.shape else {
            return;
        };
        let inputs = std::mem::take(inputs).into_iter().map(|(_, device)| device);
        for device in std::mem::take(held).into_iter().chain(inputs) {
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
    cards: &[PathBuf],
) -> Result<zgui_drm::Device, PlatformError> {
    let mut refused = Vec::new();

    for path in cards {
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

    Err(opened_nothing("the seat", &refused))
}

/// Asks the seat for one input device, and keeps the device it answered.
///
/// The same handover the card gets, and for the same reason: the seat lends its descriptor and
/// surrenders it to nobody, so what `zgui-evdev` is given is a duplicate and the seat's own device
/// stays with the session. Both name one open file description, so the grab this process takes on
/// the duplicate is the grab the daemon's descriptor carries.
///
/// A node the seat opened and `zgui-evdev` refused goes straight back. A seat hands out graphics
/// cards over the same call, and a backend that opens any path it is given hands out anything else
/// as well.
fn seated_input(
    seat: &zgui_seat::Seat,
    inputs: &mut Vec<(PathBuf, zgui_seat::Device)>,
    path: &Path,
) -> Result<zgui_evdev::Device, PlatformError> {
    let device = seat
        .open_device(path)
        .map_err(|error| backend_error(&error))?;

    let duplicate = match device.descriptor().try_clone_to_owned() {
        Ok(duplicate) => duplicate,
        Err(error) => {
            give_back(seat, device);
            return Err(PlatformError::Backend(format!(
                "the descriptor the seat opened {} on cannot be copied: {error}",
                path.display()
            )));
        }
    };

    match zgui_evdev::Device::over(duplicate, path) {
        Ok(input) => {
            inputs.push((path.to_owned(), device));
            Ok(input)
        }
        Err(error) => {
            give_back(seat, device);
            Err(backend_error(&error))
        }
    }
}

/// Returns what a device that refused reads as, for a caller above this backend.
///
/// The message is the refusal's own. Every one of them names the path it was asked for, so nothing
/// here adds it a second time.
fn backend_error(error: &dyn std::fmt::Display) -> PlatformError {
    PlatformError::Backend(error.to_string())
}

/// Opens the first card this process can, and takes DRM master on it.
///
/// A card that opens and refuses the master ends the walk. That refusal is what a compositor
/// holding the device looks like, and stopping there is the interlock this backend has always had:
/// a run that gets no further takes neither the console nor the keyboard.
fn direct_card(cards: &[PathBuf]) -> Result<zgui_drm::Device, PlatformError> {
    let mut refused = Vec::new();

    for path in cards {
        match zgui_drm::Device::open(path) {
            Ok(card) => {
                card.become_master().map_err(|error| {
                    PlatformError::Backend(format!(
                        "this process opened {} and could not take DRM master on it, which is what \
                         another process holding the display looks like: {error}",
                        path.display()
                    ))
                })?;
                return Ok(card);
            }
            Err(error) => refused.push(error.to_string()),
        }
    }

    Err(opened_nothing("this process", &refused))
}

/// Returns what a walk that opened no card reports.
///
/// A machine that lists no card and a machine whose cards all refused are two different machines,
/// and `who` names which side of the handover asked. Every refusal is carried, because the first
/// card is rarely the interesting one.
fn opened_nothing(who: &str, refused: &[String]) -> PlatformError {
    PlatformError::Backend(if refused.is_empty() {
        format!("{who} has no display device to open: this machine lists no `card*` under /dev/dri")
    } else {
        format!(
            "{who} opened no display device on this machine: {}",
            refused.join("; ")
        )
    })
}

/// Gives one device back to the seat that opened it.
///
/// `libseat_close_device` releases the daemon's record of the device while the seat stays open.
/// Dropping the device closes the descriptor and leaves that record standing until the seat closes,
/// so the way out is through the seat.
///
/// A refusal is reported through the log. The descriptor goes back either way, and there is nothing
/// a caller could do about the record.
fn give_back(seat: &zgui_seat::Seat, device: zgui_seat::Device) {
    if let Err(error) = seat.close_device(device) {
        warn!(
            target: "zgui::platform",
            "a device could not be given back to the seat, so the session daemon holds its record \
             of it until the seat closes: {error}"
        );
    }
}
