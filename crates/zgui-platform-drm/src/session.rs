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
//! **Seated.** libseat opened a seat, and the seat said whether it holds the terminal.
//! [`Session::card`] and [`Session::open_input`] ask the daemon for the card and for each input
//! device, and the console is already in graphics mode. logind and seatd set DRM master on a card
//! before they answer the client, so a card from either arrives with master on it; libseat's noop
//! backend opens the path with a plain `open(2)` and grants none of it.
//!
//! **Direct.** This process opens the card and each input device, takes master itself, and puts the
//! console into graphics mode. It is the answer where libseat is absent, where the seat was
//! refused, and where a seat opened and said nothing about itself. This path needs root or a free
//! virtual terminal, and it is allowed to be worse.
//!
//! **The fallback is free only where libseat is absent.** A machine that has the library and a seat
//! that says nothing pays [`zgui_seat::ENABLE_WITHIN`] waiting for an answer that is never coming.
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
//! # Switching terminal
//!
//! A seated session hands its devices over when a person switches to another terminal, and takes
//! them again on the way back. A loop waits on the seat's own descriptor beside the device,
//! `Session::dispatch` reads the changes off it, and `presence` turns a turn's worth of them into
//! the one thing to do about them. All three are this crate's own — [`run`](crate::run) drives a
//! console backend and nothing else does — so they are named here rather than linked.
//!
//! There is no window on the way out. logind moves the terminal, drops DRM master and revokes every
//! evdev descriptor **before** it reports the change, so a suspend catches up with what has already
//! happened. A resume opens every input device again, because `EVIOCREVOKE` cannot be undone, and
//! sets every mode again, because another session has put its own on the CRTC.
//!
//! A run started on a terminal that is not the live one is the same machinery from the other end.
//! The seat opens, its devices are another session's, and the loop waits: [`zgui_seat::Seat`]
//! leaves the change that said so in its queue, so the first turn reads it as an ordinary suspend
//! and the enable that arrives when a person switches to that terminal is an ordinary resume.
//!
//! Taking the seat also takes the terminal. logind puts the terminal into `K_OFF` and
//! `KD_GRAPHICS` when it grants control, so the console keyboard stops answering for as long as the
//! seat is held, and a key that asks for another terminal reaches this program rather than the
//! console driver. So this program asks: the layout reads `Ctrl+Alt+Fn` as the terminal it is, and
//! `Session::switch` carries that to the daemon. logind gives the terminal back when the
//! controlling process **exits**, so a seated program that stops answering leaves a machine that
//! draws nothing and answers no key until it is killed from elsewhere.
//!
//! **An ask is the one window this run has.** It is made while the session is still active and
//! still holds DRM master, and the suspend that follows it has neither — so anything the next
//! session would otherwise inherit is put right at the ask. [`crate::cursor::Planes`] is what that
//! means today: a cursor plane keeps whatever was last put on it, and a session that never names
//! that plane never clears it.

pub(crate) mod presence;

use std::os::fd::BorrowedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};
use zgui_platform::PlatformError;
use zgui_seat::Change;

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
    /// Everything this session recorded, in the order it was.
    ///
    /// See [`Asked`].
    #[cfg(test)]
    asked: Record,
}

/// One call a caller made on a session, recorded while the tests run.
///
/// Three of them are the input calls, and [`crate::input::seat::Seat`] is the only caller of those
/// in a running program: each one costs a device the daemon holds a record of. The fourth is the
/// terminal a key asked for, which the frame loop makes. On the direct shape three of the four do
/// nothing at all or refuse, so a test on that shape can assert what a seated run *would* be asked
/// for and in which order. Each call records itself before it reads the shape, so what a caller
/// asked for is visible with no seat, no daemon and no terminal.
///
/// [`Asked::Refused`] records a decision instead of a call, because that decision is otherwise a
/// line in a log nobody reads.
///
/// The last three are made **for** a session. See [`Record`].
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// [`Session::open_input`], with the path it was asked for.
    Open(PathBuf),
    /// [`Session::close_input`], with the path it was asked for.
    Close(PathBuf),
    /// [`Session::close_every_input`].
    CloseEvery,
    /// [`Session::switch`], with the terminal it was asked for.
    Switch(u32),
    /// [`Session::switch`] on the direct shape, the first time it refused one.
    ///
    /// Once for the whole run, because the refusal is reported once. Every later ask records its
    /// own [`Asked::Switch`] and nothing else, and that absence says the state was kept.
    Refused,
    /// Every cursor plane taken back, before the ask.
    ///
    /// See [`crate::cursor::Planes::give_them_back`].
    GavePlanesBack,
    /// Every cursor plane taken again, after an ask that was refused.
    ///
    /// See [`crate::cursor::Planes::take_them_again`].
    TookPlanesAgain,
}

/// The list [`Asked`] is recorded on, shared with whatever else a switch runs.
///
/// A switch is two things in one order: every cursor plane goes back, and then the terminal is
/// asked for. The planes belong to the card and the code that hides them holds no session, so the
/// two are recorded by different callers. A test that read two lists could say that both happened
/// and nothing about which came first. One shared list is one order.
///
/// [`Session::recording`] is how a test hands the same list to the other caller.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct Record(std::rc::Rc<std::cell::RefCell<Vec<Asked>>>);

#[cfg(test)]
impl Record {
    /// Records one call.
    pub(crate) fn push(&self, asked: Asked) {
        self.0.borrow_mut().push(asked);
    }

    /// Returns everything recorded on this list, in the order it was.
    pub(crate) fn taken(&self) -> Vec<Asked> {
        self.0.borrow().clone()
    }
}

/// What a session did with the terminal a key asked for.
///
/// [`Session::switch`] answers it, and the answer decides what happens to the cursor planes that
/// went back before the ask. A session that keeps the screen takes them again. Without that, a
/// person on a machine that can never switch loses the pointer for the rest of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Switched {
    /// The ask went out, so this session is about to lose the screen.
    ///
    /// The terminal moving arrives later, as a change the daemon sends.
    Asked,
    /// The ask went nowhere, so this session still holds the screen.
    ///
    /// Every switch on the direct shape, and a seated one the daemon refused.
    Refused,
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
    Direct {
        /// Whether a terminal has already been asked for and refused.
        ///
        /// Every switch on this shape is refused for the one reason, and a person who presses the
        /// chord presses it again. So the reason is stated once. See [`Session::switch`].
        refused_a_switch: bool,
    },
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
    /// opened and said nothing about itself — answers the direct shape, and one line at the crate's
    /// log says which shape this run got and why.
    ///
    /// A run started inside a desktop's own session lands on the direct shape: logind refuses
    /// control of a session that already has a controlling client, and libseat's builtin backend
    /// then hands back a seat that says nothing. Such a run fails at DRM master in
    /// [`Session::card`], which is the interlock this backend has always had.
    ///
    /// # A seat whose terminal is not the live one
    ///
    /// logind reads whether the session is active while the seat opens and reports an inactive one
    /// as disabled, so a run started on a terminal nobody is looking at gets a seat that is open
    /// and waiting. That is accepted here. The daemon holds that terminal, opens this session's
    /// devices, and enables the seat when a person switches to it; the display lights then. The
    /// interlock still holds — the screen belongs to whoever the daemon says it belongs to — and a
    /// program started this way runs instead of failing two seconds later.
    pub fn open() -> Self {
        match zgui_seat::Seat::open() {
            Ok(seat) => {
                if seat.opened_inactive() {
                    info!(
                        target: "zgui::platform",
                        "the devices come from the session daemon, on seat {}, and the terminal \
                         this run is on is not the live one — so it waits, and the display lights \
                         when somebody switches to it",
                        seat.name()
                    );
                } else {
                    info!(
                        target: "zgui::platform",
                        "the devices come from the session daemon, on seat {}, so this run needs \
                         no privilege of its own",
                        seat.name()
                    );
                }
                Self {
                    shape: Shape::Seated {
                        seat,
                        held: Vec::new(),
                        inputs: Vec::new(),
                    },
                    took: None,
                    #[cfg(test)]
                    asked: Record::default(),
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
                    shape: Shape::Direct {
                        refused_a_switch: false,
                    },
                    took: None,
                    #[cfg(test)]
                    asked: Record::default(),
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
            Shape::Direct { .. } => Taken::direct(direct_card(cards)?),
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
    /// # A path this session already holds is refused
    ///
    /// One path is one device here, because that is what [`Session::close_input`] gives back.
    /// seatd answers a path its client already holds with the *same* device id and its reference
    /// count raised, so a second open would leave two records of one device and one give-back — and
    /// the count would stand at one, with the daemon holding the device, for the rest of the run.
    /// A caller closes the path it holds and opens it again, which is what a resume does.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the node cannot be opened, when the descriptor that
    /// came back names something other than an evdev node, and when this session already holds a
    /// device at `path`. The first two are not unusual: on the seated path a daemon decides which
    /// devices this session may have, and on the direct path most nodes belong to a group.
    pub fn open_input(&mut self, path: &Path) -> Result<zgui_evdev::Device, PlatformError> {
        #[cfg(test)]
        self.asked.push(Asked::Open(path.to_owned()));

        match &mut self.shape {
            Shape::Seated { seat, inputs, .. } => seated_input(seat, inputs, path),
            Shape::Direct { .. } => {
                zgui_evdev::Device::open(path).map_err(|error| backend_error(&error))
            }
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
        #[cfg(test)]
        self.asked.push(Asked::Close(path.to_owned()));

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
        #[cfg(test)]
        self.asked.push(Asked::CloseEvery);

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

    /// Returns the descriptor a loop waits on beside the device, where this session has one.
    ///
    /// It becomes readable when the daemon has something to say about the terminal.
    /// [`Session::dispatch`] reads it. Nothing on the direct shape, where no daemon owns anything
    /// and a terminal switch reaches this program through nothing at all.
    pub(crate) fn descriptor(&self) -> Option<BorrowedFd<'_>> {
        match &self.shape {
            Shape::Seated { seat, .. } => Some(seat.descriptor()),
            Shape::Direct { .. } => None,
        }
    }

    /// Reads what the daemon has said since the last turn.
    ///
    /// This waits for nothing. A loop calls it once a turn, before anything else: a change here
    /// moves the devices, the master and the terminal, so everything below it in a turn is looking
    /// at a different machine afterwards.
    ///
    /// The direct shape answers nothing, always. There is no daemon to say anything, and a terminal
    /// switch away from a direct run leaves it holding the display.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when libseat could not read its connection. That is the
    /// seat gone: a connection that failed carries no later change, so the devices this session
    /// holds are unusable and there is nothing further to wait for.
    pub(crate) fn dispatch(&mut self) -> Result<Vec<Change>, PlatformError> {
        match &mut self.shape {
            Shape::Seated { seat, .. } => seat.dispatch().map_err(|error| {
                PlatformError::Backend(format!(
                    "the session daemon can no longer be read, so this run holds devices it can \
                     neither use nor be told about: {error}"
                ))
            }),
            Shape::Direct { .. } => Ok(Vec::new()),
        }
    }

    /// Asks for another terminal.
    ///
    /// A key is what asks for this. Both keyboard layouts say which terminal a chord asks for, so
    /// `Ctrl+Alt+F2` arrives as a reading carrying the number and the key that carried it reaches
    /// no surface.
    ///
    /// **Seated.** The daemon owns the terminal and moves it. This answers as soon as the request
    /// goes out, and the terminal moving arrives later as a change the daemon sends, which the loop
    /// reads as a suspend.
    ///
    /// **Direct.** Nothing here owns a terminal, so this run cannot move one. The refusal is
    /// reported once, because every switch on this shape fails for the same reason and a person who
    /// presses the chord presses it again — a line for each would fill the log of a run that can
    /// never answer differently.
    ///
    /// The answer says the ask went out, and says nothing about the terminal having moved. A switch
    /// that was accepted is reported later, as the change the daemon sends.
    pub(crate) fn switch(&mut self, terminal: u32) -> Switched {
        #[cfg(test)]
        self.asked.push(Asked::Switch(terminal));

        match &mut self.shape {
            Shape::Seated { seat, .. } => match seat.switch(terminal) {
                Ok(()) => Switched::Asked,
                Err(error) => {
                    warn!(
                        target: "zgui::platform",
                        "the session daemon was asked for terminal {terminal} and refused: {error}"
                    );
                    Switched::Refused
                }
            },
            Shape::Direct { refused_a_switch } => {
                if !std::mem::replace(refused_a_switch, true) {
                    #[cfg(test)]
                    self.asked.push(Asked::Refused);
                    warn!(
                        target: "zgui::platform",
                        "this run opened its devices itself, so no session daemon owns the terminal \
                         and a key cannot move it: terminal {terminal} was asked for and refused, \
                         and every later ask is refused without a word. Switch from elsewhere, such \
                         as `chvt` over a network connection"
                    );
                }
                Switched::Refused
            }
        }
    }

    /// Returns the direct shape, asked for by name.
    ///
    /// [`Session::open`] reads the machine, and on the seated path that takes the terminal. A unit
    /// test needs neither, so it is given this: the shape a machine with no libseat answers, built
    /// without asking libseat anything.
    #[cfg(test)]
    pub(crate) fn direct() -> Self {
        Self {
            shape: Shape::Direct {
                refused_a_switch: false,
            },
            took: None,
            asked: Record::default(),
        }
    }

    /// Returns every input call this session was asked for, in the order they were made.
    ///
    /// See [`Asked`] for what this is for.
    #[cfg(test)]
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.asked.taken()
    }

    /// Returns the list this session records on, for whatever else a switch runs.
    ///
    /// See [`Record`].
    #[cfg(test)]
    pub(crate) fn recording(&self) -> Record {
        self.asked.clone()
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
///
/// A path this session already holds is refused before the seat is asked, for the reason
/// [`Session::open_input`] states: one path is one device here, and a second open of one path is a
/// device this session could never give back.
fn seated_input(
    seat: &zgui_seat::Seat,
    inputs: &mut Vec<(PathBuf, zgui_seat::Device)>,
    path: &Path,
) -> Result<zgui_evdev::Device, PlatformError> {
    if inputs.iter().any(|(held, _)| held == path) {
        return Err(PlatformError::Backend(format!(
            "{} is already open through this session, and a daemon that answered a second open \
             would hold a record of the device this run has no way to give back",
            path.display()
        )));
    }

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

#[cfg(test)]
mod tests {
    //! What a session with no daemon does with the terminal a key asked for.
    //!
    //! The direct shape is the one a test can build. [`Session::direct`] asks libseat nothing and
    //! takes no card and no terminal, so what is left to decide is what a switch does on a machine
    //! where nothing owns one.

    use super::{Asked, Session};

    #[test]
    fn a_run_that_owns_no_terminal_states_the_refusal_once() {
        // Every switch on this shape fails for the one reason, and a person who presses the chord
        // presses it again: one line each would fill the log of a run that can never answer
        // differently. Every ask is still recorded, so what is held up here is the state rather
        // than the call.
        let mut session = Session::direct();

        session.switch(2);
        session.switch(3);
        session.switch(2);

        assert_eq!(
            session.asked(),
            [
                Asked::Switch(2),
                Asked::Refused,
                Asked::Switch(3),
                Asked::Switch(2),
            ],
            "the first ask was refused with the reason, and every later one without a word"
        );
    }
}
