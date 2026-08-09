//! One `/dev/input/eventN` node: what it is, what it can report, and what it reports.

use std::path::{Path, PathBuf};

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{Mode, OFlags};

use crate::code::{Absolute, EventType, Key, Relative};
use crate::error::{Error, Result};
use crate::event::{Batch, Reader};
use crate::ioctl;
use crate::sys;

/// The longest device name this crate asks for.
///
/// The kernel truncates to whatever is asked for and reports how much it wrote, so a name longer
/// than this arrives cut rather than lost.
const NAME_LIMIT: usize = 256;

/// How many bytes hold `codes` bits.
const fn bytes_for(codes: u32) -> usize {
    codes.div_ceil(8) as usize
}

/// Which bitmap a device reports its event types in.
const TYPE_BITMAP: usize = bytes_for(sys::EV_CNT);

/// Which bitmap a device reports its key codes in.
const KEY_BITMAP: usize = bytes_for(sys::KEY_CNT);

/// Which bitmap a device reports its relative axes in.
const RELATIVE_BITMAP: usize = bytes_for(sys::REL_CNT);

/// Which bitmap a device reports its absolute axes in.
const ABSOLUTE_BITMAP: usize = bytes_for(sys::ABS_CNT);

/// A set of codes, as the kernel writes one.
///
/// The kernel answers "which codes does this device have" with a bitmap: bit `n` is code `n`. A
/// map is as long as the caller asked for, so a code past its end is a code the kernel had no room
/// to report and reads as absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bitmap {
    /// The bits, least significant first, as the kernel wrote them.
    bits: Vec<u8>,
}

impl Bitmap {
    /// Returns the bitmap these bytes hold.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bits: bytes.to_vec(),
        }
    }

    /// Returns the bitmap holding exactly `codes`.
    ///
    /// This is how a test states a device's capabilities without a device, and how a caller
    /// describes one it is about to create.
    pub fn from_codes(codes: impl IntoIterator<Item = u16>) -> Self {
        let mut bits: Vec<u8> = Vec::new();
        for code in codes {
            let byte = usize::from(code) / 8;
            if bits.len() <= byte {
                bits.resize(byte + 1, 0);
            }
            bits[byte] |= 1 << (code % 8);
        }
        Self { bits }
    }

    /// Returns `true` if `code` is in this map.
    pub fn contains(&self, code: u16) -> bool {
        let byte = usize::from(code) / 8;
        self.bits
            .get(byte)
            .is_some_and(|bits| bits & (1 << (code % 8)) != 0)
    }

    /// Returns every code in this map, in order.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        self.bits.iter().enumerate().flat_map(|(byte, bits)| {
            (0..8)
                .filter(move |bit| bits & (1 << bit) != 0)
                .map(move |bit| u16::try_from(byte * 8 + bit).expect("a code fits in sixteen bits"))
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
    types: Bitmap,
    /// Which keys and buttons it has.
    keys: Bitmap,
    /// Which relative axes it has.
    relative: Bitmap,
    /// Which absolute axes it has.
    absolute: Bitmap,
}

impl Capabilities {
    /// The capabilities these four maps describe.
    pub fn new(types: Bitmap, keys: Bitmap, relative: Bitmap, absolute: Bitmap) -> Self {
        Self {
            types,
            keys,
            relative,
            absolute,
        }
    }

    /// Returns which event types the device emits.
    pub fn types(&self) -> &Bitmap {
        &self.types
    }

    /// Returns which keys and buttons it has.
    pub fn keys(&self) -> &Bitmap {
        &self.keys
    }

    /// Returns which relative axes it has.
    pub fn relative(&self) -> &Bitmap {
        &self.relative
    }

    /// Returns which absolute axes it has.
    pub fn absolute(&self) -> &Bitmap {
        &self.absolute
    }

    /// Returns `true` if the device emits `kind` at all.
    pub fn has(&self, kind: EventType) -> bool {
        self.types.contains(kind.raw())
    }

    /// Returns the jobs these capabilities amount to.
    ///
    /// Each answer is a question about the codes. The types alone put every mouse among the
    /// keyboards, because a mouse has `EV_KEY` for its buttons.
    ///
    /// - A keyboard has a key in the kernel's first block of keys, `KEY_RESERVED` aside. See
    ///   [`Key::is_keyboard_key`].
    /// - A pointer has both `REL_X` and `REL_Y`. A device with a wheel and no axes is a keyboard
    ///   with a wheel on it, and there are several.
    /// - A touch device has both `ABS_X` and `ABS_Y`, or the multi-touch pair. A volume dial
    ///   reports an absolute axis too, and it is not a touchscreen.
    pub fn roles(&self) -> Roles {
        Roles {
            // `KEY_RESERVED` is code zero and means nothing. A driver that sets its bit would
            // otherwise make a device with one meaningless code into a keyboard, and udev's own
            // rule leaves it out for the same reason.
            keyboard: self.has(EventType::EV_KEY)
                && self.keys.iter().any(|code| {
                    code != Key::KEY_RESERVED.raw() && Key::new(code).is_keyboard_key()
                }),
            pointer: self.has(EventType::EV_REL)
                && self.relative.contains(Relative::REL_X.raw())
                && self.relative.contains(Relative::REL_Y.raw()),
            touch: self.has(EventType::EV_ABS)
                && ((self.absolute.contains(Absolute::ABS_X.raw())
                    && self.absolute.contains(Absolute::ABS_Y.raw()))
                    || (self.absolute.contains(Absolute::ABS_MT_POSITION_X.raw())
                        && self.absolute.contains(Absolute::ABS_MT_POSITION_Y.raw()))),
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

        let name = read_name(fd.as_fd())?;
        let identity = read_identity(fd.as_fd())?;
        let capabilities = read_capabilities(fd.as_fd())?;

        Ok(Self {
            fd,
            path,
            name,
            identity,
            capabilities,
            grabbed: false,
            reader: Reader::new(),
        })
    }

    /// Returns where this device was opened from.
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

    /// Returns the range and the current reading of one absolute axis.
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

    /// Which keys are held down right now.
    ///
    /// This is the state a caller needs when it opens a device that is already in use: a modifier
    /// held when the first event arrives was pressed before anything was listening, and nothing
    /// later in the stream says so.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses the query.
    pub fn pressed_keys(&self) -> Result<Bitmap> {
        let mut bits = [0_u8; KEY_BITMAP];
        let written = ioctl::issue_bytes(self.fd(), ioctl::key_state(), &mut bits)?;
        Ok(Bitmap::from_bytes(&bits[..written.min(KEY_BITMAP)]))
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

/// Asks the device what it calls itself.
fn read_name(fd: BorrowedFd<'_>) -> Result<String> {
    let mut bytes = [0_u8; NAME_LIMIT];
    let written = ioctl::issue_bytes(fd, ioctl::name(), &mut bytes)?;
    let written = written.min(NAME_LIMIT);
    // The kernel counts the terminator in what it wrote, and a driver may write a shorter string
    // than it claims, so the name ends at the first zero either way.
    let text = &bytes[..written];
    let end = text.iter().position(|byte| *byte == 0).unwrap_or(written);
    // A name that is not UTF-8 is a device with an odd descriptor rather than a device to refuse:
    // it still reports keys. The replacement character says so where a person reads it.
    Ok(String::from_utf8_lossy(&text[..end]).into_owned())
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
    let types = read_bits(fd, 0, TYPE_BITMAP)?;

    let map = |kind: EventType, len: usize| -> Result<Bitmap> {
        if types.contains(kind.raw()) {
            read_bits(fd, kind.raw(), len)
        } else {
            Ok(Bitmap::default())
        }
    };

    let keys = map(EventType::EV_KEY, KEY_BITMAP)?;
    let relative = map(EventType::EV_REL, RELATIVE_BITMAP)?;
    let absolute = map(EventType::EV_ABS, ABSOLUTE_BITMAP)?;

    Ok(Capabilities::new(types, keys, relative, absolute))
}

/// Reads one `EVIOCGBIT` map of `len` bytes.
fn read_bits(fd: BorrowedFd<'_>, kind: u16, len: usize) -> Result<Bitmap> {
    let mut bits = vec![0_u8; len];
    let written = ioctl::issue_bytes(fd, ioctl::bits(kind)?, &mut bits)?;
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
            Bitmap::from_codes(types.iter().map(|kind| kind.raw())),
            Bitmap::from_codes(keys.iter().map(|key| key.raw())),
            Bitmap::from_codes(relative.iter().map(|axis| axis.raw())),
            Bitmap::from_codes(absolute.iter().map(|axis| axis.raw())),
        )
    }

    #[test]
    fn a_bitmap_holds_the_codes_it_was_built_from() {
        let map = Bitmap::from_codes([Key::KEY_A.raw(), Key::KEY_Z.raw(), Key::BTN_LEFT.raw()]);

        assert!(map.contains(Key::KEY_A.raw()));
        assert!(map.contains(Key::BTN_LEFT.raw()));
        assert!(!map.contains(Key::KEY_B.raw()));
        assert_eq!(map.len(), 3);
        assert!(!map.is_empty());
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            [Key::KEY_A.raw(), Key::KEY_Z.raw(), Key::BTN_LEFT.raw()],
            "the codes come back in order, whatever order they went in"
        );
    }

    #[test]
    fn a_code_past_the_end_of_a_bitmap_is_absent_rather_than_a_panic() {
        // The kernel fills as much of a map as was asked for. A caller that asked for two bytes
        // and then asks about code 700 gets an answer rather than an index out of range.
        let map = Bitmap::from_bytes(&[0b0000_0011, 0]);

        assert!(map.contains(0));
        assert!(map.contains(1));
        assert!(!map.contains(700));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn an_empty_bitmap_holds_nothing() {
        assert!(Bitmap::default().is_empty());
        assert!(
            Bitmap::from_bytes(&[0, 0, 0]).is_empty(),
            "bytes the kernel wrote as zero are no codes at all"
        );
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
    fn a_bitmap_is_as_many_bytes_as_the_codes_need() {
        assert_eq!(TYPE_BITMAP, 4, "thirty-two event types");
        assert_eq!(KEY_BITMAP, 96, "seven hundred and sixty-eight key codes");
        assert_eq!(RELATIVE_BITMAP, 2, "sixteen relative axes");
        assert_eq!(ABSOLUTE_BITMAP, 8, "sixty-four absolute axes");
    }
}
