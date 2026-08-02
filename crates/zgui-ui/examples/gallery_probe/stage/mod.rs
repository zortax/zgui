//! The stage a script acts on: the real window, driven from inside the loop that owns it.
//!
//! Everything a step does goes through the same seam the compositor's own events go through, so
//! what is exercised is dispatch, restyle, layout, paint and present, in a window that is on the
//! screen and drawn by the machine's graphics device. What is *not* exercised is the windowing
//! backend's own decoding of a libinput event, which is why the run this instrument produces is
//! accompanied by a shorter one driven by the kernel.

pub(crate) mod census;
pub(crate) mod handles;
pub(crate) mod shot;
pub(crate) mod synth;

use std::time::{Duration, Instant};

use zgui::geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui::platform::{AppHandler, PlatformCx, SurfaceEvent, SurfaceId};
use zgui::vocab::{KeyState, Modifiers, NamedKey, PointerAction, PointerButton, ScrollPhase};

use crate::report::Report;
use crate::stage::census::Census;
use crate::stage::handles::Handles;

/// How long to let the compositor settle before a capture, so that what `grim` reads back is the
/// frame this process just presented rather than the one before it.
const COMPOSITED: Duration = Duration::from_millis(90);

/// How much real time one frame of a settle costs.
///
/// A refresh, at the slowest rate a desktop is likely to be running at, so that the number of
/// frames a step asks for reads as a length of time a person could have been looking for.
const REFRESH: Duration = Duration::from_millis(16);

/// The window, the application, and everything a step needs to act on them.
pub(crate) struct Stage<'a> {
    /// The application under the driver.
    inner: &'a mut dyn AppHandler,
    /// What the platform offers this turn.
    cx: &'a dyn PlatformCx,
    /// Which surface the events are for.
    surface: SurfaceId,
    /// The document's engine seams.
    handles: Handles,
    /// When the run started, which every timestamp is measured from.
    started: Instant,
    /// Where the pointer is, so that a press lands where the last move left it.
    pointer: Point<CssPx, Css>,
    /// Where the findings go.
    pub(crate) report: &'a mut Report,
}

impl<'a> Stage<'a> {
    /// A stage over `inner`, acting on `surface`.
    pub(crate) fn new(
        inner: &'a mut dyn AppHandler,
        cx: &'a dyn PlatformCx,
        surface: SurfaceId,
        handles: Handles,
        started: Instant,
        report: &'a mut Report,
    ) -> Self {
        Self {
            inner,
            cx,
            surface,
            handles,
            started,
            pointer: Point::new(CssPx(0.0), CssPx(0.0)),
            report,
        }
    }

    /// The document's engine seams.
    pub(crate) fn handles(&self) -> &Handles {
        &self.handles
    }

    /// Everything in the document, as it stands now.
    pub(crate) fn census(&self) -> Census {
        Census::take(&self.handles)
    }

    /// The window's drawable area, in device pixels from its own top-left corner.
    ///
    /// What a capture of the window is a picture of, and therefore the bounds any rectangle a
    /// picture is judged over has to be inside. A rectangle reaching past the edge — a panel
    /// taller than the window, or one that was measured before the window was made smaller —
    /// names pixels that are in no picture at all.
    pub(crate) fn window(&self) -> zgui::geom::Rect<DevicePx, Device> {
        let size = self
            .cx
            .surfaces()
            .into_iter()
            .find(|surface| surface.id() == self.surface)
            .map_or(Size::new(DevicePx(0.0), DevicePx(0.0)), |surface| {
                surface.size()
            });
        zgui::geom::Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), size)
    }

    /// How many device pixels one CSS pixel is.
    pub(crate) fn scale(&self) -> f32 {
        self.handles.host.scale()
    }

    /// A device-pixel point in the CSS pixels the platform reports input in.
    pub(crate) fn css(&self, at: Point<DevicePx, Device>) -> Point<CssPx, Css> {
        let scale = self.scale().max(0.01);
        Point::new(CssPx(at.x.0 / scale), CssPx(at.y.0 / scale))
    }

    /// The moment now is, as the platform stamps its events.
    fn now(&self) -> zgui::vocab::Timestamp {
        zgui::vocab::Timestamp::from_origin(self.started.elapsed())
    }

    /// Hands one event to the application.
    pub(crate) fn deliver(&mut self, event: SurfaceEvent) {
        self.inner.surface_event(self.cx, self.surface, event);
    }

    /// Runs `count` frames, so that everything the last event asked for has happened.
    ///
    /// A frame is asked for by name rather than waited for, because the loop that would deliver it
    /// is the one this call is inside. Each one costs a refresh of *real* time, and a deadline is
    /// reached before it, because much of what an event asks for is paced by a clock rather than
    /// by frames: a surface that fades out, a fold that opens, a bar that grows to its new value.
    /// Frames run back to back advance those by microseconds, so a run that asked for eight of
    /// them and then looked would be reading the first instant of every transition and calling it
    /// the result — and would report a menu that is on its way out as one that never closed.
    pub(crate) fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.inner.deadline_reached(self.cx);
            self.deliver(SurfaceEvent::RedrawRequested);
            let _ = self.inner.idle(self.cx);
            std::thread::sleep(REFRESH);
        }
    }

    /// Lets real time pass, running frames while it does, for anything paced by a clock.
    pub(crate) fn wait(&mut self, how_long: Duration) -> usize {
        let until = Instant::now() + how_long;
        let mut frames = 0;
        while Instant::now() < until {
            self.inner.deadline_reached(self.cx);
            self.deliver(SurfaceEvent::RedrawRequested);
            let _ = self.inner.idle(self.cx);
            frames += 1;
            std::thread::sleep(Duration::from_millis(4));
        }
        frames
    }

    /// Scrolls `node` into view and lets the frames that move it run.
    ///
    /// The gallery is three times taller than the window it opens in, so most of it is below the
    /// fold at any moment. A step that aimed at a panel without bringing it into view first would
    /// be aiming outside the surface, where nothing can be clicked and every claim would come back
    /// the same way: not there.
    pub(crate) fn reveal(&mut self, node: zgui::view::NodeId) {
        self.handles.host.scroll_to(
            node,
            zgui::view::ScrollTarget::IntoViewStart,
            zgui::view::ScrollBehavior::Instant,
        );
        self.settle(6);
    }

    /// Moves the pointer to `at`, in device pixels.
    pub(crate) fn move_to(&mut self, at: Point<DevicePx, Device>) {
        let position = self.css(at);
        let first = self.pointer == Point::new(CssPx(0.0), CssPx(0.0));
        self.pointer = position;
        let when = self.now();
        if first {
            self.deliver(synth::pointer(
                PointerAction::Entered,
                position,
                None,
                Modifiers::NONE,
                when,
            ));
        }
        self.deliver(synth::pointer(
            PointerAction::Moved,
            position,
            None,
            Modifiers::NONE,
            when,
        ));
        self.settle(2);
    }

    /// Takes the pointer off the window entirely.
    pub(crate) fn leave(&mut self) {
        let position = self.pointer;
        let when = self.now();
        self.deliver(synth::pointer(
            PointerAction::Left,
            position,
            None,
            Modifiers::NONE,
            when,
        ));
        self.pointer = Point::new(CssPx(0.0), CssPx(0.0));
        self.settle(2);
    }

    /// Presses and releases `button` where the pointer is.
    pub(crate) fn press_release(&mut self, button: PointerButton, modifiers: Modifiers) {
        let position = self.pointer;
        let when = self.now();
        self.deliver(synth::pointer(
            PointerAction::Pressed,
            position,
            Some(button),
            modifiers,
            when,
        ));
        self.settle(2);
        let when = self.now();
        self.deliver(synth::pointer(
            PointerAction::Released,
            position,
            Some(button),
            modifiers,
            when,
        ));
        self.settle(3);
    }

    /// Moves to `at` and clicks the primary button there.
    pub(crate) fn click(&mut self, at: Point<DevicePx, Device>) {
        self.move_to(at);
        self.press_release(PointerButton::Primary, Modifiers::NONE);
    }

    /// Moves to `at` and clicks the secondary button there.
    pub(crate) fn right_click(&mut self, at: Point<DevicePx, Device>) {
        self.move_to(at);
        self.press_release(PointerButton::Secondary, Modifiers::NONE);
    }

    /// Drags from `from` to `to`, with the button held the whole way.
    pub(crate) fn drag(&mut self, from: Point<DevicePx, Device>, to: Point<DevicePx, Device>) {
        self.move_to(from);
        let when = self.now();
        let position = self.pointer;
        self.deliver(synth::pointer(
            PointerAction::Pressed,
            position,
            Some(PointerButton::Primary),
            Modifiers::NONE,
            when,
        ));
        self.settle(1);
        for step in 1..=8 {
            let fraction = step as f32 / 8.0;
            let between = Point::new(
                DevicePx(from.x.0 + (to.x.0 - from.x.0) * fraction),
                DevicePx(from.y.0 + (to.y.0 - from.y.0) * fraction),
            );
            let position = self.css(between);
            self.pointer = position;
            let when = self.now();
            self.deliver(synth::pointer(
                PointerAction::Moved,
                position,
                None,
                Modifiers::NONE,
                when,
            ));
            self.settle(1);
        }
        let position = self.pointer;
        let when = self.now();
        self.deliver(synth::pointer(
            PointerAction::Released,
            position,
            Some(PointerButton::Primary),
            Modifiers::NONE,
            when,
        ));
        self.settle(3);
    }

    /// Turns a wheel by `lines` where the pointer is.
    pub(crate) fn wheel(&mut self, lines: (f32, f32)) {
        let position = self.pointer;
        let when = self.now();
        self.deliver(synth::wheel(position, lines, Modifiers::NONE, when));
        self.settle(3);
    }

    /// Scrolls a trackpad gesture of `pixels` where the pointer is.
    pub(crate) fn trackpad(&mut self, pixels: Size<CssPx, Css>) {
        let position = self.pointer;
        for phase in [ScrollPhase::Started, ScrollPhase::Moved, ScrollPhase::Ended] {
            let when = self.now();
            self.deliver(synth::trackpad(position, pixels, phase, when));
            self.settle(2);
        }
    }

    /// Presses and releases a named key.
    pub(crate) fn key(&mut self, key: NamedKey) {
        self.key_with(key, Modifiers::NONE);
    }

    /// Presses and releases a named key with `modifiers` held.
    pub(crate) fn key_with(&mut self, key: NamedKey, modifiers: Modifiers) {
        let when = self.now();
        self.deliver(synth::named(key, KeyState::Pressed, modifiers, when));
        self.settle(2);
        let when = self.now();
        self.deliver(synth::named(key, KeyState::Released, modifiers, when));
        self.settle(2);
    }

    /// Types `text`, one key at a time.
    pub(crate) fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            let one = character.to_string();
            let when = self.now();
            self.deliver(synth::character(
                &one,
                KeyState::Pressed,
                Modifiers::NONE,
                when,
            ));
            self.settle(1);
            let when = self.now();
            self.deliver(synth::character(
                &one,
                KeyState::Released,
                Modifiers::NONE,
                when,
            ));
            self.settle(1);
        }
        self.settle(2);
    }

    /// Whether anything saying exactly `text` is on the screen.
    ///
    /// See [`Census::shown`](crate::stage::census::Census::shown) for why this is not the same
    /// question as whether a box was produced.
    pub(crate) fn shown(&self, text: &str) -> bool {
        let census = self.census();
        census.showing(&self.handles, text)
    }

    /// Whether anything on a floating surface says exactly `text` and is on the screen.
    ///
    /// What a claim about a menu, a dialog or a tooltip asks, so that a page which happens to use
    /// the same word cannot answer for it. See
    /// [`Census::floating`](crate::stage::census::Census::floating).
    pub(crate) fn floating(&self, text: &str) -> bool {
        self.census().floating(&self.handles, text)
    }

    /// Why [`Stage::shown`] answers the way it does about `text`.
    pub(crate) fn presence(&self, text: &str) -> String {
        self.census().presence(&self.handles, text)
    }

    /// How many animations or transitions are running on `node` right now.
    ///
    /// The one thing about a component's *motion* that can be read from outside it. A thumb that
    /// slides and a bar that travels do so by transform, which is a paint-time property: the box
    /// the layout produced is in the same place throughout, so nothing about where either has got
    /// to is visible in the geometry. What is visible is that the animation exists and is running.
    pub(crate) fn animations(&self, node: zgui::view::NodeId) -> usize {
        self.handles.host.running_animations(node)
    }

    /// Which node has keyboard focus.
    pub(crate) fn focused(&self) -> Option<zgui::view::NodeId> {
        use zgui::reactive::prelude::GetUntracked;
        self.handles.host.focused().get_untracked()
    }

    /// Where the caret is in the focused element, as an offset into its text.
    ///
    /// The framework's own answer. There is no caret in the document to measure — the insertion
    /// point belongs to the editing model, which is the only thing that knows where it is, and it
    /// is drawn over the lines the frame laid out rather than placed as a box among them.
    pub(crate) fn caret(&self) -> Option<core::ops::Range<usize>> {
        let node = self.focused()?;
        self.handles.host.selection(node)
    }

    /// What the focused node says, for naming it in a report.
    pub(crate) fn focused_text(&self) -> String {
        self.focused()
            .map(|node| self.handles.dom.text_content(node))
            .unwrap_or_default()
    }

    /// Captures the window, under `name`, once the compositor has had the frame.
    pub(crate) fn shot(&mut self, name: &str) {
        self.settle(2);
        std::thread::sleep(COMPOSITED);
        if let Err(error) = shot::capture(name) {
            self.report.note("capture", &format!("{name}: {error}"));
        }
    }
}
