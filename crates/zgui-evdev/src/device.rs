//! One `/dev/input/eventN` node: what it is, what it can report, and what it reports.

use std::ffi::c_int;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{Mode, OFlags};

use crate::code::{Absolute, Code, EventType, Key, Relative};
use crate::error::{Error, Result};
use crate::event::{Batch, Reader};
use crate::ioctl;
use crate::sys;

/// The longest device name this crate asks for.
///
/// The kernel truncates to whatever is asked for and reports how much it wrote, so a name longer
/// than this arrives cut rather than lost.
const NAME_LIMIT: usize = 256;

/// Returns how many bytes hold every code of `C`.
///
/// The count comes from the code type, so a map can only be asked for at the length its own
/// vocabulary needs.
const fn bitmap_bytes<C: Code>() -> usize {
    C::COUNT.div_ceil(8) as usize
}

/// The most bytes a bitmap keeps.
///
/// A code is a `u16`, so this many bytes hold every code the kernel can name and a byte past them
/// names none. The kernel's own maps are far smaller — ninety-six bytes for the key codes, the
/// largest of them — so this bounds what a caller can hand to [`Bitmap::from_bytes`] rather than
/// anything a device produces.
pub const BITMAP_LIMIT: usize = (u16::MAX as usize + 1) / 8;

/// A set of codes drawn from one vocabulary.
///
/// The kernel answers "which codes does this device have" with a bitmap: bit `n` is code `n`. A
/// map is as long as the caller asked for, so a code past its end is a code the kernel had no room
/// to report and reads as absent.
///
/// `C` says what the bits mean. Bit one is `KEY_ESC` in a `Bitmap<Key>` and `REL_Y` in a
/// `Bitmap<Relative>`. The two are different types, so a map cannot be read against a vocabulary it
/// was not filled from, and [`Capabilities::new`] cannot be given its four maps in the wrong order.
///
/// ```
/// use zgui_evdev::{Bitmap, Key};
///
/// let keys = Bitmap::from_codes([Key::KEY_A, Key::BTN_LEFT]);
///
/// assert!(keys.contains(Key::KEY_A));
/// assert!(!keys.contains(Key::KEY_B));
/// assert_eq!(keys.iter().collect::<Vec<_>>(), [Key::KEY_A, Key::BTN_LEFT]);
/// ```
pub struct Bitmap<C> {
    /// The bits, least significant first, as the kernel wrote them.
    bits: Vec<u8>,
    /// What the bits mean, carrying no value.
    vocabulary: PhantomData<fn() -> C>,
}

// The four impls below are written out by hand. A derive would bound `C`, and `C` is a marker no
// value here is ever an instance of: a `Bitmap<Key>` is as comparable and as cloneable as its bytes
// are, whatever `Key` implements.
impl<C> Clone for Bitmap<C> {
    fn clone(&self) -> Self {
        Self {
            bits: self.bits.clone(),
            vocabulary: PhantomData,
        }
    }
}

impl<C> Default for Bitmap<C> {
    fn default() -> Self {
        Self {
            bits: Vec::new(),
            vocabulary: PhantomData,
        }
    }
}

impl<C> PartialEq for Bitmap<C> {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}

impl<C> Eq for Bitmap<C> {}

impl<C: Code> std::fmt::Debug for Bitmap<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter().map(Code::raw)).finish()
    }
}

impl<C: Code> Bitmap<C> {
    /// Returns the bitmap these bytes hold, up to the last byte a code can reach.
    ///
    /// A code is sixteen bits, so [`BITMAP_LIMIT`] bytes hold every code there is and anything past
    /// that names nothing. Bytes beyond it are dropped, which makes [`Bitmap::iter`] total: every
    /// bit it can reach has a code.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bits: bytes[..bytes.len().min(BITMAP_LIMIT)].to_vec(),
            vocabulary: PhantomData,
        }
    }

    /// Returns the bitmap holding exactly `codes`.
    ///
    /// This is how a test states a device's capabilities without a device, and how a caller
    /// describes one it is about to create.
    pub fn from_codes(codes: impl IntoIterator<Item = C>) -> Self {
        let mut bits: Vec<u8> = Vec::new();
        for code in codes {
            let code = code.raw();
            let byte = usize::from(code) / 8;
            if bits.len() <= byte {
                bits.resize(byte + 1, 0);
            }
            bits[byte] |= 1 << (code % 8);
        }
        Self {
            bits,
            vocabulary: PhantomData,
        }
    }

    /// Returns `true` if `code` is in this map.
    pub fn contains(&self, code: C) -> bool {
        let code = code.raw();
        let byte = usize::from(code) / 8;
        self.bits
            .get(byte)
            .is_some_and(|bits| bits & (1 << (code % 8)) != 0)
    }

    /// Returns every code in this map, in order.
    pub fn iter(&self) -> impl Iterator<Item = C> + '_ {
        self.bits.iter().enumerate().flat_map(|(byte, bits)| {
            // The cap in `from_bytes` already makes this conversion total. It is written as one
            // that can decline anyway, so the code holds that totality on its own.
            (0..8)
                .filter(move |bit| bits & (1 << bit) != 0)
                .filter_map(move |bit| u16::try_from(byte * 8 + bit).ok())
                .map(C::new)
        })
    }

    /// Returns how many codes are in this map.
    pub fn len(&self) -> usize {
        self.bits
            .iter()
            .map(|bits| bits.count_ones() as usize)
            .sum()
    }

    /// Returns `true` if this map holds no codes at all.
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|bits| *bits == 0)
    }
}

/// What a device is, as the kernel and the hardware describe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Which bus it is on: `BUS_USB`, `BUS_BLUETOOTH`, `BUS_I8042`.
    pub bus: u16,
    /// The vendor's number.
    pub vendor: u16,
    /// The product's number.
    pub product: u16,
    /// The version the device reports.
    pub version: u16,
}

/// The range and the current reading of one absolute axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisRange {
    /// What the axis reads right now.
    pub value: i32,
    /// The smallest value it reports.
    pub minimum: i32,
    /// The largest value it reports.
    pub maximum: i32,
    /// How much noise the driver filters out.
    pub fuzz: i32,
    /// A band around the centre reported as the centre.
    pub flat: i32,
    /// Units per millimetre, or units per radian for a rotation.
    pub resolution: i32,
}

/// A job a device does.
///
/// A device does as many of these as it does. The set is what matters: a keyboard with a wheel and
/// a mouse with a full key map are both ordinary, and a device modelled as one job would have the
/// other half of it dropped.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// It sends the keys a keyboard has.
    Keyboard,
    /// It reports how far it moved.
    Pointer,
    /// It reports where it is.
    Touch,
}

/// The jobs one device does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Roles {
    /// Whether it sends the keys a keyboard has.
    keyboard: bool,
    /// Whether it reports how far it moved.
    pointer: bool,
    /// Whether it reports where it is.
    touch: bool,
}

impl Roles {
    /// Returns `true` if this device does `role`.
    pub fn contains(self, role: Role) -> bool {
        match role {
            Role::Keyboard => self.keyboard,
            Role::Pointer => self.pointer,
            Role::Touch => self.touch,
        }
    }

    /// Returns `true` if this device does none of the jobs this crate names.
    ///
    /// A power button, a lid switch and an accelerometer all land here. They are real devices, and
    /// a caller reading input for a window has nothing to do with them.
    pub fn is_empty(self) -> bool {
        !self.keyboard && !self.pointer && !self.touch
    }

    /// Returns every job this device does.
    pub fn iter(self) -> impl Iterator<Item = Role> {
        [Role::Keyboard, Role::Pointer, Role::Touch]
            .into_iter()
            .filter(move |role| self.contains(*role))
    }
}

/// Which codes a device reports, by the type they arrive under.
///
/// The kernel answers `EVIOCGBIT` with these maps, and they are kept as it wrote them.
/// Classification is a question asked *of* them, so a caller that disagrees with [`Roles`] can read
/// the maps and decide for itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Which event types the device emits.
    types: Bitmap<EventType>,
    /// Which keys and buttons it has.
    keys: Bitmap<Key>,
    /// Which relative axes it has.
    relative: Bitmap<Relative>,
    /// Which absolute axes it has.
    absolute: Bitmap<Absolute>,
}

impl Capabilities {
    /// Builds the capabilities these four maps describe.
    ///
    /// Each map names the vocabulary it holds, so the four cannot be given in the wrong order.
    pub fn new(
        types: Bitmap<EventType>,
        keys: Bitmap<Key>,
        relative: Bitmap<Relative>,
        absolute: Bitmap<Absolute>,
    ) -> Self {
        Self {
            types,
            keys,
            relative,
            absolute,
        }
    }

    /// Returns which event types the device emits.
    pub fn types(&self) -> &Bitmap<EventType> {
        &self.types
    }

    /// Returns which keys and buttons it has.
    pub fn keys(&self) -> &Bitmap<Key> {
        &self.keys
    }

    /// Returns which relative axes it has.
    pub fn relative(&self) -> &Bitmap<Relative> {
        &self.relative
    }

    /// Returns which absolute axes it has.
    pub fn absolute(&self) -> &Bitmap<Absolute> {
        &self.absolute
    }

    /// Returns `true` if the device emits `kind` at all.
    pub fn has(&self, kind: EventType) -> bool {
        self.types.contains(kind)
    }

    /// Returns the jobs these capabilities amount to.
    ///
    /// Each answer is a question about the codes. The types alone put every mouse among the
    /// keyboards, because a mouse has `EV_KEY` for its buttons.
    ///
    /// - A keyboard has a code that is a key rather than a button, in any of the blocks the kernel
    ///   puts keys in. This is udev's `ID_INPUT_KEY` rule. See [`Key::is_key`].
    /// - A pointer has both `REL_X` and `REL_Y`. A device with a wheel and no axes is a keyboard
    ///   with a wheel on it, and there are several.
    /// - A touch device has both `ABS_X` and `ABS_Y`, or the multi-touch pair. A volume dial
    ///   reports an absolute axis too, and it is not a touchscreen.
    ///
    /// ```
    /// use zgui_evdev::{Bitmap, Capabilities, EventType, Key, Relative, Role};
    ///
    /// let mouse = Capabilities::new(
    ///     Bitmap::from_codes([EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL]),
    ///     Bitmap::from_codes([Key::BTN_LEFT, Key::BTN_RIGHT]),
    ///     Bitmap::from_codes([Relative::REL_X, Relative::REL_Y]),
    ///     Bitmap::default(),
    /// );
    ///
    /// assert_eq!(mouse.roles().iter().collect::<Vec<_>>(), [Role::Pointer]);
    /// assert!(!mouse.roles().contains(Role::Keyboard));
    /// ```
    pub fn roles(&self) -> Roles {
        Roles {
            keyboard: self.has(EventType::EV_KEY) && self.keys.iter().any(Key::is_key),
            pointer: self.has(EventType::EV_REL)
                && self.relative.contains(Relative::REL_X)
                && self.relative.contains(Relative::REL_Y),
            touch: self.has(EventType::EV_ABS)
                && ((self.absolute.contains(Absolute::ABS_X)
                    && self.absolute.contains(Absolute::ABS_Y))
                    || (self.absolute.contains(Absolute::ABS_MT_POSITION_X)
                        && self.absolute.contains(Absolute::ABS_MT_POSITION_Y))),
        }
    }
}

/// An open input device.
///
/// [`Device::open`] opens a node, and [`Device::over`] builds one over a descriptor a session
/// daemon opened. Either way the device answers what it is as soon as it is built, and
/// [`Device::read`] answers what it has reported since the last call.
///
/// ```no_run
/// use zgui_evdev::{Device, EventType};
///
/// let mut device = Device::open("/dev/input/event0")?;
///
/// // The kernel ends every report with one, so every input device emits it.
/// assert!(device.capabilities().has(EventType::EV_SYN));
///
/// // A loop parks on the descriptor and calls this when it wakes.
/// for batch in device.read()? {
///     for event in &batch.events {
///         assert_eq!(event.key().is_some(), event.kind == EventType::EV_KEY);
///     }
/// }
/// # Ok::<(), zgui_evdev::Error>(())
/// ```
#[derive(Debug)]
pub struct Device {
    /// The open descriptor.
    fd: OwnedFd,
    /// Where it was opened from, for messages.
    path: PathBuf,
    /// What the device calls itself.
    name: String,
    /// What the hardware says it is.
    identity: Identity,
    /// Which codes it reports.
    capabilities: Capabilities,
    /// Whether the kernel accepted the monotonic clock for this device's timestamps.
    monotonic: bool,
    /// Whether this crate holds the device's grab.
    grabbed: bool,
    /// What turns reads into batches.
    reader: Reader,
}

impl Device {
    /// Opens the device at `path` and reads what it is.
    ///
    /// The node is opened read-only: nothing here writes to a device, and a caller outside the
    /// `input` group is more likely to be allowed to read one than to open it for writing. It is
    /// opened non-blocking, so [`Device::read`] is a poll rather than a wait.
    ///
    /// The device is asked to timestamp on the monotonic clock. A kernel that refuses leaves the
    /// stream on the real clock and answers this call the same way, and
    /// [`Device::has_monotonic_timestamps`] says which clock this device is on.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] when the device cannot be opened — permission is the ordinary case
    /// — and [`Error::Ioctl`] when the node opens and is not an input device.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let fd = rustix::fs::open(
            &path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|errno| Error::Open {
            path: path.clone(),
            source: errno.into(),
        })?;

        Self::described(fd, path)
    }

    /// Builds a device over `fd` and reads what it is.
    ///
    /// `path` names the device for messages. This call does not open it, because the caller
    /// already holds the descriptor. A session daemon that opened the node and handed the
    /// descriptor over is the caller this exists for, and naming the device keeps its messages
    /// reading the way [`Device::open`] makes them read.
    ///
    /// `fd` is checked to name an evdev node first. A session hands out graphics cards over the
    /// same call it hands out input devices, and a descriptor onto one of those would otherwise
    /// build a device with no name, no capabilities and no job.
    ///
    /// `O_NONBLOCK` is raised on `fd` after that, because [`Device::read`] answers at once only
    /// while that flag is on. logind and seatd both open with it, and neither says so anywhere a
    /// program may rely on, so it is asked for here.
    ///
    /// # Identifying before changing
    ///
    /// The status flags belong to the open file description, so raising one reaches the daemon's
    /// own descriptor onto the node as well. On a node this crate keeps, that is the point of the
    /// raise. On a descriptor this crate is about to refuse, it is a change to somebody else's
    /// device. So the identity is asked for first, and a descriptor that names something other than
    /// an input device goes back to its owner with the flags it arrived with.
    ///
    /// # The open mode
    ///
    /// [`Device::open`] asks for `O_RDONLY`, and logind hands over `O_RDWR`. Every call this crate
    /// makes on a device is an ioctl or a read, and both are answered on either mode, so what a
    /// device does is the same on both. Nothing here writes to a node.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] naming `path` when `fd` names something other than an evdev
    /// node, and when `fd` cannot be made non-blocking. Returns [`Error::Ioctl`] when the node
    /// answers and will not say what it is.
    pub fn over(fd: OwnedFd, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        confirm_input_device(fd.as_fd(), &path)?;
        raise_non_blocking(fd.as_fd(), &path)?;

        Self::described(fd, path)
    }

    /// Asks an open descriptor what device it is, and builds it.
    ///
    /// [`Device::open`] and [`Device::over`] both end here, so a descriptor this crate opened and a
    /// descriptor handed to it are read the same way.
    fn described(fd: OwnedFd, path: PathBuf) -> Result<Self> {
        // `EVIOCSCLOCKID` arrived in 2.6.36 and a driver may still refuse it. Every later read of
        // this device is timestamped on whichever clock is in force, so which one it is has to be
        // recorded rather than assumed. See `Event::at`.
        let monotonic = ioctl::issue(
            fd.as_fd(),
            ioctl::SET_CLOCK,
            &mut c_int::try_from(sys::CLOCK_MONOTONIC).unwrap_or(1),
        )
        .is_ok();

        let name = read_name(fd.as_fd())?;
        let identity = read_identity(fd.as_fd())?;
        let capabilities = read_capabilities(fd.as_fd())?;

        Ok(Self {
            fd,
            path,
            name,
            identity,
            capabilities,
            monotonic,
            grabbed: false,
            reader: Reader::new(),
        })
    }

    /// Returns which device this is.
    ///
    /// [`Device::open`] answers the path it opened. [`Device::over`] answers the path its caller
    /// named.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns what the device calls itself.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns what the hardware says it is.
    pub fn identity(&self) -> Identity {
        self.identity
    }

    /// Returns which codes it reports.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Returns the jobs this device does.
    pub fn roles(&self) -> Roles {
        self.capabilities.roles()
    }

    /// Returns `true` if this device timestamps its events on the monotonic clock.
    ///
    /// Asked for when the device was opened. A kernel or a driver that refused leaves the stream
    /// on `CLOCK_REALTIME`, where a clock step moves every later timestamp and can move one
    /// backwards. A caller that measures an interval between events — a double click, a key
    /// repeat — reads this before trusting one.
    pub fn has_monotonic_timestamps(&self) -> bool {
        self.monotonic
    }

    /// Returns the range and the current reading of one absolute axis.
    ///
    /// The reading comes from the axis itself, so this is the other half of resynchronising after a
    /// `SYN_DROPPED` — see [`Batch`] — where the position an axis reached in the discarded part is
    /// otherwise unknowable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when `axis` is past the last one the kernel names, and
    /// [`Error::Ioctl`] when the device has no such axis.
    pub fn axis(&self, axis: Absolute) -> Result<AxisRange> {
        let mut info = sys::input_absinfo::default();
        ioctl::issue(self.fd(), ioctl::absolute(axis.raw())?, &mut info)?;
        Ok(AxisRange {
            value: info.value,
            minimum: info.minimum,
            maximum: info.maximum,
            fuzz: info.fuzz,
            flat: info.flat,
            resolution: info.resolution,
        })
    }

    /// Returns which keys are held down right now.
    ///
    /// The device itself is asked, so this answers in the two places a stream cannot.
    /// One is opening a device already in use: a modifier held when the first event arrives was
    /// pressed before anything was listening, and nothing later in the stream says so. The other is
    /// resynchronising after the kernel reports `SYN_DROPPED` — see [`Batch`] — where a key that
    /// went down in the discarded part would otherwise stay down for ever.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses the query.
    pub fn pressed_keys(&self) -> Result<Bitmap<Key>> {
        let mut bits = [0_u8; bitmap_bytes::<Key>()];
        let written = ioctl::issue_bytes(self.fd(), ioctl::key_state(), &mut bits)?;
        Ok(Bitmap::from_bytes(&bits[..written.min(bits.len())]))
    }

    /// Takes the device, so that its events reach this process and no other.
    ///
    /// A grab stops a keystroke reaching the console behind a full-screen application. It is held
    /// by the open file description, and only one client can hold it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when another client already holds it.
    pub fn grab(&mut self) -> Result<()> {
        ioctl::issue_value(self.fd.as_fd(), ioctl::GRAB, 1)?;
        self.grabbed = true;
        Ok(())
    }

    /// Gives the device back.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses.
    pub fn release(&mut self) -> Result<()> {
        ioctl::issue_value(self.fd.as_fd(), ioctl::GRAB, 0)?;
        self.grabbed = false;
        Ok(())
    }

    /// Returns `true` if this device is grabbed.
    pub fn is_grabbed(&self) -> bool {
        self.grabbed
    }

    /// Reads whatever the device has to say, without waiting.
    ///
    /// The device is opened non-blocking, so this returns an empty vector when nothing has
    /// happened yet. A loop parks on the descriptor and calls this when it wakes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Read`] when the read fails for a reason other than there being nothing to
    /// read. Its errno says whether this device is worth polling again: see the variant.
    pub fn read(&mut self) -> Result<Vec<Batch>> {
        self.reader.read(self.fd.as_fd())
    }

    /// Returns the descriptor, for the modules that issue ioctls against it.
    fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// Gives the grab back before the descriptor goes.
///
/// Closing the description releases the grab on its own, so this matters in the one case where
/// closing this descriptor does not close the description: a caller that duplicated it. A grab left
/// behind is a keyboard whose keys reach nothing, and no later run puts it back.
impl Drop for Device {
    fn drop(&mut self) {
        if self.grabbed {
            let _ = ioctl::issue_value(self.fd.as_fd(), ioctl::GRAB, 0);
        }
    }
}

/// The open descriptor, for a caller that needs the device as a file.
///
/// A loop polls this. A device says nothing until it has something to say, so the descriptor is how
/// a caller waits for one without asking it repeatedly.
///
/// # The borrow
///
/// The descriptor is owned by the [`Device`] and is closed when the device is dropped. A caller
/// that keeps the number instead of the borrow must keep the device alive for at least as long: a
/// number kept past the drop names whatever the process opened next.
impl AsFd for Device {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

/// Raises `O_NONBLOCK` on `fd`, keeping every other status flag it carries.
///
/// [`Device::open`] asks for the flag when it opens the node, and this is the same thing for a
/// descriptor somebody else opened. `F_SETFL` writes the whole set of status flags, so `F_GETFL`
/// reads them first.
///
/// # Errors
///
/// Returns [`Error::Unusable`] naming `path` when the kernel refuses either call.
fn raise_non_blocking(fd: BorrowedFd<'_>, path: &Path) -> Result<()> {
    let flags = rustix::fs::fcntl_getfl(fd).map_err(|errno| {
        Error::Unusable(format!(
            "cannot read the flags of the descriptor for {}: {errno}",
            path.display()
        ))
    })?;

    rustix::fs::fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(|errno| {
        Error::Unusable(format!(
            "cannot make the descriptor for {} non-blocking: {errno}",
            path.display()
        ))
    })
}

/// Confirms that `fd` names an evdev node.
///
/// The request is `EVIOCGVERSION`. The input driver answers it for every device before it reads
/// anything about the hardware, so one ioctl is enough and a caller with no privilege still gets an
/// answer. A descriptor onto anything else refuses the request number.
///
/// # Errors
///
/// Returns [`Error::Unusable`] naming `path` when the query is refused.
fn confirm_input_device(fd: BorrowedFd<'_>, path: &Path) -> Result<()> {
    let mut version: c_int = 0;

    ioctl::issue(fd, ioctl::GET_VERSION, &mut version).map_err(|error| {
        Error::Unusable(format!(
            "the descriptor for {} names something other than an input device: {error}",
            path.display()
        ))
    })
}

/// Asks the device what it calls itself.
fn read_name(fd: BorrowedFd<'_>) -> Result<String> {
    let mut bytes = [0_u8; NAME_LIMIT];
    let written = ioctl::issue_bytes(fd, ioctl::name(), &mut bytes)?;
    Ok(name_from(&bytes[..written.min(NAME_LIMIT)]))
}

/// Returns the name held in what `EVIOCGNAME` wrote.
///
/// Three rules:
///
/// - The kernel counts the terminator in the length it reports, so the bytes it wrote are one
///   longer than the name.
/// - A driver may write a shorter string than the length says, so the name ends at the first zero
///   whatever the length was.
/// - A name that is not UTF-8 is kept. Such a device still reports keys, and dropping it would lose
///   a working one. The replacement character says what happened where a person reads it.
fn name_from(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Asks the device what the hardware says it is.
fn read_identity(fd: BorrowedFd<'_>) -> Result<Identity> {
    let mut id = sys::input_id::default();
    ioctl::issue(fd, ioctl::GET_ID, &mut id)?;
    Ok(Identity {
        bus: id.bustype,
        vendor: id.vendor,
        product: id.product,
        version: id.version,
    })
}

/// Asks the device which codes it reports.
///
/// The type map is read first and the three code maps are read only where it says there is
/// something to read. A device with no absolute axes is the ordinary case, and asking it for them
/// gets zeros at best.
fn read_capabilities(fd: BorrowedFd<'_>) -> Result<Capabilities> {
    // The type map is the one request that is not `read_codes`: it is asked for through the slot
    // `EV_SYN` would occupy, and there is nothing to check it against yet.
    let types = read_bits(fd, EventType::EV_SYN, bitmap_bytes::<EventType>())?;
    let keys = read_codes(fd, &types)?;
    let relative = read_codes(fd, &types)?;
    let absolute = read_codes(fd, &types)?;

    Ok(Capabilities::new(types, keys, relative, absolute))
}

/// Reads the codes of one vocabulary, or an empty map where the device has no such type.
///
/// Which request to issue and how long the map is both come from `C`, so a call cannot ask for one
/// vocabulary and store the answer as another.
fn read_codes<C: Code>(fd: BorrowedFd<'_>, types: &Bitmap<EventType>) -> Result<Bitmap<C>> {
    if types.contains(C::KIND) {
        read_bits(fd, C::KIND, bitmap_bytes::<C>())
    } else {
        Ok(Bitmap::default())
    }
}

/// Reads one `EVIOCGBIT` map of `len` bytes.
fn read_bits<C: Code>(fd: BorrowedFd<'_>, kind: EventType, len: usize) -> Result<Bitmap<C>> {
    let mut bits = vec![0_u8; len];
    let written = ioctl::issue_bytes(fd, ioctl::bits(kind.raw())?, &mut bits)?;
    bits.truncate(written.min(len));
    Ok(Bitmap::from_bytes(&bits))
}

#[cfg(test)]
mod tests {
    //! Bitmaps and classification, over capabilities written here.
    //!
    //! Every device on the development machine is in here as the bitmaps it reports, so these
    //! assertions are about real hardware and need none of it present. The devices that matter are
    //! the ones that do two jobs: a mouse with a full key map and a keyboard with a wheel both
    //! exist, and a classification that answers with one word loses half of each.

    use super::*;

    /// The capabilities of a device with these types, keys, relative axes and absolute axes.
    fn capabilities(
        types: &[EventType],
        keys: &[Key],
        relative: &[Relative],
        absolute: &[Absolute],
    ) -> Capabilities {
        Capabilities::new(
            Bitmap::from_codes(types.iter().copied()),
            Bitmap::from_codes(keys.iter().copied()),
            Bitmap::from_codes(relative.iter().copied()),
            Bitmap::from_codes(absolute.iter().copied()),
        )
    }

    #[test]
    fn a_name_ends_at_the_terminator_the_kernel_counted() {
        // The kernel reports the length including the zero, so what it wrote is one longer than
        // the name. Keeping the terminator would put a NUL in the middle of every log line.
        assert_eq!(name_from(b"Razer BlackWidow\0"), "Razer BlackWidow");
    }

    #[test]
    fn a_name_ends_at_the_first_zero_whatever_the_length_said() {
        // A driver may write a shorter string than the length it reports, and the bytes behind it
        // are whatever was in the buffer. No device here does that, so this is the case only a
        // written-out test reaches.
        assert_eq!(name_from(b"Trackball\0\0\0junk\0"), "Trackball");
    }

    #[test]
    fn a_name_with_no_terminator_at_all_is_the_whole_of_what_was_written() {
        // The kernel truncates to the buffer it was given, so a name longer than the buffer
        // arrives with no room left for the zero.
        assert_eq!(name_from(b"a name that filled it"), "a name that filled it");
        assert_eq!(name_from(b""), "");
    }

    #[test]
    fn a_name_that_is_not_text_is_kept_rather_than_refused() {
        // The device still reports keys. Refusing it over its descriptor string would lose a
        // working device, and the replacement character says what happened where a person reads
        // it.
        assert_eq!(name_from(b"caf\xff\0"), "caf\u{fffd}");
    }

    #[test]
    fn a_bitmap_holds_the_codes_it_was_built_from() {
        let map = Bitmap::from_codes([Key::KEY_A, Key::KEY_Z, Key::BTN_LEFT]);

        assert!(map.contains(Key::KEY_A));
        assert!(map.contains(Key::BTN_LEFT));
        assert!(!map.contains(Key::KEY_B));
        assert_eq!(map.len(), 3);
        assert!(!map.is_empty());
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            [Key::KEY_A, Key::KEY_Z, Key::BTN_LEFT],
            "the codes come back in order, whatever order they went in"
        );
    }

    #[test]
    fn a_code_past_the_end_of_a_bitmap_is_absent_rather_than_a_panic() {
        // The kernel fills as much of a map as was asked for. A caller that asked for two bytes
        // and then asks about code 700 gets an answer rather than an index out of range.
        let map = Bitmap::from_bytes(&[0b0000_0011, 0]);

        assert!(map.contains(Key::new(0)));
        assert!(map.contains(Key::new(1)));
        assert!(!map.contains(Key::new(700)));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn a_bitmap_longer_than_the_codes_it_could_hold_is_cut_rather_than_walked() {
        // No kernel path reaches this, and `from_bytes` is public and documented as safe, so a
        // caller with a long buffer can. Walking it would run the bit index past sixteen bits.
        let long: Bitmap<Key> = Bitmap::from_bytes(&vec![0xff; BITMAP_LIMIT + 1000]);

        assert_eq!(
            long.iter().count(),
            BITMAP_LIMIT * 8,
            "every code there is, and none that there is not"
        );
        assert_eq!(
            long.iter().last(),
            Some(Key::new(u16::MAX)),
            "the walk ends on the last code rather than past it"
        );
        assert_eq!(long.len(), BITMAP_LIMIT * 8);
    }

    #[test]
    fn an_empty_bitmap_holds_nothing() {
        assert!(Bitmap::<Key>::default().is_empty());
        assert!(
            Bitmap::<Key>::from_bytes(&[0, 0, 0]).is_empty(),
            "bytes the kernel wrote as zero are no codes at all"
        );
    }

    #[test]
    fn a_remote_control_that_reports_only_the_high_keys_is_still_a_keyboard() {
        // An HDMI-CEC remote or an infrared receiver reports `KEY_OK` and its neighbours and
        // nothing under `BTN_MISC`. Reading the first block alone left this classified as nothing,
        // so a consumer would never open it and every button on it would reach nothing. Nothing on
        // the development machine has this shape, which is why it is asserted here.
        let remote = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_OK, Key::KEY_CHANNELUP, Key::KEY_SUBTITLE],
            &[],
            &[],
        );

        assert_eq!(remote.roles().iter().collect::<Vec<_>>(), [Role::Keyboard]);
    }

    #[test]
    fn a_keyboard_is_a_device_with_keys_from_the_first_block() {
        let keyboard = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_ESC, Key::KEY_A, Key::KEY_LEFTSHIFT],
            &[],
            &[],
        );

        assert_eq!(
            keyboard.roles().iter().collect::<Vec<_>>(),
            [Role::Keyboard]
        );
    }

    #[test]
    fn the_reserved_code_is_not_evidence_of_a_keyboard() {
        // Code zero means nothing. A device whose map holds it and no other key in the block has
        // no keyboard in it, whatever the bit says.
        let odd = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_RESERVED, Key::BTN_LEFT],
            &[],
            &[],
        );

        assert!(!odd.roles().contains(Role::Keyboard));
    }

    #[test]
    fn a_mouse_is_a_pointer_and_not_a_keyboard() {
        // A mouse has `EV_KEY` for its buttons. Reading the types alone would make every mouse a
        // keyboard, which is why the answer is a question about the codes.
        let mouse = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL],
            &[Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE],
            &[Relative::REL_X, Relative::REL_Y, Relative::REL_WHEEL],
            &[],
        );

        assert_eq!(mouse.roles().iter().collect::<Vec<_>>(), [Role::Pointer]);
        assert!(!mouse.roles().contains(Role::Keyboard));
    }

    #[test]
    fn a_device_that_does_two_jobs_reports_both() {
        // The Logitech MX Master on the development machine, as `/proc/bus/input/devices` reports
        // it: a full key map and two relative axes on one node. Collapsing this to one word loses
        // whichever half was not chosen.
        let combined = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL],
            &[Key::KEY_A, Key::BTN_LEFT],
            &[Relative::REL_X, Relative::REL_Y],
            &[],
        );

        assert_eq!(
            combined.roles().iter().collect::<Vec<_>>(),
            [Role::Keyboard, Role::Pointer],
            "one device, two jobs"
        );
    }

    #[test]
    fn a_keyboard_with_a_wheel_is_not_a_pointer() {
        // The Razer keyboard's second node, again as the machine reports it: `EV_REL` with
        // `REL_HWHEEL` and no axes. A pointer is what has somewhere to move to.
        let keyboard = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL],
            &[Key::KEY_VOLUMEUP, Key::KEY_PLAYPAUSE],
            &[Relative::REL_HWHEEL, Relative::REL_HWHEEL_HI_RES],
            &[],
        );

        assert_eq!(
            keyboard.roles().iter().collect::<Vec<_>>(),
            [Role::Keyboard],
            "a wheel is not a pointer"
        );
    }

    #[test]
    fn a_touchscreen_is_an_absolute_device_with_somewhere_to_touch() {
        let touchscreen = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_ABS],
            &[Key::BTN_TOUCH],
            &[],
            &[
                Absolute::ABS_X,
                Absolute::ABS_Y,
                Absolute::ABS_MT_POSITION_X,
                Absolute::ABS_MT_POSITION_Y,
            ],
        );

        assert_eq!(
            touchscreen.roles().iter().collect::<Vec<_>>(),
            [Role::Touch]
        );
    }

    #[test]
    fn a_multi_touch_device_that_reports_only_the_slot_axes_is_still_a_touch_device() {
        let pad = capabilities(
            &[EventType::EV_SYN, EventType::EV_ABS],
            &[],
            &[],
            &[Absolute::ABS_MT_POSITION_X, Absolute::ABS_MT_POSITION_Y],
        );

        assert!(pad.roles().contains(Role::Touch));
    }

    #[test]
    fn a_dial_is_not_a_touch_device() {
        // The Razer keyboard's third node reports `ABS_VOLUME` and `ABS_MISC`. An absolute axis is
        // not a surface, and treating one as a touchscreen would put a cursor wherever the volume
        // was.
        let dial = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_ABS],
            &[Key::KEY_VOLUMEUP],
            &[],
            &[Absolute::ABS_VOLUME, Absolute::ABS_MISC],
        );

        assert!(!dial.roles().contains(Role::Touch));
        assert_eq!(dial.roles().iter().collect::<Vec<_>>(), [Role::Keyboard]);
    }

    #[test]
    fn a_device_this_crate_has_no_job_for_reports_none() {
        // A power button sends `KEY_POWER`, which is in the first block, so it is a keyboard by
        // this measure and honestly so. A lid switch sends `EV_SW` and nothing else, and that is
        // what an empty set is for.
        let lid = capabilities(&[EventType::EV_SYN, EventType::EV_SW], &[], &[], &[]);

        assert!(lid.roles().is_empty());
        assert_eq!(lid.roles().iter().count(), 0);
    }

    #[test]
    fn a_type_the_device_does_not_have_is_answered_without_its_codes() {
        // The kernel is asked for a code map only where the type map says there is one. A caller
        // reading the relative map of a keyboard gets an empty map rather than an error.
        let keyboard = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_A],
            &[],
            &[],
        );

        assert!(keyboard.has(EventType::EV_KEY));
        assert!(!keyboard.has(EventType::EV_REL));
        assert!(keyboard.relative().is_empty());
    }

    #[test]
    fn a_bitmap_is_as_many_bytes_as_its_own_vocabulary_needs() {
        assert_eq!(bitmap_bytes::<EventType>(), 4, "thirty-two event types");
        assert_eq!(
            bitmap_bytes::<Key>(),
            96,
            "seven hundred and sixty-eight key codes"
        );
        assert_eq!(bitmap_bytes::<Relative>(), 2, "sixteen relative axes");
        assert_eq!(bitmap_bytes::<Absolute>(), 8, "sixty-four absolute axes");
    }

    #[test]
    fn a_map_of_one_vocabulary_is_not_a_map_of_another() {
        // Bit one is `KEY_ESC` here and `REL_Y` there, and the two maps are different types, so
        // `Capabilities::new` cannot be handed them the wrong way round. This is the assertion
        // that the marker is doing something; the rest of it is the compiler's.
        let keys = Bitmap::from_codes([Key::new(1)]);
        let axes = Bitmap::from_codes([Relative::new(1)]);

        assert!(keys.contains(Key::KEY_ESC));
        assert!(axes.contains(Relative::REL_Y));
    }
}
