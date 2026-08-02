//! The seam a diagnostic tool attaches to.
//!
//! A window's interesting state — what it computed, what it damaged, what it emitted, what it holds
//! on the device — exists only for the instant between the frame that produced it and the next
//! frame that overwrites it. A tool that wants to show that state cannot go and ask for it later,
//! and it cannot ask for it from inside the view either, because a view sees the document and not
//! the frame that painted it.
//!
//! So a probe is *called*, once, at the end of every frame, and handed the window as it stands. It
//! is deliberately the narrowest possible seam: one method, no return value, nothing it can change.
//! A probe that mutated the window would be a probe that changed the thing it was measuring, and a
//! probe that could refuse a frame would be a second frame loop.
//!
//! Nothing in this framework implements it. It exists so that an inspector — or a recorder, or a
//! test that wants to assert about the frames a script produced — has one object to install rather
//! than a fork of the loop.
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//! use zgui_runtime::{FrameProbe, Window};
//!
//! /// A probe that counts frames and nothing else.
//! #[derive(Default)]
//! struct Counting {
//!     frames: Cell<u64>,
//! }
//!
//! impl FrameProbe for Counting {
//!     fn frame_ended(&self, _window: &Window) {
//!         self.frames.set(self.frames.get() + 1);
//!     }
//! }
//!
//! let probe: Rc<dyn FrameProbe> = Rc::new(Counting::default());
//! // `App::with_probe(probe)` installs it into every window the application opens.
//! # let _ = probe;
//! ```

use crate::window::Window;

/// Something told about each frame after it has been produced.
///
/// Called with the window in the state the frame left it: the scene it emitted, the damage it
/// answered, the layout it computed and the renderer's own report are all still the ones belonging
/// to the frame that has just been presented.
///
/// The method takes `&self` rather than `&mut self` because the window is borrowed for the call, so
/// a probe that wanted to keep something writes it through a cell or a signal. That is not a
/// restriction in practice — everything a probe does with what it is handed is either recording it
/// or publishing it — and it is what lets the same probe be installed into several windows.
pub trait FrameProbe {
    /// A frame has finished on `window`.
    fn frame_ended(&self, window: &Window);

    /// What to call this probe in a diagnostic rendering of the window's options.
    ///
    /// Defaulted, because a probe is identified by what it does rather than by what it is called,
    /// and requiring a name of every implementation would be requiring a decision nobody has an
    /// opinion about.
    fn describe(&self) -> &str {
        "a frame probe"
    }
}

impl core::fmt::Debug for dyn FrameProbe {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.describe())
    }
}
