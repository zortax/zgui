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

use std::marker::PhantomData;
use std::os::fd::{BorrowedFd, RawFd};
use std::ptr::NonNull;

use crate::error::{Error, Result};
use crate::library::{Libinput, Library};

pub use crate::context::files::Files;

use crate::context::files::{Callers, INTERFACE};

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
    /// What makes this type `!Send` and `!Sync`.
    ///
    /// The raw pointers do this as well. The marker states it, so that a field which becomes
    /// something shareable cannot make the whole type shareable with it.
    thread_bound: PhantomData<*const ()>,
}

impl Context {
    /// Opens libinput and makes a context that reads the devices it is given.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] or [`Error::Symbol`] when libinput cannot be opened, and
    /// [`Error::Context`] when libinput would not make a context.
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
            thread_bound: PhantomData,
        })
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
        // SAFETY: `raw` is this context and this is the only place it is freed, in the drop of the
        // one value that owns it. Nothing calls through it afterwards.
        unsafe { (self.library.symbols().unref)(self.raw.as_ptr()) };

        // SAFETY: the box leaked in `over`, reclaimed once, here. libinput is freed, so nothing
        // holds the address any more — including the `close_restricted` calls the free itself
        // made, which have already returned.
        drop(unsafe { Box::from_raw(self.callers.as_ptr()) });
    }
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
