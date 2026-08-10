//! The window these assertions act on, and the four things they act with.
//!
//! A pointer, a clock, a census and a readback. The clock is the one that is easy to leave out and
//! impossible to do without: every interaction in this library is styled with a transition, so what
//! a control looks like *while* the pointer is on it and what it looks like a moment later are two
//! different pictures, and a fixture that never moved time on can only ever see the first.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use zgui::geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui::platform::{AppHandler, SurfaceEvent};
use zgui::prelude::*;
use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Timestamp};
use zgui_platform_headless::Harness;

use crate::desktop::census::Census;
use crate::desktop::grab::{self, Grab, Handles};
use crate::device::{self, Log};

/// How wide the surface every fixture opens is, in device pixels.
pub const WIDTH: f32 = 900.0;

/// How tall it is.
pub const HEIGHT: f32 = 600.0;

/// How long a transition in this library's tokens runs, with room to spare.
///
/// `--zui-motion-duration-fast` is 120ms. Waiting exactly that long lands on the frame the
/// interpolation stops and asserts nothing about the frames after it, which is precisely where the
/// interesting failure lives.
pub const SETTLED: Duration = Duration::from_millis(400);

/// One frame of a sixty-hertz output, which is the rate the clock is stepped at.
const TICK: Duration = Duration::from_micros(16_667);

/// Serialises the fixtures in this binary.
///
/// A process has one reactive runtime and one set of thread-locals; these fixtures are the only
/// thing that would ask for several windows at once.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|held| held.into_inner())
}

/// An open window, driven the way a compositor drives one, drawn by a real device.
pub struct Stage {
    /// The application, over the headless platform.
    harness: Harness<Box<dyn AppHandler>>,
    /// The document's engine seams.
    handles: Handles,
    /// Every frame the device drew, in order.
    log: Log,
    /// Where the pointer is, so a press lands where the last move left it.
    pointer: Point<CssPx, Css>,
    /// Whether a pointer has ever been on the surface.
    entered: bool,
    /// Held for the life of the fixture.
    _turn: MutexGuard<'static, ()>,
    /// The graphics device, held for the same reason.
    _device: MutexGuard<'static, ()>,
}

impl Stage {
    /// Opens `view` in a window styled by `sheet`, or nothing on a machine with no device.
    ///
    /// # Panics
    ///
    /// Panics when the application will not build, and when the document it built cannot be reached
    /// — either of which would leave every assertion below it measuring nothing.
    pub fn open<F, V>(sheet: &str, view: F) -> Option<Self>
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        if !device::available() {
            return None;
        }
        let turn = exclusive();
        let held = device::device_lock();
        grab::forget();
        let log: Log = Log::default();
        let mut view = view;
        let handler = zgui::app()
            .with_size(WIDTH, HEIGHT)
            .with_stylesheet(sheet)
            .with_renderer(Box::new(device::factory(&log)))
            .into_handler(move || (Grab, view().into_view()))
            .expect("the application builds");
        let mut inner: Option<Box<dyn AppHandler>> = None;
        handler
            .drive(|app| {
                inner = Some(app);
                Ok(())
            })
            .expect("the driver takes the application");
        let mut harness = Harness::new(inner.expect("the driver was handed the application"));
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(WIDTH),
            DevicePx(HEIGHT),
        )));
        harness.settle(64);
        let handles =
            grab::taken().expect("the marker view was built, so the document is reachable");
        let mut stage = Self {
            harness,
            handles,
            log,
            pointer: Point::new(CssPx(0.0), CssPx(0.0)),
            entered: false,
            _turn: turn,
            _device: held,
        };
        stage.repaint();
        Some(stage)
    }

    /// The document's engine seams.
    pub fn handles(&self) -> &Handles {
        &self.handles
    }

    /// Everything in the document, as it stands now.
    pub fn census(&self) -> Census {
        Census::take(&self.handles)
    }

    /// Moves the clock on by one output frame and runs exactly one frame for it.
    ///
    /// One frame, not a settle: a window with an animation running always owes another, so
    /// settling would run a burst of frames against one reading of the clock and the picture read
    /// back would be the last of them rather than the one the output showed.
    pub fn tick(&mut self) {
        self.harness.advance(TICK);
        self.harness.pump();
    }

    /// Runs frames until nothing is owed, without moving the clock.
    pub fn settle(&mut self) {
        self.harness.settle(64);
    }

    /// Moves the clock on in output-frame steps, running the frames each step asks for.
    ///
    /// Stepped rather than jumped, because a transition is sampled once per frame and a clock that
    /// went from its start to past its end in one move would produce a single sample — which is the
    /// one shape in which an interpolation that never runs and one that runs correctly are the same
    /// picture.
    pub fn wait(&mut self, total: Duration) {
        let steps = (total.as_secs_f64() / TICK.as_secs_f64()).ceil() as u32;
        for _ in 0..steps.max(1) {
            self.harness.advance(TICK);
            self.harness.settle(64);
        }
        self.repaint();
    }

    /// Moves the clock on the way [`Stage::wait`] does, without asking for a complete picture.
    ///
    /// For claims about the frames an animation drew by itself: the ordinary wait ends by forcing a
    /// full redraw, which repairs exactly the kind of defect a partial repaint can have.
    pub fn wait_quietly(&mut self, total: Duration) {
        let steps = (total.as_secs_f64() / TICK.as_secs_f64()).ceil() as u32;
        for _ in 0..steps.max(1) {
            self.harness.advance(TICK);
            self.harness.settle(64);
        }
    }

    /// Resizes the surface, exactly as a compositor does, and lets the frames that asks for run.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.harness
            .deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(width),
                DevicePx(height),
            )));
        self.harness.settle(64);
    }

    /// Makes the window draw a complete picture, and waits for it.
    ///
    /// A window repaints the rectangles it damaged and nothing else, so the display list of an
    /// ordinary frame holds one control and the target it is composed into holds only that. A
    /// reading taken from such a frame is a reading of whatever the surface was cleared to
    /// everywhere the frame did not touch — which is not a picture of the window and is not what
    /// anybody is looking at.
    ///
    /// A surface that comes back from being hidden owes a full redraw, because nothing observed
    /// what the compositor did to it while it was away. That is the event asked for here: it is one
    /// a real desktop sends, it costs one frame, and what it produces is the whole window.
    pub fn repaint(&mut self) {
        self.harness.deliver_to_first(SurfaceEvent::Occluded(true));
        self.harness.settle(64);
        self.harness.deliver_to_first(SurfaceEvent::Occluded(false));
        self.harness.settle(64);
    }

    /// Moves the pointer to `at`, in device pixels, and lets the frames that asks for run.
    pub fn move_to(&mut self, at: Point<DevicePx, Device>) {
        let scale = self.handles.host.scale().max(0.01);
        let position = Point::new(CssPx(at.x.0 / scale), CssPx(at.y.0 / scale));
        self.pointer = position;
        if !self.entered {
            self.entered = true;
            self.deliver(pointer(PointerAction::Entered, position, None));
        }
        self.deliver(pointer(PointerAction::Moved, position, None));
    }

    /// Takes the pointer off the surface altogether.
    pub fn leave(&mut self) {
        let position = self.pointer;
        self.entered = false;
        self.deliver(pointer(PointerAction::Left, position, None));
    }

    /// Presses the primary button where the pointer is.
    pub fn press(&mut self) {
        let position = self.pointer;
        self.deliver(pointer(
            PointerAction::Pressed,
            position,
            Some(PointerButton::Primary),
        ));
    }

    /// Releases it.
    pub fn release(&mut self) {
        let position = self.pointer;
        self.deliver(pointer(
            PointerAction::Released,
            position,
            Some(PointerButton::Primary),
        ));
    }

    /// Presses and releases where the pointer is.
    pub fn press_release(&mut self) {
        self.press();
        self.release();
    }

    /// Moves to `at` and clicks there, exactly as a mouse does.
    pub fn click(&mut self, at: Point<DevicePx, Device>) {
        self.move_to(at);
        self.press_release();
    }

    /// Turns the wheel `lines` down where the pointer is, and lets the scroll finish.
    ///
    /// A detent is carried to its destination over the frames that follow rather than landing in
    /// the one it arrived in, so the clock is run before anything is read back.
    /// Delivers one wheel movement and runs exactly one frame, leaving any glide mid-flight.
    ///
    /// [`wheel`](Stage::wheel) settles the window afterwards, which is right for a fixture that
    /// asks where a scroll *ends*. A fixture that asks what the window looked like *during* the
    /// glide steps it frame by frame with this instead.
    pub fn wheel_step(&mut self, lines: f32) {
        self.harness.deliver_to_first(SurfaceEvent::Wheel {
            event: zgui::vocab::WheelEvent {
                id: zgui::vocab::PointerId::MOUSE,
                kind: zgui::vocab::PointerKind::Mouse,
                position: self.pointer,
                delta: zgui::vocab::ScrollDelta::Lines { x: 0.0, y: lines },
                phase: zgui::vocab::ScrollPhase::Discrete,
            },
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.advance(TICK);
        self.harness.pump();
    }

    pub fn wheel(&mut self, lines: f32) {
        self.deliver(SurfaceEvent::Wheel {
            event: zgui::vocab::WheelEvent {
                id: zgui::vocab::PointerId::MOUSE,
                kind: zgui::vocab::PointerKind::Mouse,
                position: self.pointer,
                delta: zgui::vocab::ScrollDelta::Lines { x: 0.0, y: lines },
                phase: zgui::vocab::ScrollPhase::Discrete,
            },
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.wait(Duration::from_millis(600));
    }

    /// Moves to `at` and presses and releases the secondary button there.
    ///
    /// What asks for a context menu on a mouse. The platform layer is what turns this into a
    /// request event, so a fixture that sent the request directly would skip the half that a real
    /// right-click goes through.
    pub fn right_click(&mut self, at: Point<DevicePx, Device>) {
        self.move_to(at);
        let position = self.pointer;
        let button = Some(PointerButton::Secondary);
        self.deliver(pointer(PointerAction::Pressed, position, button));
        self.deliver(pointer(PointerAction::Released, position, button));
    }

    /// Presses and releases one key, exactly as a keyboard does.
    ///
    /// The press is what a window acts on; the release is sent because a window receives one and a
    /// model that watched for it would otherwise never see the key finish.
    pub fn press_key(&mut self, key: zgui::vocab::Key) {
        let event = zgui::vocab::KeyEvent {
            key: key.clone(),
            key_without_modifiers: key,
            physical: zgui::vocab::PhysicalKey::Unidentified(0),
            location: zgui::vocab::KeyLocation::Standard,
            repeat: false,
        };
        for state in [
            zgui::vocab::KeyState::Pressed,
            zgui::vocab::KeyState::Released,
        ] {
            self.deliver(SurfaceEvent::Key {
                state,
                event: event.clone(),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
    }

    /// Types `text`, one character key at a time.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.press_key(zgui::vocab::Key::Character(
                zgui::vocab::SharedString::from(character.to_string()),
            ));
        }
    }

    /// Presses and releases one named key.
    pub fn press_named(&mut self, key: zgui::vocab::NamedKey) {
        self.press_key(zgui::vocab::Key::Named(key));
    }

    /// Which element has keyboard focus.
    pub fn focused(&self) -> Option<zgui::view::NodeId> {
        use zgui::reactive::prelude::GetUntracked;
        self.handles.host.focused().get_untracked()
    }

    /// Every filled rectangle the most recent frame's display list held inside `rect`.
    ///
    /// The display list rather than the pixels, because a caret is one pixel wide and a claim about
    /// how many of them there are cannot be made by looking at a picture: two carets a pixel apart
    /// and one caret twice as wide are the same photograph.
    ///
    /// # Panics
    ///
    /// Panics when nothing has been drawn yet.
    pub fn quads_in(
        &self,
        rect: zgui::geom::Rect<DevicePx, Device>,
    ) -> Vec<crate::device::frame::Filled> {
        self.drawn(|frame| {
            frame
                .quads
                .iter()
                .copied()
                .filter(|quad| inside(rect, quad.bounds))
                .collect()
        })
    }

    /// Reads `of` off the last frame that drew anything.
    ///
    /// Not simply the last frame. A window paints the rectangles it damaged and nothing else, so a
    /// frame that damaged nothing holds an empty display list — and one of those is run after every
    /// interaction, because settling means running frames until nothing more is owed. The picture on
    /// the screen is the last frame that drew, and after a [`repaint`](Stage::repaint) that frame is
    /// the whole window.
    ///
    /// # Panics
    ///
    /// Panics when no frame has drawn anything at all, because every reading taken from a window
    /// that never drew agrees with every other one and none of them means anything.
    fn drawn<T>(&self, of: impl Fn(&crate::device::frame::Frame) -> T) -> T {
        let frames = self.log.lock().unwrap_or_else(|held| held.into_inner());
        let frame = frames
            .iter()
            .rev()
            .find(|frame| {
                !frame.quads.is_empty() || !frame.glyphs.is_empty() || !frame.drawings.is_empty()
            })
            .expect("the window drew something at least once");
        of(frame)
    }

    /// Every drawing the most recent frame's display list held inside `rect`, in painting order.
    ///
    /// A drawing is the one kind of content a picture of the window cannot be asked about on its
    /// own: an icon that is not in the display list and an icon whose curves happen to land on
    /// pixels the colour of the page are the same photograph. This says whether the frame carried
    /// the item at all.
    ///
    /// # Panics
    ///
    /// Panics when nothing has been drawn yet.
    pub fn drawings_in(
        &self,
        rect: zgui::geom::Rect<DevicePx, Device>,
    ) -> Vec<crate::device::frame::Drawing> {
        self.drawn(|frame| {
            frame
                .drawings
                .iter()
                .copied()
                .filter(|drawing| inside(rect, drawing.ink))
                .collect()
        })
    }

    /// Every glyph the most recent frame's display list held inside `rect`, in painting order.
    ///
    /// # Panics
    ///
    /// Panics when nothing has been drawn yet.
    pub fn glyphs_in(
        &self,
        rect: zgui::geom::Rect<DevicePx, Device>,
    ) -> Vec<crate::device::frame::Glyph> {
        let mut found: Vec<crate::device::frame::Glyph> = self.drawn(|frame| {
            frame
                .glyphs
                .iter()
                .copied()
                .filter(|glyph| inside(rect, glyph.bounds))
                .collect()
        });
        // Left to right, because painting order across three sprite arrays is not reading order and
        // the question being asked is what the line *says*.
        found.sort_by(|left, right| left.bounds.origin.x.0.total_cmp(&right.bounds.origin.x.0));
        found
    }

    /// Writes the frame these assertions are reading out as a picture, when a run asked for one.
    ///
    /// The same buffer, not a second run: a picture taken from another window proves that *a*
    /// window looked like that, and this one cannot disagree with the assertion above it. Nothing
    /// is written unless `ZGUI_SHOT_DIR` names somewhere to put it.
    ///
    /// # Panics
    ///
    /// Panics when a directory was asked for and the write failed, because a missing picture and a
    /// picture of the wrong thing are told apart only by saying so.
    pub fn capture(&self, name: &str) {
        if crate::device::shot::directory().is_none() {
            return;
        }
        self.drawn(|frame| crate::device::shot::whole(&frame.pixels, name))
            .expect("the picture was written");
    }

    /// The colour at `at` in the most recent frame the device drew, as red, green and blue.
    ///
    /// # Panics
    ///
    /// Panics when nothing has been drawn yet, because every reading taken from a window that never
    /// drew agrees with every other one and none of them means anything.
    pub fn colour_at(&self, at: Point<DevicePx, Device>) -> (u8, u8, u8) {
        self.drawn(|frame| {
            let [red, green, blue, _] = frame.pixels.rgba(at.x.0 as i32, at.y.0 as i32);
            (red, green, blue)
        })
    }

    /// Where `node` is on the surface.
    ///
    /// # Panics
    ///
    /// Panics when the node has no box.
    pub fn rect_of(&self, node: zgui::view::NodeId) -> zgui::geom::Rect<DevicePx, Device> {
        crate::desktop::census::absolute(&self.handles, node)
            .unwrap_or_else(|| panic!("{node:?} is not laid out, so it covers nothing"))
    }

    /// Every colour inside `rect` in the most recent frame that drew, one entry per pixel.
    ///
    /// The same frame [`Stage::quads_in`] and [`Stage::capture`] read, and for the same reason: a
    /// window settles by running frames until nothing more is owed, so the last frame of an
    /// interaction is routinely one that damaged nothing and drew nothing. Its target holds
    /// whatever it was cleared to, and a colour read out of it is a colour nobody was ever looking
    /// at — the picture on the screen is the last frame that drew.
    ///
    /// # Panics
    ///
    /// Panics when nothing has been drawn yet.
    pub fn colours_in(&self, rect: zgui::geom::Rect<DevicePx, Device>) -> Vec<(u8, u8, u8)> {
        self.drawn(|frame| {
            let size = frame.pixels.size();
            let left = (rect.origin.x.0.floor() as i32).clamp(0, size.width.saturating_sub(1));
            let top = (rect.origin.y.0.floor() as i32).clamp(0, size.height.saturating_sub(1));
            let right =
                ((rect.origin.x.0 + rect.size.width.0).ceil() as i32).clamp(left, size.width);
            let bottom =
                ((rect.origin.y.0 + rect.size.height.0).ceil() as i32).clamp(top, size.height);
            let mut colours = Vec::new();
            for y in top..bottom {
                for x in left..right {
                    let [red, green, blue, _] = frame.pixels.rgba(x, y);
                    colours.push((red, green, blue));
                }
            }
            colours
        })
    }

    /// How many animations are running on `node` right now.
    ///
    /// The computed half of an appearance: a picture taken while a transition is still moving is
    /// not the picture the control settles at, and the two are told apart only by asking.
    pub fn running_animations(&self, node: zgui::view::NodeId) -> usize {
        self.handles.host.running_animations(node)
    }

    /// Where `node` is on the surface, and its middle.
    ///
    /// # Panics
    ///
    /// Panics when the node has no box, because a fixture that quietly aimed at the origin reports
    /// the same thing as a control that does not answer.
    pub fn centre_of(&self, node: zgui::view::NodeId) -> Point<DevicePx, Device> {
        let rect = crate::desktop::census::absolute(&self.handles, node)
            .unwrap_or_else(|| panic!("{node:?} is not laid out, so there is nowhere to aim"));
        Point::new(
            DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
            DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
        )
    }

    /// A point a fraction of the way across `node`, at its vertical middle.
    ///
    /// # Panics
    ///
    /// Panics when the node has no box.
    pub fn along(&self, node: zgui::view::NodeId, fraction: f32) -> Point<DevicePx, Device> {
        let rect = crate::desktop::census::absolute(&self.handles, node)
            .unwrap_or_else(|| panic!("{node:?} is not laid out, so there is nowhere to aim"));
        Point::new(
            DevicePx(rect.origin.x.0 + rect.size.width.0 * fraction),
            DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
        )
    }

    /// Hands one event to the application and lets the frames it asked for run.
    ///
    /// The clock is moved on by one output frame afterwards, because an event arriving is not the
    /// end of what it causes: a handler writes a signal, the write is answered by the reactive
    /// flush, and what *that* runs — the callback a component reports its new value through — is
    /// serviced by the frame after it. A fixture that delivered events on a stopped clock would
    /// read the value a control held before the interaction it just performed.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(64);
        self.harness.advance(TICK);
        self.harness.settle(64);
    }
}

/// A pointer doing `action` at `at`.
fn pointer(
    action: PointerAction,
    at: Point<CssPx, Css>,
    button: Option<PointerButton>,
) -> SurfaceEvent {
    let mut event = PointerEvent::mouse(at);
    if let Some(button) = button {
        event = event.with_button(button);
    }
    SurfaceEvent::Pointer {
        action,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// Whether `inner`'s middle falls inside `outer`.
///
/// The middle rather than containment, because a glyph's box overhangs its line by a fraction of a
/// pixel and a caret is drawn flush against the field's edge — either of which would put a drawing
/// that is plainly inside the control outside a strict test.
fn inside(
    outer: zgui::geom::Rect<DevicePx, Device>,
    inner: zgui::geom::Rect<DevicePx, Device>,
) -> bool {
    let x = inner.origin.x.0 + inner.size.width.0 / 2.0;
    let y = inner.origin.y.0 + inner.size.height.0 / 2.0;
    x >= outer.origin.x.0
        && x <= outer.origin.x.0 + outer.size.width.0
        && y >= outer.origin.y.0
        && y <= outer.origin.y.0 + outer.size.height.0
}
