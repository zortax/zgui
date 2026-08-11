//! One libinput context: the descriptor to wait on, and the call that reads it.
//!
//! A context is where the devices are gathered and where their events come out. This is the path
//! backend, which reads the devices its caller names. libinput's other backend takes a
//! `struct udev *` and finds them itself, and that would mean opening libudev here for a walk this
//! crate's caller already does.
//!
//! [`Context::open`] makes one. [`Context::descriptor`] is what a loop waits on, and
//! [`Context::dispatch`] is what reads it. Dropping the context gives every device back.
//!
//! # Threads
//!
//! libinput reads its devices on the thread that calls it, holds no lock of its own, and calls back
//! into its caller from inside those calls. So the context stays on the thread that made it.

pub mod files;

use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char};
use std::marker::PhantomData;
use std::os::fd::{BorrowedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::Duration;

use crate::device::{Capabilities, Capability, Device, DeviceId};
use crate::error::{Error, Result};
use crate::event::{Event, Press, Scrolled};
use crate::library::{
    Libinput, LibinputDevice, LibinputEvent, LibinputPointerEvent, Library, Symbols,
};

pub use crate::context::files::Files;

use crate::context::files::{Callers, INTERFACE};

/// libinput's `LIBINPUT_EVENT_DEVICE_ADDED`.
const DEVICE_ADDED: u32 = 1;

/// libinput's `LIBINPUT_EVENT_DEVICE_REMOVED`.
const DEVICE_REMOVED: u32 = 2;

/// libinput's `LIBINPUT_EVENT_KEYBOARD_KEY`.
const KEYBOARD_KEY: u32 = 300;

/// libinput's `LIBINPUT_EVENT_POINTER_MOTION`.
const POINTER_MOTION: u32 = 400;

/// libinput's `LIBINPUT_EVENT_POINTER_MOTION_ABSOLUTE`.
const POINTER_MOTION_ABSOLUTE: u32 = 401;

/// libinput's `LIBINPUT_EVENT_POINTER_BUTTON`.
const POINTER_BUTTON: u32 = 402;

/// libinput's `LIBINPUT_EVENT_POINTER_SCROLL_WHEEL`.
const POINTER_SCROLL_WHEEL: u32 = 404;

/// libinput's `LIBINPUT_EVENT_POINTER_SCROLL_FINGER`.
const POINTER_SCROLL_FINGER: u32 = 405;

/// libinput's `LIBINPUT_EVENT_POINTER_SCROLL_CONTINUOUS`.
const POINTER_SCROLL_CONTINUOUS: u32 = 406;

/// libinput's `LIBINPUT_POINTER_AXIS_SCROLL_VERTICAL`.
const VERTICAL: u32 = 0;

/// libinput's `LIBINPUT_POINTER_AXIS_SCROLL_HORIZONTAL`.
const HORIZONTAL: u32 = 1;

/// The width an absolute position is asked for in.
///
/// libinput scales the device's own range into whatever width the caller names, so a width of one
/// answers the position as a fraction. A fraction is what leaves this crate: how wide the screen is
/// belongs to whoever draws on it.
const AS_A_FRACTION: u32 = 1;

/// How much libinput reports about itself, through its own log.
///
/// The log goes to standard error. It carries what libinput decided about a device and why it
/// refused one, and it is the first thing to read when a device behaves unlike the same device
/// under another desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Loudness {
    /// Everything, including what each device was probed for.
    Debug,
    /// What libinput decided about each device.
    Information,
    /// What went wrong. libinput's own default.
    Errors,
}

impl Loudness {
    /// Returns the number libinput knows this by.
    const fn as_raw(self) -> u32 {
        match self {
            Self::Debug => 10,
            Self::Information => 20,
            Self::Errors => 30,
        }
    }
}

/// The name every evdev node starts with.
///
/// `/dev/input` holds more than the event nodes — `mice`, `mouse0`, `js0` — and libinput reads none
/// of them. All three are character devices, so libinput refuses one silently.
const NODE: &str = "event";

/// One node this context has been given.
#[derive(Debug)]
struct Node {
    /// Where it was added from.
    path: PathBuf,
    /// What it is called in `/sys/class/input`, read when it was added.
    ///
    /// The name matches a device libinput reports to the node it came from. libinput hands back a
    /// different device for the same node after a resume, so the address cannot make that match and
    /// the name can.
    sysname: String,
    /// The device libinput is reading it as, while there is one.
    ///
    /// Empty between a suspend and its resume, and between a node being added and the arrival that
    /// follows.
    live: Option<Live>,
}

/// A device libinput is reading.
#[derive(Debug)]
struct Live {
    /// libinput's own device, with one reference held here.
    ///
    /// The reference makes the pointer safe to keep. Without it libinput can free the device while
    /// a removal is still queued, and a call made afterwards would reach freed memory.
    raw: NonNull<LibinputDevice>,
    /// What was copied out of it when it arrived.
    device: Device,
}

/// libinput, with the devices one caller has given it.
pub struct Context {
    /// The open library. Every symbol called below is an address inside it, so the mapping stands
    /// for as long as this context lives.
    library: Library,
    /// The context itself.
    raw: NonNull<Libinput>,
    /// What the two callbacks reach the caller through.
    ///
    /// Leaked rather than held in a box, because libinput carries its address and calls back
    /// through it from inside calls this context makes. A box would be a second owner of the same
    /// value, reached with `&mut` while libinput held the address. [`Drop`] reclaims the address,
    /// after libinput has been freed.
    callers: NonNull<Callers>,
    /// The descriptor to wait on, read once when the context was made.
    descriptor: RawFd,
    /// Every node this context has been given, and the device libinput reads each one as.
    nodes: Vec<Node>,
    /// What has been read out of libinput's queue and not yet taken by the caller.
    ///
    /// libinput's queue is drained into this one at the moment the state that classifies it is
    /// right. A suspend and the resume that follows it can both happen before a caller reads
    /// anything, and reading libinput's queue afterwards could no longer tell which removal was
    /// which.
    pending: VecDeque<Event>,
    /// The next device id to give out.
    next: u64,
    /// What makes this type `!Send` and `!Sync`.
    ///
    /// The raw pointers do this as well. The marker states it, so that a field which becomes
    /// something shareable cannot make the whole type shareable with it.
    thread_bound: PhantomData<*const ()>,
}

impl Context {
    /// Opens libinput and makes a context that reads the devices it is given.
    ///
    /// ```no_run
    /// use std::os::fd::OwnedFd;
    /// use std::os::unix::fs::OpenOptionsExt;
    /// use std::path::Path;
    ///
    /// use zgui_libinput::{Context, Files};
    ///
    /// // A caller that opens the nodes itself.
    /// struct Own;
    ///
    /// impl Files for Own {
    ///     fn open(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
    ///         std::fs::OpenOptions::new()
    ///             .read(true)
    ///             .custom_flags(flags)
    ///             .open(path)
    ///             .map(OwnedFd::from)
    ///             .map_err(|error| error.raw_os_error().unwrap_or(5))
    ///     }
    ///
    ///     fn close(&mut self, fd: OwnedFd) {
    ///         drop(fd);
    ///     }
    /// }
    ///
    /// let mut files = Own;
    /// let mut context = Context::open()?;
    /// assert!(context.add(&mut files, Path::new("/dev/input/event0")));
    ///
    /// context.dispatch(&mut files)?;
    /// while let Some(event) = context.next_event() {
    ///     println!("{event:?}");
    /// }
    /// # Ok::<(), zgui_libinput::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] or [`Error::Symbol`] when libinput cannot be opened,
    /// [`Error::Context`] when libinput would not make a context, and [`Error::Descriptor`] when
    /// the context it made has nothing to wait on.
    pub fn open() -> Result<Self> {
        Self::over(Library::load()?)
    }

    /// Makes a context over a library the caller opened.
    ///
    /// A caller that has already asked whether libinput is on the machine avoids opening it twice
    /// this way.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Context`] when libinput would not make a context, and
    /// [`Error::Descriptor`] when the context it made has nothing to wait on.
    pub fn over(library: Library) -> Result<Self> {
        let callers = NonNull::from(Box::leak(Box::new(Callers::new())));

        // SAFETY: `INTERFACE` is a `static`, so the pointer libinput keeps stays valid for the
        // whole program. `callers` is a leaked box, which stays where it is until `Drop` reclaims
        // it — after libinput has been freed and can no longer read it.
        let raw = unsafe {
            (library.symbols().path_create_context)(&raw const INTERFACE, callers.as_ptr().cast())
        };
        let Some(raw) = NonNull::new(raw) else {
            // SAFETY: the box leaked above, reclaimed here because no context was made and nothing
            // else holds the address.
            drop(unsafe { Box::from_raw(callers.as_ptr()) });
            return Err(Error::Context);
        };

        // SAFETY: `raw` is a context libinput just made and nothing has freed it.
        let descriptor = unsafe { (library.symbols().get_fd)(raw.as_ptr()) };
        if descriptor < 0 {
            // SAFETY: as above, and the context is unreachable after this function returns. It is
            // freed before the box, because freeing it is what stops libinput reading the box.
            unsafe { (library.symbols().unref)(raw.as_ptr()) };
            // SAFETY: as the reclaim above.
            drop(unsafe { Box::from_raw(callers.as_ptr()) });
            return Err(Error::Descriptor);
        }

        Ok(Self {
            library,
            raw,
            callers,
            descriptor,
            nodes: Vec::new(),
            pending: VecDeque::new(),
            next: 0,
            thread_bound: PhantomData,
        })
    }

    /// Sets how much libinput reports about itself.
    ///
    /// libinput writes to standard error, which is where its own default sends it. A run that wants
    /// those lines redirects that descriptor: on a console the terminal has been drawn over by
    /// then, so the lines land where nobody can read them.
    ///
    /// The handler libinput can take instead is left alone. It is given the context and no
    /// `user_data`, so reaching the caller from inside it needs a map from contexts to callers held
    /// outside the context, and the message is a C format string with a `va_list`, which only the C
    /// library's own `vsnprintf` can read. Redirecting a descriptor answers the same question in
    /// the shell.
    pub fn loudness(&mut self, loudness: Loudness) {
        let set = self.library.symbols().log_set_priority;
        // SAFETY: `raw` is this context, and the number is one libinput knows.
        unsafe { set(self.raw.as_ptr(), loudness.as_raw()) };
    }

    /// Returns the descriptor a loop waits on.
    ///
    /// It becomes readable when a device has reported something. [`Context::dispatch`] is what
    /// reads it, and nothing else may: libinput owns what is on it.
    pub fn descriptor(&self) -> BorrowedFd<'_> {
        // SAFETY: the descriptor is libinput's, read when the context was made, and libinput keeps
        // it open for as long as the context lives. The borrow says so: it cannot outlive `self`.
        unsafe { BorrowedFd::borrow_raw(self.descriptor) }
    }

    /// Gives libinput one device to read, and says whether it took it.
    ///
    /// The device arrives as [`Event::DeviceAdded`] rather than here, because that is also how it
    /// arrives after a resume, and a caller that reads it in one place reads it in every case.
    ///
    /// Four things are refused without asking libinput: a node this context already holds, a name
    /// that is not an evdev node's, a path that is not a character device, and a path with a zero
    /// byte in it. The first would give two live devices for one node, and every keystroke twice.
    /// libinput draws one `client bug: Invalid path` line for a path that is not a character
    /// device, and it refuses a character device that is not evdev silently, so both are settled
    /// here. A path with a zero byte would reach libinput cut short.
    ///
    /// A node this process may not open is refused quietly: [`Files::open`] says no, and libinput
    /// answers null.
    #[must_use]
    pub fn add(&mut self, files: &mut impl Files, path: &Path) -> bool {
        if self.nodes.iter().any(|node| node.path == path) {
            return false;
        }
        if !is_a_node(path) {
            return false;
        }
        let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
            // A path with a zero byte in it would arrive cut short and open something nobody asked
            // for.
            return false;
        };

        let add = self.library.symbols().path_add_device;
        // SAFETY: `raw` is this context, `name` is a C string that lives across the call, and
        // `files` is reachable for exactly this call — which is when libinput opens the node.
        let added = self.lending(files, |raw| unsafe { add(raw, name.as_ptr()) });

        let Some(added) = NonNull::new(added) else {
            return false;
        };

        // The sysname is read here rather than at the arrival, because this is the one moment the
        // node and the device libinput made for it are both in hand.
        let sysname = text(self.library.symbols().device_get_sysname, added);
        self.nodes.push(Node {
            path: path.to_owned(),
            sysname,
            live: None,
        });
        true
    }

    /// Takes one device away from libinput.
    ///
    /// It leaves as [`Event::DeviceRemoved`], and the node is forgotten with it, so the same path
    /// can be added again.
    pub fn remove(&mut self, files: &mut impl Files, device: DeviceId) {
        let Some(node) = self.nodes.iter().find(|node| {
            node.live
                .as_ref()
                .is_some_and(|live| live.device.id() == device)
        }) else {
            return;
        };
        let Some(live) = node.live.as_ref() else {
            return;
        };
        let raw = live.raw;

        let remove = self.library.symbols().path_remove_device;
        // SAFETY: `raw` is a device this context holds a reference to, so it is alive. `files` is
        // reachable for the call, which is when libinput hands the descriptor back.
        self.lending(files, |_| unsafe { remove(raw.as_ptr()) });
        self.drain(false);
    }

    /// Returns what libinput says about one device it is reading.
    #[must_use]
    pub fn device(&self, device: DeviceId) -> Option<&Device> {
        self.nodes
            .iter()
            .filter_map(|node| node.live.as_ref())
            .map(|live| &live.device)
            .find(|held| held.id() == device)
    }

    /// Returns every device libinput is reading, in the order the nodes were added.
    pub fn devices(&self) -> impl Iterator<Item = &Device> {
        self.nodes
            .iter()
            .filter_map(|node| node.live.as_ref())
            .map(|live| &live.device)
    }

    /// Turns tap-to-click on, where the device has it.
    ///
    /// libinput's own default is off, and a person who taps a touchpad expects a click. Nothing
    /// else about a device is set here: which acceleration curve a person wants is a preference,
    /// and this crate can read none.
    ///
    /// A device with no taps to configure — every mouse, every keyboard — is left alone, and so is
    /// one that refuses. Both answer `false`.
    pub fn tap_to_click(&mut self, device: DeviceId) -> bool {
        let symbols = *self.library.symbols();
        let Some(live) = self
            .nodes
            .iter()
            .filter_map(|node| node.live.as_ref())
            .find(|live| live.device.id() == device)
        else {
            return false;
        };

        // SAFETY: `raw` is a device this context holds a reference to, so it is alive.
        let fingers = unsafe { (symbols.device_config_tap_get_finger_count)(live.raw.as_ptr()) };
        if fingers <= 0 {
            return false;
        }
        // SAFETY: as above, and `1` is libinput's own `ENABLED`.
        let answered = unsafe { (symbols.device_config_tap_set_enabled)(live.raw.as_ptr(), 1) };
        // `0` is libinput's `SUCCESS`.
        answered == 0
    }

    /// Closes every device and stops reading, keeping the nodes.
    ///
    /// This is what a session does when somebody switches to another terminal. Each device is
    /// handed back through [`Files::close`] and reported as [`Event::DeviceRemoved`], and the paths
    /// stay so that [`Context::resume`] can open them again.
    pub fn suspend(&mut self, files: &mut impl Files) {
        // Whatever is already queued is classified before the suspend queues anything of its own,
        // so a device that went away by itself is not read as one this call took.
        self.drain(false);

        let suspend = self.library.symbols().suspend;
        // SAFETY: `raw` is this context, and `files` is reachable for the call, which is when
        // libinput hands every descriptor back.
        self.lending(files, |raw| unsafe { suspend(raw) });

        // Classified as this call's removals, so the nodes are kept.
        self.drain(true);
    }

    /// Opens every node again and starts reading.
    ///
    /// Each one is opened through [`Files::open`] and reported as [`Event::DeviceAdded`], with a
    /// new [`DeviceId`]. A node that does not open is forgotten, so that a device unplugged while
    /// the session was away does not stay in the way of the same node arriving again.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Resume`] when libinput refused. The nodes are kept, so another resume can
    /// be asked for.
    pub fn resume(&mut self, files: &mut impl Files) -> Result<()> {
        self.drain(false);

        let resume = self.library.symbols().resume;
        // SAFETY: `raw` is this context, and `files` is reachable for the call, which is when
        // libinput asks for every node again.
        let answered = self.lending(files, |raw| unsafe { resume(raw) });
        if answered != 0 {
            return Err(Error::Resume);
        }

        self.drain(false);

        // A node with nothing live is one libinput did not open again. libinput has already
        // forgotten it, so holding it here would only keep the path from being added again.
        self.nodes.retain(|node| node.live.is_some());
        Ok(())
    }

    /// Returns the next thing a device did, or nothing.
    ///
    /// [`Context::dispatch`] makes events available. This takes them one at a time. With nothing
    /// left over from an earlier call, it first reads everything libinput has queued, and it
    /// answers nothing when there is no more to read.
    pub fn next_event(&mut self) -> Option<Event> {
        if self.pending.is_empty() {
            self.drain(false);
        }
        self.pending.pop_front()
    }

    /// Reads what the devices have reported and turns it into events.
    ///
    /// `files` is lent to libinput for this call, because a device that stops answering is dropped
    /// inside it and given back through [`Files::close`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Dispatch`] when libinput could not read its devices.
    pub fn dispatch(&mut self, files: &mut impl Files) -> Result<()> {
        let dispatch = self.library.symbols().dispatch;
        // SAFETY: `raw` is this context, and `files` is reachable for exactly this call — which is
        // the only span in which libinput makes a callback out of it.
        let answered = self.lending(files, |raw| unsafe { dispatch(raw) });

        if answered < 0 {
            return Err(Error::Dispatch { errno: -answered });
        }
        Ok(())
    }

    /// Reads everything libinput has queued, and classifies it with the state as it is now.
    ///
    /// `suspending` says that the removals about to be read are [`Context::suspend`]'s own, so the
    /// nodes are kept rather than forgotten.
    fn drain(&mut self, suspending: bool) {
        let symbols = *self.library.symbols();
        loop {
            // SAFETY: `raw` is this context. The event that comes back belongs to this caller until
            // it is destroyed, which is the last thing done with it below.
            let taken = unsafe { (symbols.get_event)(self.raw.as_ptr()) };
            let Some(event) = NonNull::new(taken) else {
                return;
            };

            // SAFETY: `event` is an event libinput just handed over and nothing has destroyed it.
            let kind = unsafe { (symbols.event_get_type)(event.as_ptr()) };
            match kind {
                DEVICE_ADDED => self.arrived(&symbols, event),
                DEVICE_REMOVED => self.went(&symbols, event, suspending),
                KEYBOARD_KEY => self.struck(&symbols, event),
                POINTER_MOTION
                | POINTER_MOTION_ABSOLUTE
                | POINTER_BUTTON
                | POINTER_SCROLL_WHEEL
                | POINTER_SCROLL_FINGER
                | POINTER_SCROLL_CONTINUOUS => self.pointed(&symbols, event, kind),
                // Touch, gestures, tablets, switches — and `POINTER_AXIS`, which libinput reports
                // beside each scroll above for callers written before the three of them existed.
                // Reading both would scroll twice as far as the wheel was turned. Destroying the
                // event is all that is owed for any of them.
                _ => {}
            }

            // SAFETY: as above, and nothing built from it outlives this line: every reading taken
            // above was copied into a value of this crate's own.
            unsafe { (symbols.event_destroy)(event.as_ptr()) };
        }
    }

    /// Records a device libinput has started reading.
    fn arrived(&mut self, symbols: &Symbols, event: NonNull<LibinputEvent>) {
        // SAFETY: `event` is a live event, and the device is borrowed from it.
        let raw = unsafe { (symbols.event_get_device)(event.as_ptr()) };
        let Some(raw) = NonNull::new(raw) else {
            return;
        };

        let sysname = text(symbols.device_get_sysname, raw);
        let Some(at) = self.nodes.iter().position(|node| node.sysname == sysname) else {
            // A device for a node this context never gave libinput. There is nothing to route it
            // to, so it is left where it is.
            return;
        };

        // SAFETY: the device is alive, because the event that carries it is. This reference is what
        // keeps it alive after the event is destroyed, and it is released in `release`.
        unsafe { (symbols.device_ref)(raw.as_ptr()) };

        let id = DeviceId::new(self.next);
        self.next += 1;

        let device = Device::new(
            id,
            self.nodes[at].path.clone(),
            text(symbols.device_get_name, raw),
            sysname,
            // SAFETY: the device is alive and these read it.
            unsafe { (symbols.device_get_id_vendor)(raw.as_ptr()) },
            // SAFETY: as above.
            unsafe { (symbols.device_get_id_product)(raw.as_ptr()) },
            capabilities(symbols, raw),
        );

        // A node that somehow still holds a device is one whose removal was never reported. The
        // reference is released rather than dropped, because dropping it would leak the device.
        if let Some(stale) = self.nodes[at].live.take() {
            release(symbols, stale);
        }
        self.nodes[at].live = Some(Live {
            raw,
            device: device.clone(),
        });
        self.pending.push_back(Event::DeviceAdded(device));
    }

    /// Records a device libinput has stopped reading.
    fn went(&mut self, symbols: &Symbols, event: NonNull<LibinputEvent>, suspending: bool) {
        // SAFETY: as `arrived`.
        let raw = unsafe { (symbols.event_get_device)(event.as_ptr()) };
        let Some(raw) = NonNull::new(raw) else {
            return;
        };

        let sysname = text(symbols.device_get_sysname, raw);
        let Some(at) = self.nodes.iter().position(|node| node.sysname == sysname) else {
            return;
        };

        if let Some(live) = self.nodes[at].live.take() {
            let device = live.device.clone();
            release(symbols, live);
            self.pending.push_back(Event::DeviceRemoved(device));
        }

        // A suspend keeps its nodes, because a resume opens every one of them again. Anything else
        // is a device that has gone, and holding its node would keep the same path from ever being
        // added again.
        if !suspending {
            self.nodes.remove(at);
        }
    }

    /// Reads one key.
    fn struck(&mut self, symbols: &Symbols, event: NonNull<LibinputEvent>) {
        let Some(device) = self.reporting(symbols, event) else {
            return;
        };
        // SAFETY: the event is live and its type is `KEYBOARD_KEY`, so reading it as a keyboard
        // event is the right question. libinput answers null for any other type.
        let keyboard = unsafe { (symbols.event_get_keyboard_event)(event.as_ptr()) };
        let Some(keyboard) = NonNull::new(keyboard) else {
            return;
        };

        self.pending.push_back(Event::Key {
            device,
            // SAFETY: `keyboard` is this event read as a keyboard event, and these three read it.
            key: unsafe { (symbols.keyboard_get_key)(keyboard.as_ptr()) },
            // SAFETY: as above.
            press: Press::of(unsafe { (symbols.keyboard_get_key_state)(keyboard.as_ptr()) }),
            // SAFETY: as above.
            at: Duration::from_micros(unsafe {
                (symbols.keyboard_get_time_usec)(keyboard.as_ptr())
            }),
        });
    }

    /// Reads one thing done with a pointing device.
    fn pointed(&mut self, symbols: &Symbols, event: NonNull<LibinputEvent>, kind: u32) {
        let Some(device) = self.reporting(symbols, event) else {
            return;
        };
        // SAFETY: the event is live and its type is one of the pointer kinds, so reading it as a
        // pointer event is the right question.
        let pointer = unsafe { (symbols.event_get_pointer_event)(event.as_ptr()) };
        let Some(pointer) = NonNull::new(pointer) else {
            return;
        };
        let raw = pointer.as_ptr();

        // SAFETY: `pointer` is this event read as a pointer event, and this reads it. Every other
        // call below is the same.
        let at = Duration::from_micros(unsafe { (symbols.pointer_get_time_usec)(raw) });

        let read = match kind {
            POINTER_MOTION => Event::Motion {
                device,
                // SAFETY: as above.
                dx: unsafe { (symbols.pointer_get_dx)(raw) },
                // SAFETY: as above.
                dy: unsafe { (symbols.pointer_get_dy)(raw) },
                at,
            },
            POINTER_MOTION_ABSOLUTE => Event::MotionAbsolute {
                device,
                // SAFETY: as above.
                x: unsafe { (symbols.pointer_get_absolute_x_transformed)(raw, AS_A_FRACTION) },
                // SAFETY: as above.
                y: unsafe { (symbols.pointer_get_absolute_y_transformed)(raw, AS_A_FRACTION) },
                at,
            },
            POINTER_BUTTON => Event::Button {
                device,
                // SAFETY: as above.
                button: unsafe { (symbols.pointer_get_button)(raw) },
                // SAFETY: as above.
                press: Press::of(unsafe { (symbols.pointer_get_button_state)(raw) }),
                at,
            },
            _ => {
                let source = match kind {
                    POINTER_SCROLL_FINGER => Scrolled::Finger,
                    POINTER_SCROLL_CONTINUOUS => Scrolled::Continuous,
                    _ => Scrolled::Wheel,
                };
                Event::Scroll {
                    device,
                    source,
                    vertical: scrolled(symbols, pointer, source, VERTICAL),
                    horizontal: scrolled(symbols, pointer, source, HORIZONTAL),
                    at,
                }
            }
        };
        self.pending.push_back(read);
    }

    /// Returns which of this context's devices an event came from.
    ///
    /// An event from a device this context is not holding is one nothing can be filed under, so it
    /// is dropped. A device arriving out of order looks like this. Reporting the event under
    /// another device's id would be worse.
    fn reporting(&self, symbols: &Symbols, event: NonNull<LibinputEvent>) -> Option<DeviceId> {
        // SAFETY: `event` is a live event, and the device is borrowed from it.
        let raw = unsafe { (symbols.event_get_device)(event.as_ptr()) };
        let raw = NonNull::new(raw)?;
        let sysname = text(symbols.device_get_sysname, raw);

        self.nodes
            .iter()
            .filter_map(|node| node.live.as_ref())
            .find(|live| live.device.sysname() == sysname)
            .map(|live| live.device.id())
    }

    /// Runs one call into libinput with `files` reachable from the two callbacks.
    ///
    /// Every call libinput can open or close a device from goes through here, and no other call
    /// does. `body` is handed the context alone, so nothing inside it reaches the caller except
    /// through the callbacks.
    fn lending<R>(&self, files: &mut impl Files, body: impl FnOnce(*mut Libinput) -> R) -> R {
        /// Takes the caller away again however the call ends.
        ///
        /// The lent pointer is only valid while the borrow it was made from is, and that borrow
        /// ends when this function returns — by any route. A line after the call would be skipped
        /// by an unwind and leave a pointer to a caller that is gone.
        struct Until<'a>(&'a Callers);

        impl Drop for Until<'_> {
            fn drop(&mut self) {
                self.0.take_back();
            }
        }

        // SAFETY: the leaked box, which lives until this context is dropped. Every reference taken
        // to it is shared, here and in the callbacks.
        let callers = unsafe { self.callers.as_ref() };

        callers.lend(files);
        let _until = Until(callers);

        body(self.raw.as_ptr())
    }
}

impl Drop for Context {
    /// Frees the context, which gives every device it still holds back.
    ///
    /// Nothing is lent here, so each descriptor is closed rather than handed to a caller that is no
    /// longer in a call. A session daemon is therefore not told, and it learns the same thing when
    /// the seat it opened the devices on closes.
    fn drop(&mut self) {
        let symbols = *self.library.symbols();

        // The reference each live device was given when it arrived. libinput holds its own, so
        // releasing these frees nothing yet; leaving them would keep every device alive after the
        // context that owns them has gone.
        for node in &mut self.nodes {
            if let Some(live) = node.live.take() {
                release(&symbols, live);
            }
        }

        // SAFETY: `raw` is this context and this is the only place it is freed, in the drop of the
        // one value that owns it. Nothing calls through it afterwards.
        unsafe { (symbols.unref)(self.raw.as_ptr()) };

        // SAFETY: the box leaked in `over`, reclaimed once, here. libinput is freed, so nothing
        // holds the address any more — including the `close_restricted` calls the free itself
        // made, which have already returned.
        drop(unsafe { Box::from_raw(self.callers.as_ptr()) });
    }
}

/// Returns `true` when a path is one libinput reads.
///
/// `/dev/input` holds `mice`, `mouse0` and `js0` beside the event nodes, and a caller that walks the
/// directory meets all of them. libinput draws one `client bug: Invalid path` line for anything
/// that is not a character device, and a character device that is not evdev it opens through
/// [`Files::open`] and then refuses silently. Both are settled here instead.
fn is_a_node(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !name.starts_with(NODE) {
        return false;
    }
    std::fs::metadata(path).is_ok_and(|about| about.file_type().is_char_device())
}

/// Reads one of libinput's strings about a device.
///
/// The string belongs to libinput, so it is copied. A device that answers nothing gives an empty
/// name rather than a failure: a run works whatever a device is called.
fn text(
    read: unsafe extern "C" fn(*mut LibinputDevice) -> *const c_char,
    device: NonNull<LibinputDevice>,
) -> String {
    // SAFETY: `device` is alive, and `read` is one of the two rows that answer a string about one.
    let raw = unsafe { read(device.as_ptr()) };
    if raw.is_null() {
        return String::new();
    }
    // SAFETY: libinput's own string, which stays valid while the device does, and is copied here
    // rather than kept.
    unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}

/// Asks a device about every capability, one at a time.
fn capabilities(symbols: &Symbols, device: NonNull<LibinputDevice>) -> Capabilities {
    let mut answered = Capabilities::NONE;
    for capability in Capability::EVERY {
        // SAFETY: `device` is alive, and the number is the one libinput knows the capability by.
        let has = unsafe { (symbols.device_has_capability)(device.as_ptr(), capability.as_raw()) };
        if has != 0 {
            answered = answered.with(capability);
        }
    }
    answered
}

/// Returns how far one scroll went along one axis, where it went along it at all.
///
/// An axis this scroll does not carry is absent rather than zero. Zero is what a finger source
/// reports to say that the fingers have stopped, and a kinetic scroll ends on it.
///
/// A wheel is read in one hundred and twentieths of a detent and everything else in pixels. That is
/// the same rule the kernel's own `REL_WHEEL_HI_RES` follows, and the reason is the same: a
/// free-spinning wheel reports fine movement in every update and a whole detent only when it has
/// accumulated one.
fn scrolled(
    symbols: &Symbols,
    pointer: NonNull<LibinputPointerEvent>,
    source: Scrolled,
    axis: u32,
) -> Option<f64> {
    // SAFETY: `pointer` is a live event read as a pointer event, of one of the scroll kinds, and
    // `axis` is one of the two numbers libinput knows.
    if unsafe { (symbols.pointer_has_axis)(pointer.as_ptr(), axis) } == 0 {
        return None;
    }
    let value = match source {
        // SAFETY: as above.
        Scrolled::Wheel => unsafe {
            (symbols.pointer_get_scroll_value_v120)(pointer.as_ptr(), axis)
        },
        // SAFETY: as above.
        Scrolled::Finger | Scrolled::Continuous => unsafe {
            (symbols.pointer_get_scroll_value)(pointer.as_ptr(), axis)
        },
    };
    Some(value)
}

/// Gives back the reference a device was given when it arrived.
fn release(symbols: &Symbols, live: Live) {
    // SAFETY: the reference taken in `arrived`, released once, here. Taking the `Live` by value is
    // what makes that once: there is no second path to the pointer afterwards.
    unsafe { (symbols.device_unref)(live.raw.as_ptr()) };
}

/// Reports the descriptor the context waits on, and none of its addresses.
impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! A context with no devices in it.
    //!
    //! Everything here holds on a machine with libinput and no input device this process may open,
    //! which is the ordinary machine: a context is made, it has a descriptor, and it reads nothing.
    //! What needs a device is tested where the devices are.

    use std::os::fd::{AsRawFd, OwnedFd};
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::library::{INSTALLED_AS, is_on_this_machine};

    /// A caller that opens nothing, and remembers being asked.
    ///
    /// It refuses with `EACCES`, the number a node this process may not open answers with. It
    /// never panics: an unwind out of a [`Files`] call would run into C.
    #[derive(Debug, Default)]
    pub(crate) struct Refusing {
        /// Every path libinput asked for.
        pub(crate) asked: Vec<PathBuf>,
        /// Every descriptor libinput handed back.
        pub(crate) taken: usize,
    }

    impl Files for Refusing {
        fn open(&mut self, path: &Path, _flags: i32) -> std::result::Result<OwnedFd, i32> {
            self.asked.push(path.to_owned());
            // `EACCES`, the number a node this process may not open answers with.
            Err(13)
        }

        fn close(&mut self, fd: OwnedFd) {
            self.taken += 1;
            drop(fd);
        }
    }

    /// Returns `true` when this machine has no libinput, and prints why the test was skipped.
    ///
    /// The precondition is asked of the loader rather than of `Context`, so a `Context` that stops
    /// opening fails its tests instead of skipping them.
    pub(crate) fn without_libinput(test: &str) -> bool {
        if INSTALLED_AS.into_iter().any(is_on_this_machine) {
            return false;
        }
        eprintln!(
            "{test}: this machine has no libinput, so nothing about a context was checked. \
             Install libinput, or run the suite from `nix develop`, which puts `libinput.so.10` on \
             the library path."
        );
        true
    }

    #[test]
    fn the_event_numbers_are_the_ones_the_header_gives() {
        // Written out rather than compared with the constants they check. These come from
        // `libinput.h` by hand, and a wrong one is silent: the kind never matches, the event is
        // dropped as one this crate does not read, and a keyboard simply reports nothing.
        assert_eq!(DEVICE_ADDED, 1);
        assert_eq!(DEVICE_REMOVED, 2);
        assert_eq!(KEYBOARD_KEY, 300);
        assert_eq!(POINTER_MOTION, 400);
        assert_eq!(POINTER_MOTION_ABSOLUTE, 401);
        assert_eq!(POINTER_BUTTON, 402);
        assert_eq!(POINTER_SCROLL_WHEEL, 404);
        assert_eq!(POINTER_SCROLL_FINGER, 405);
        assert_eq!(POINTER_SCROLL_CONTINUOUS, 406);
        assert_eq!(VERTICAL, 0);
        assert_eq!(HORIZONTAL, 1);
    }

    #[test]
    fn the_log_priorities_are_the_ones_the_header_gives() {
        // Written out rather than compared with the arms they check. A wrong number is quiet:
        // libinput reports at whatever volume it reads the number as, and a run asking for
        // everything gets errors alone.
        assert_eq!(Loudness::Debug.as_raw(), 10);
        assert_eq!(Loudness::Information.as_raw(), 20);
        assert_eq!(Loudness::Errors.as_raw(), 30);

        // Louder is a smaller number.
        assert!(Loudness::Debug < Loudness::Errors);
    }

    #[test]
    fn the_deprecated_axis_event_is_not_one_this_crate_reads() {
        // `LIBINPUT_EVENT_POINTER_AXIS` is 403, and libinput reports one beside every scroll above
        // for callers written before the three scroll kinds existed. A `403` in the match would
        // scroll twice as far as the wheel was turned.
        for kind in [
            DEVICE_ADDED,
            DEVICE_REMOVED,
            KEYBOARD_KEY,
            POINTER_MOTION,
            POINTER_MOTION_ABSOLUTE,
            POINTER_BUTTON,
            POINTER_SCROLL_WHEEL,
            POINTER_SCROLL_FINGER,
            POINTER_SCROLL_CONTINUOUS,
        ] {
            assert_ne!(kind, 403, "no kind this crate reads is the deprecated axis");
        }
    }

    #[test]
    fn a_context_opens_and_has_a_descriptor_to_wait_on() {
        if without_libinput("a_context_opens_and_has_a_descriptor_to_wait_on") {
            return;
        }

        let context = Context::open().expect("libinput is here, so a context is made");

        // A loop that waits on nothing spins at the speed of the processor, so the descriptor is
        // the one thing a context is unusable without.
        assert!(
            context.descriptor().as_raw_fd() >= 0,
            "the descriptor is one the loop can wait on"
        );
    }

    #[test]
    fn a_context_with_no_devices_reads_nothing_and_says_so() {
        if without_libinput("a_context_with_no_devices_reads_nothing_and_says_so") {
            return;
        }

        let mut context = Context::open().expect("libinput is here, so a context is made");
        let mut files = Refusing::default();

        context
            .dispatch(&mut files)
            .expect("a context with nothing in it reads nothing, successfully");

        assert!(
            files.asked.is_empty(),
            "nothing was added, so nothing was opened"
        );
        assert_eq!(files.taken, 0, "and nothing was handed back");
    }

    #[test]
    fn a_context_can_be_dropped_without_ever_being_dispatched() {
        // The path a caller takes when it decides, after opening libinput, that it wants the other
        // input source after all.
        if without_libinput("a_context_can_be_dropped_without_ever_being_dispatched") {
            return;
        }

        let context = Context::open().expect("libinput is here, so a context is made");
        drop(context);
    }

    /// A caller that opens a node read-only, and remembers what it was asked.
    ///
    /// libinput takes a read-only descriptor. Such a descriptor gives up the writes, which are a
    /// device's lights and nothing a run needs.
    ///
    /// **The rest of the flags are passed through**, and they carry `O_NONBLOCK`. Without that bit
    /// libinput reads the device until it stops answering, which a keyboard nobody is typing on
    /// never does, and the thread stays inside libinput for the rest of the run. `custom_flags`
    /// masks the access mode out, so the read-only decision above still stands.
    #[derive(Debug, Default)]
    pub(crate) struct Opening {
        /// Every path libinput asked for, in order.
        pub(crate) asked: Vec<PathBuf>,
        /// Every descriptor libinput handed back.
        pub(crate) taken: usize,
    }

    impl Files for Opening {
        fn open(&mut self, path: &Path, flags: i32) -> std::result::Result<OwnedFd, i32> {
            use std::os::unix::fs::OpenOptionsExt;

            self.asked.push(path.to_owned());
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(flags)
                .open(path)
                .map(OwnedFd::from)
                // `EIO`, for an open that failed with nothing to say.
                .map_err(|error| error.raw_os_error().unwrap_or(5))
        }

        fn close(&mut self, fd: OwnedFd) {
            self.taken += 1;
            drop(fd);
        }
    }

    /// Returns a node in `/dev/input` this process can open, where the machine has one.
    ///
    /// Asked of the machine rather than of libinput, for the reason the loader precondition is:
    /// a helper that asked the subject would skip every test exactly when the subject broke. Most
    /// nodes belong to the `input` group, so an ordinary user often has none.
    fn an_openable_node() -> Option<PathBuf> {
        let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev/input")
            .ok()?
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("event"))
            })
            .filter(|path| std::fs::File::open(path).is_ok())
            .collect();
        nodes.sort();
        nodes.into_iter().next()
    }

    /// Returns the node the device tests run against, or says why there is none.
    fn a_device(test: &str) -> Option<(Context, PathBuf)> {
        if without_libinput(test) {
            return None;
        }
        let Some(node) = an_openable_node() else {
            eprintln!(
                "{test}: this process can open no node in `/dev/input`, so nothing about a device \
                 was checked. Most of them belong to the `input` group: run `usermod -aG input \
                 $USER`, log in again, and run the suite."
            );
            return None;
        };
        let context = Context::open().expect("libinput is here, so a context is made");
        Some((context, node))
    }

    #[test]
    fn a_node_that_is_added_arrives_as_a_device_that_says_what_it_is() {
        let Some((mut context, node)) = a_device("a_node_that_is_added_arrives_as_a_device") else {
            return;
        };
        let mut files = Opening::default();

        assert!(
            context.add(&mut files, &node),
            "the node is one libinput reads"
        );
        assert_eq!(
            files.asked,
            std::slice::from_ref(&node),
            "and it was opened once"
        );

        let Some(Event::DeviceAdded(device)) = context.next_event() else {
            panic!("a node that was added arrives");
        };

        assert_eq!(
            device.path(),
            node,
            "the device names the node it came from"
        );
        assert_eq!(
            Some(device.sysname()),
            node.file_name().and_then(|name| name.to_str()),
            "and what the kernel calls it"
        );
        assert!(
            !device.name().is_empty(),
            "a real device has a name: {device:?}"
        );
        assert_eq!(
            context.device(device.id()).map(Device::id),
            Some(device.id()),
            "and the context holds it under its id"
        );
    }

    #[test]
    fn a_node_this_context_already_holds_is_refused_without_asking_libinput() {
        // libinput deduplicates nothing: a second add gives a second live device for one node, and
        // every keystroke arrives twice.
        let Some((mut context, node)) = a_device("a_node_this_context_already_holds_is_refused")
        else {
            return;
        };
        let mut files = Opening::default();

        assert!(context.add(&mut files, &node));
        assert!(!context.add(&mut files, &node), "the second add is refused");

        assert_eq!(
            files.asked,
            [node],
            "and libinput was never asked, so the node was opened once"
        );

        // A device is held from the moment its arrival is read, so the count is taken after the
        // queue is drained rather than after the add.
        while context.next_event().is_some() {}
        assert_eq!(context.devices().count(), 1, "one device for one node");
    }

    #[test]
    fn a_path_that_is_not_an_evdev_node_is_refused_without_asking_libinput() {
        // None of these reaches libinput. The two that are not character devices would each draw a
        // `client bug: Invalid path` line, and the two that are character devices would be opened
        // through `Files` and then refused silently. `/dev/input` holds `mice` beside the event
        // nodes, so a caller that walks the directory meets it.
        if without_libinput("a_path_that_is_not_an_evdev_node_is_refused") {
            return;
        }
        let mut context = Context::open().expect("libinput is here, so a context is made");
        let mut files = Opening::default();

        for path in [
            "/dev/input",
            "/dev/input/mice",
            "/etc/hostname",
            "/dev/null",
        ] {
            assert!(
                !context.add(&mut files, Path::new(path)),
                "{path} is not a node libinput reads"
            );
        }
        assert!(
            files.asked.is_empty(),
            "and none of them was opened: {:?}",
            files.asked
        );
    }

    #[test]
    fn a_node_this_process_cannot_open_is_refused_by_the_caller() {
        if without_libinput("a_node_this_process_cannot_open_is_refused_by_the_caller") {
            return;
        }
        let mut context = Context::open().expect("libinput is here, so a context is made");
        let mut files = Refusing::default();

        // A node that exists and that `Files` refuses. Every machine has `event0`, and this test
        // holds whether or not this process may open it: `Refusing` says no either way.
        let node = Path::new("/dev/input/event0");
        if !node.exists() {
            eprintln!(
                "a_node_this_process_cannot_open_is_refused_by_the_caller: this machine has no \
                 `/dev/input/event0`, so a refused open was not covered."
            );
            return;
        }

        assert!(!context.add(&mut files, node), "the caller refused it");
        assert_eq!(files.asked, [node.to_owned()], "having been asked once");
        assert_eq!(context.devices().count(), 0, "so nothing is read");
    }

    #[test]
    fn a_suspend_gives_every_device_back_and_a_resume_takes_them_again() {
        // The two halves of a terminal switch. libinput asks for the same path again, which is why
        // the session daemon can own the device and this process can be lent it.
        let Some((mut context, node)) = a_device("a_suspend_gives_every_device_back") else {
            return;
        };
        let mut files = Opening::default();

        assert!(context.add(&mut files, &node));
        let Some(Event::DeviceAdded(before)) = context.next_event() else {
            panic!("the device arrives");
        };

        context.suspend(&mut files);

        let Some(Event::DeviceRemoved(went)) = context.next_event() else {
            panic!("a suspend gives every device back");
        };
        assert_eq!(went.id(), before.id(), "the one that was there");
        assert_eq!(files.taken, 1, "and the descriptor came back");
        assert_eq!(
            context.devices().count(),
            0,
            "nothing is read while suspended"
        );

        context.resume(&mut files).expect("the node opens again");

        let Some(Event::DeviceAdded(after)) = context.next_event() else {
            panic!("a resume takes every device again");
        };
        assert_eq!(after.path(), node, "the same node");
        assert_eq!(
            files.asked,
            [node.clone(), node],
            "opened again by path, which is what makes a daemon able to own it"
        );
        assert_ne!(
            after.id(),
            before.id(),
            "and it is a new device, so state held for the old one cannot match it"
        );
    }

    #[test]
    fn a_device_that_is_removed_frees_its_node_for_another_add() {
        let Some((mut context, node)) = a_device("a_device_that_is_removed_frees_its_node") else {
            return;
        };
        let mut files = Opening::default();

        assert!(context.add(&mut files, &node));
        let Some(Event::DeviceAdded(device)) = context.next_event() else {
            panic!("the device arrives");
        };

        context.remove(&mut files, device.id());

        let Some(Event::DeviceRemoved(went)) = context.next_event() else {
            panic!("a device that is removed says so");
        };
        assert_eq!(went.id(), device.id());
        assert_eq!(files.taken, 1, "the descriptor came back");

        // The node is forgotten with the device. A replug needs that: the same path arrives again
        // and has to be addable.
        assert!(
            context.add(&mut files, &node),
            "the node can be added again"
        );
    }

    #[test]
    fn a_context_over_a_library_the_caller_opened_is_the_same_context() {
        // `Context::open` is this over a library it opens itself, and a caller that has already
        // asked whether libinput is here should not have to open it twice.
        if without_libinput("a_context_over_a_library_the_caller_opened_is_the_same_context") {
            return;
        }

        let library = Library::load().expect("libinput is here, so it loads");
        let context = Context::over(library).expect("and a context is made over it");

        assert!(context.descriptor().as_raw_fd() >= 0);
    }
}
