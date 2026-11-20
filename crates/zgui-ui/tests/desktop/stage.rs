//! The open window, and the events a fixture acts on it with.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use zgui::geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui::platform::{AppHandler, SurfaceEvent};
use zgui::prelude::*;
use zgui::vocab::{
    KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, Timestamp,
};
use zgui_platform_headless::Harness;

use crate::desktop::census::Census;
use crate::desktop::grab::{self, Grab, Handles};
use crate::desktop::reader::{self, Announced};

/// How wide the surface every fixture opens is, in device pixels.
pub const WIDTH: f32 = 1200.0;

/// How tall it is.
pub const HEIGHT: f32 = 900.0;

/// One frame of a sixty-hertz output, which is the rate the clock is stepped at.
const TICK: Duration = Duration::from_micros(16_667);

/// One thing a person does, as a batch of events can carry it.
///
/// A window system does not hand over one event per turn: everything that arrived while the last
/// frame was being drawn comes together, so a click and the key pressed a moment after it are
/// ordinarily delivered in the same breath. [`Stage::burst`] is what a fixture says that with.
#[derive(Clone, Copy, Debug)]
pub enum Act {
    /// A press and a release over the middle of the control that says this.
    ClickSaying(&'static str),
    /// A press and a release of a named key, wherever the keyboard is.
    Key(NamedKey),
}

/// Serialises the fixtures in this binary.
///
/// A process has one reactive runtime and one set of thread-locals; these fixtures are the only
/// thing that would ask for several windows at once.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|held| held.into_inner())
}

/// An open window, driven the way a compositor drives one.
pub struct Stage {
    /// The application, over the headless platform.
    harness: Harness<Box<dyn AppHandler>>,
    /// The document's engine seams.
    handles: Handles,
    /// Where the pointer is, so a press lands where the last move left it.
    pointer: Point<CssPx, Css>,
    /// Held for the life of the fixture.
    _turn: MutexGuard<'static, ()>,
}

impl Stage {
    /// Opens `view` in a window styled by `sheet`.
    ///
    /// # Panics
    ///
    /// Panics when the application will not build, and when the document it built cannot be reached
    /// — either of which would leave every assertion below it measuring nothing.
    pub fn open<F, V>(sheet: &str, view: F) -> Self
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        let turn = exclusive();
        grab::forget();
        let mut view = view;
        let handler = zgui::app()
            .with_size(WIDTH, HEIGHT)
            .with_stylesheet(sheet)
            .with_renderer(Box::new(crate::desktop::renderer::build))
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
        Self {
            harness,
            handles,
            pointer: Point::new(CssPx(0.0), CssPx(0.0)),
            _turn: turn,
        }
    }

    /// The document's engine seams.
    pub fn handles(&self) -> &Handles {
        &self.handles
    }

    /// Everything in the document, as it stands now.
    pub fn census(&self) -> Census {
        Census::take(&self.handles)
    }

    /// Runs frames until nothing is owed.
    pub fn settle(&mut self) {
        self.harness.settle(64);
    }

    /// Presents the same window on an output of a different density.
    ///
    /// The surface keeps the device pixels it had, which is what a window dragged between two
    /// outputs of different densities does *not* do — and is what a fixture wants, because it keeps
    /// every coordinate in this file comparable with the ones taken at one device pixel per CSS
    /// pixel. What changes is the number a CSS length resolves to, which is the whole of what is
    /// being asked about.
    pub fn present_at(&mut self, scale: f64) {
        self.deliver(SurfaceEvent::ScaleFactorChanged {
            scale_factor: scale,
            size: Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
        });
        self.settle();
    }

    /// How many device pixels one CSS pixel is.
    fn scale(&self) -> f32 {
        self.handles.host.scale().max(0.01)
    }

    /// A device-pixel point in the CSS pixels the platform reports input in.
    fn css(&self, at: Point<DevicePx, Device>) -> Point<CssPx, Css> {
        let scale = self.scale();
        Point::new(CssPx(at.x.0 / scale), CssPx(at.y.0 / scale))
    }

    /// Hands one event to the application and lets the frames it asked for run.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(16);
    }

    /// Resizes the surface, the way dragging the window's edge does.
    pub fn deliver_resize(&mut self, width: f32, height: f32) {
        self.deliver(SurfaceEvent::Resized(Size::new(
            DevicePx(width),
            DevicePx(height),
        )));
    }

    /// Moves the pointer to `at`, in device pixels.
    pub fn move_to(&mut self, at: Point<DevicePx, Device>) {
        let position = self.css(at);
        let first = self.pointer == Point::new(CssPx(0.0), CssPx(0.0));
        self.pointer = position;
        if first {
            self.deliver(pointer(PointerAction::Entered, position, None));
        }
        self.deliver(pointer(PointerAction::Moved, position, None));
    }

    /// Turns the wheel `lines` down where the pointer is, and lets the scroll finish.
    ///
    /// A detent is carried to its destination over the frames that follow rather than landing in
    /// the one it arrived in, so the frames it asks for are run before anything is read back.
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
        self.hold(Duration::from_millis(600));
    }

    /// Presses and releases the primary button where the pointer is.
    pub fn press_release(&mut self) {
        self.press();
        self.release();
    }

    /// Puts the primary button down where the pointer is, and leaves it down.
    pub fn press(&mut self) {
        let position = self.pointer;
        self.deliver(pointer(
            PointerAction::Pressed,
            position,
            Some(PointerButton::Primary),
        ));
    }

    /// Lets the primary button up where the pointer is.
    pub fn release(&mut self) {
        let position = self.pointer;
        self.deliver(pointer(
            PointerAction::Released,
            position,
            Some(PointerButton::Primary),
        ));
    }

    /// Puts the button down over the middle of the control that says `text`, and leaves it down.
    ///
    /// # Panics
    ///
    /// Panics when nothing in the document says it, for the reason [`Self::click_saying`] does.
    pub fn press_saying(&mut self, text: &str) {
        let at = self
            .census()
            .control(text)
            .and_then(|node| node.centre())
            .unwrap_or_else(|| panic!("nothing laid out says {text:?} to press"));
        self.move_to(at);
        self.press();
    }

    /// Moves to `at` and clicks there, exactly as a mouse does.
    pub fn click(&mut self, at: Point<DevicePx, Device>) {
        self.move_to(at);
        self.press_release();
    }

    /// Clicks the middle of the control that says `text`.
    ///
    /// # Panics
    ///
    /// Panics when nothing in the document says it, because a fixture that quietly clicked nowhere
    /// would report the same thing as a control that did not answer.
    pub fn click_saying(&mut self, text: &str) {
        let at = self
            .census()
            .control(text)
            .and_then(|node| node.centre())
            .unwrap_or_else(|| panic!("nothing laid out says {text:?} to click"));
        self.click(at);
    }

    /// Delivers everything `acts` describes in one batch, exactly as a compositor delivers one.
    ///
    /// The whole difference from calling the acts one at a time is that no frame runs between
    /// them: the second event is routed into the document the first one left behind, and whether
    /// that document is the one the first event *asked* for is the question. A surface opened by a
    /// press exists only once the reactive work has run, so a key delivered in the same batch is
    /// the shortest test there is of whether that happens in time.
    ///
    /// # Panics
    ///
    /// Panics when nothing in the document says what a [`Act::ClickSaying`] names, because a
    /// fixture that quietly clicked nowhere reports the same thing as a control that did not
    /// answer.
    pub fn burst(&mut self, acts: &[Act]) {
        let mut events = Vec::new();
        for act in acts {
            match act {
                Act::ClickSaying(text) => {
                    let at = self
                        .census()
                        .control(text)
                        .and_then(|node| node.centre())
                        .unwrap_or_else(|| panic!("nothing laid out says {text:?} to click"));
                    let position = self.css(at);
                    let button = Some(PointerButton::Primary);
                    if self.pointer == Point::new(CssPx(0.0), CssPx(0.0)) {
                        events.push(pointer(PointerAction::Entered, position, None));
                    }
                    self.pointer = position;
                    events.push(pointer(PointerAction::Moved, position, None));
                    events.push(pointer(PointerAction::Pressed, position, button));
                    events.push(pointer(PointerAction::Released, position, button));
                }
                Act::Key(key) => {
                    events.push(named(*key, KeyState::Pressed, Modifiers::NONE));
                    events.push(named(*key, KeyState::Released, Modifiers::NONE));
                }
            }
        }
        let surface = zgui::platform::Surface::id(self.surface().as_ref());
        self.harness.deliver_all(surface, events);
        self.harness.settle(64);
    }

    /// Moves the clock on in output-frame steps, running the frames each step asks for.
    ///
    /// Stepped rather than jumped, because an animation is sampled once per frame: a clock that
    /// went from the start of a transition to past its end in one move produces a single sample,
    /// which is the one shape in which an interpolation that never ran and one that ran correctly
    /// look the same.
    pub fn hold(&mut self, total: Duration) {
        let steps = (total.as_secs_f64() / TICK.as_secs_f64()).ceil() as u32;
        for _ in 0..steps {
            self.harness.advance(TICK);
            self.harness.settle(64);
        }
        self.harness.settle(64);
    }

    /// Presses and releases a named key with `modifiers` held.
    pub fn key_with(&mut self, key: NamedKey, modifiers: Modifiers) {
        self.deliver(named(key, KeyState::Pressed, modifiers));
        self.deliver(named(key, KeyState::Released, modifiers));
    }

    /// Presses and releases a named key.
    pub fn key(&mut self, key: NamedKey) {
        self.key_with(key, Modifiers::NONE);
    }

    /// Types one character wherever the keyboard is.
    ///
    /// Nothing is aimed at a node: the event goes to the surface and the framework routes it to
    /// whatever has focus, so a keystroke after a click that landed nowhere is typed nowhere.
    pub fn type_char(&mut self, character: char) {
        let event = KeyEvent::character(character.to_string());
        for state in [KeyState::Pressed, KeyState::Released] {
            self.deliver(SurfaceEvent::Key {
                state,
                event: event.clone(),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
    }

    /// Which node has keyboard focus.
    pub fn focused(&self) -> Option<zgui::view::NodeId> {
        use zgui::reactive::prelude::GetUntracked;
        self.handles.host.focused().get_untracked()
    }

    /// What the focused node says.
    pub fn focused_text(&self) -> String {
        self.focused()
            .map(|node| self.handles.dom.text_content(node))
            .unwrap_or_default()
    }

    /// The surface everything this window publishes goes to.
    ///
    /// # Panics
    ///
    /// Panics when none was created, because every claim below it would be a claim about a window
    /// that never opened.
    pub fn surface(&self) -> std::sync::Arc<zgui_platform_headless::OffscreenSurface> {
        self.harness
            .platform()
            .offscreens()
            .first()
            .cloned()
            .expect("a surface was created")
    }

    /// What a screen reader would say about whatever holds the keyboard.
    pub fn announced_focus(&self) -> Option<Announced> {
        reader::focused(&self.surface())
    }

    /// Everything a screen reader would meet in the window, in tree order.
    pub fn announced(&self) -> Vec<Announced> {
        reader::everything(&self.surface())
    }

    /// Whether `text` is on the page: laid out, and not collapsed to nothing by anything holding it.
    ///
    /// Both halves matter. A control that folds its contents away — an accordion, a collapsible, a
    /// tab panel — leaves the text *in the document* with a box of its own, and only the wrapper
    /// around it goes to zero. Asking whether anything with that text has a box would call a folded
    /// section open.
    pub fn shows(&self, text: &str) -> bool {
        self.census().shows(text)
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

/// A named key going `state`, with the physical key filled in as a backend fills it.
fn named(key: NamedKey, state: KeyState, modifiers: Modifiers) -> SurfaceEvent {
    let physical = match key {
        NamedKey::Tab => PhysicalKey::Code(zgui::vocab::KeyCode::Tab),
        NamedKey::Enter => PhysicalKey::Code(zgui::vocab::KeyCode::Enter),
        NamedKey::Space => PhysicalKey::Code(zgui::vocab::KeyCode::Space),
        NamedKey::Escape => PhysicalKey::Code(zgui::vocab::KeyCode::Escape),
        NamedKey::ArrowDown => PhysicalKey::Code(zgui::vocab::KeyCode::ArrowDown),
        NamedKey::ArrowUp => PhysicalKey::Code(zgui::vocab::KeyCode::ArrowUp),
        NamedKey::ArrowRight => PhysicalKey::Code(zgui::vocab::KeyCode::ArrowRight),
        NamedKey::ArrowLeft => PhysicalKey::Code(zgui::vocab::KeyCode::ArrowLeft),
        NamedKey::Home => PhysicalKey::Code(zgui::vocab::KeyCode::Home),
        NamedKey::End => PhysicalKey::Code(zgui::vocab::KeyCode::End),
        _ => PhysicalKey::Unidentified(0),
    };
    SurfaceEvent::Key {
        state,
        event: KeyEvent::named(key, physical),
        modifiers,
        timestamp: Timestamp::ORIGIN,
    }
}
