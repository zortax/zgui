//! Turning the events that arrived into handler calls, and carrying out what they asked for.
//!
//! The sequence is fixed and is not free. Interaction state is written **before** listeners are
//! resolved, so a handler that reads a computed style sees the state its own event produced. The
//! framework's own behaviour — focusing what was pressed, activating what was released, scrolling
//! what the wheel was over — is *computed* alongside the listeners and carried out **after** every
//! one of them has run, because until then nothing knows whether one of them took responsibility
//! for the event.

use zgui_platform::SurfaceEvent;
use zgui_vocab::{EventKind, FocusCause, FocusEvent, Modifiers, Payload, Timestamp};

/// One focus move, waiting to be announced to the document.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FocusMoved {
    /// What held focus before, if anything did.
    pub(crate) leaving: Option<zgui_dom::NodeKey>,
    /// What holds it now, if anything does.
    pub(crate) arrived: Option<zgui_dom::NodeKey>,
    /// Why it moved.
    pub(crate) cause: FocusCause,
}

use crate::dispatch::{self, HostSink};
use crate::host::Command;
use crate::window::Window;

impl Window {
    /// Dispatches everything that arrived since the last frame, and reports what it left owing.
    ///
    /// A batch is several events, and the second one is routed into whatever the first one made.
    /// A window system hands over everything that arrived while the last frame was being drawn, so
    /// a press and the key that follows it are ordinarily in the same batch — and a press that
    /// opens a surface opens it by writing a signal, which builds nothing until the reactive work
    /// is run. Draining the batch without running it between events resolves every later event
    /// against the document as it was *before* the batch: the key is delivered to a window in
    /// which the surface that should have heard it does not exist yet, and it is not delivered
    /// again. Escape pressed a moment after the click that opened a dialog is then silently lost,
    /// and the dialog stays up.
    ///
    /// So each event's consequences are settled before the next one is routed. What that costs is
    /// one poll of a pool with nothing in it for every event that wrote nothing, which is what a
    /// stream of pointer moves is.
    pub(crate) fn drain_input(&mut self, timestamp: Timestamp) -> bool {
        let events = core::mem::take(&mut self.queued);
        let last = events.len().saturating_sub(1);
        let mut owed = false;
        for (position, event) in events.iter().enumerate() {
            self.dispatch_one(event, timestamp);
            if position < last {
                owed |= self.settle_between_events(timestamp);
            }
        }
        owed
    }

    /// Runs what the event just dispatched asked for, before the next one is routed.
    ///
    /// The same two stages the frame runs after the whole batch, and in the same order: the
    /// reactive work, which is where an effect mounts what a handler opened, and then the commands
    /// its handlers issued, which is where a focus move a listener asked for actually happens.
    /// Nothing after those is repeated — a document that gained elements has no boxes for them
    /// until this frame lays out, so an event routed here reaches them by the path a listener
    /// takes rather than by a hit test.
    ///
    /// Answers whether the flush left work for another frame.
    fn settle_between_events(&mut self, timestamp: Timestamp) -> bool {
        let flush = zgui_reactive::flush();
        self.carry_out_commands(timestamp);
        flush.needs_another_frame
    }

    /// Dispatches one platform event.
    fn dispatch_one(&mut self, event: &SurfaceEvent, timestamp: Timestamp) {
        // The surface's own keyboard focus reaches no element and dispatches nothing, so it is
        // taken before the routing: what it changes is what the window is holding on the document's
        // behalf — a composition, a value nobody has been told about, a pointer's leavings.
        if let Some(focused) = Self::surface_focus_of(event) {
            self.surface_focus_changed(focused, timestamp);
            return;
        }
        let modifiers = event.modifiers().unwrap_or(Modifiers::NONE);
        // How the user is interacting right now, remembered for the focus moves scripts make: a
        // surface opened by a click that focuses its first field is acting for the pointer, and
        // the ring follows the interaction rather than the API that moved the focus.
        match event {
            SurfaceEvent::Key {
                state: zgui_vocab::KeyState::Pressed,
                ..
            } => self.focus_modality = zgui_input::FocusSource::Keyboard,
            SurfaceEvent::Pointer {
                action: zgui_vocab::PointerAction::Pressed,
                ..
            } => self.focus_modality = zgui_input::FocusSource::Pointer,
            _ => {}
        }
        let Some((kind, payload)) = event.to_dispatch() else {
            return;
        };

        // Everything the route needs belongs to this frame, so it is assembled per event and held
        // by nothing: a router that kept last frame's geometry would aim at where things were.
        // What a pointer event did to the set of elements the pointer is inside, taken out of the
        // routing block because the queue it goes on is on the window and the block holds the
        // window's document borrowed.
        let mut crossed: Option<(zgui_input::Moved, zgui_vocab::PointerEvent)> = None;
        let (target, steps, default) = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let filter = self.engine.filter();
            let world = zgui_input::World {
                document: &document,
                layout: &layout,
                hit: &self.hit,
                clips: &self.scene.clips,
                spatial: &self.scene.spatial,
                scale: zgui_geom::Scale::new(self.scale),
                filter: &filter,
            };
            match event {
                SurfaceEvent::Pointer { action, event, .. } => {
                    let routed = self
                        .router
                        .pointer(&world, *action, event, modifiers, timestamp);
                    if !routed.hover.is_empty() {
                        crossed = Some((routed.hover.clone(), *event));
                    }
                    (routed.target(), routed.steps.to_vec(), routed.default)
                }
                SurfaceEvent::Wheel { event, .. } => {
                    let routed = self.router.wheel(&world, event);
                    (routed.target(), routed.steps.to_vec(), routed.default)
                }
                SurfaceEvent::Key { state, event, .. } => {
                    let shortcuts = self.host.window_shortcuts();
                    let routed = self
                        .router
                        .key(&world, *state, event, modifiers, &shortcuts);
                    (routed.target(), routed.steps.to_vec(), routed.default)
                }
                // An input method's edits and a drop are aimed at whatever has focus. No
                // interaction state changes and there is no framework behaviour to compute: the
                // path is resolved, walked, and that is all.
                _ => {
                    let chain = match self.router.interaction().focus.focused() {
                        Some(node) => zgui_input::HitChain::to_root(document.store(), node),
                        None => world.root_chain(),
                    };
                    let mut plan = zgui_input::dispatch::Plan::default();
                    zgui_input::dispatch::resolve(document.store(), &chain, kind, &mut plan);
                    (chain.target(), plan.steps().to_vec(), None)
                }
            }
        };

        // Before the event that caused them, because that is the order a handler needs and the
        // order the browser event model uses: a control is told the pointer arrived on it before it
        // is told the pointer moved within it. Both are dispatched from here rather than from
        // inside the routing above, where the document's change batch is still open.
        if let Some((moved, pointer)) = crossed {
            self.note_crossings(&moved, pointer);
            self.announce_crossings(timestamp);
        }

        if !self
            .binding
            .before_dispatch(target.map(zgui_view_dom::id::to_view), kind)
        {
            return;
        }

        let dispatched = {
            let host = std::rc::Rc::clone(&self.host);
            let mut sink = HostSink::new(&host);
            dispatch::run(
                self, &steps, kind, target, &payload, modifiers, timestamp, &mut sink,
            )
        };

        // Whether an input method has provisional text on the screen right now, read *before* this
        // event is allowed to do anything, because the event may be the one that ends it.
        let composing = self.ime.is_composing();
        // Editing is the first of the framework's own behaviours, and it takes the event away from
        // the rest of them: space and enter activate whatever has focus, and a field that both
        // typed a space and clicked itself is neither. A key the editing model refused still falls
        // through, which is what leaves tab moving the focus out of a field.
        let typed = dispatched.default_allowed && self.edit_focused(event, timestamp);
        // A key that arrives during a composition belongs to the composition even though the input
        // method did not consume it — the window system forwards every one of them. Left to take a
        // framework default it moves the focus or activates a control out from under provisional
        // text that is still on the screen, and the commit that follows lands in whatever gained
        // the focus.
        let held = composing && matches!(event, SurfaceEvent::Key { .. });
        if dispatched.default_allowed
            && !typed
            && !held
            && let Some(default) = default
        {
            self.carry_out_default(default, timestamp);
        }
        // After the focus default rather than before it, because a press on a field that did not
        // have focus both focuses it and puts the caret where the press landed, and the second of
        // those is meaningless if the first has not happened yet.
        if dispatched.default_allowed {
            self.point_at_text(event, target);
        }
        // A finger produces presses and moves and nothing else; what those *mean* is read here and
        // acted on after the listeners, exactly as the wheel's own default is. A handler that took
        // responsibility for the press has already stopped the reading from reaching a scroll,
        // because the recogniser only ever sees what the platform sent.
        if let SurfaceEvent::Pointer {
            action,
            event,
            timestamp: sent,
            ..
        } = event
        {
            // The *event's* stamp, not the frame's. A flick's speed is the distance between two
            // contacts divided by the time between them, and every event drained in one frame
            // shares that frame's stamp — so reading the frame's clock here makes every gesture
            // instantaneous and every lift a dead stop.
            let read = self.gestures.pointer(*action, event, *sent);
            if !read.is_empty() {
                self.carry_out_gestures(&read);
            }
        }
        // Nothing is asked for here. Events are drained at the top of a frame and everything a
        // handler wrote is styled, laid out and painted further down the same one, so a request
        // made from this point would buy a second frame that damages nothing and presents a
        // surface identical to the one just presented. A handler whose work genuinely outlives the
        // frame — a task that finishes later, an effect scheduled from a callback — asks for
        // itself, from where it finishes.
    }

    /// Types into whatever editable element has focus.
    ///
    /// Editing is a default action and runs here, after every listener on the path: a field with a
    /// handler that took responsibility for the key — a numeric-only field, a shortcut bar —
    /// types nothing, which is what `prevent_default` has to mean for a key event.
    ///
    /// A composition is followed whether or not anything has focus, because the platform reports
    /// it against the surface rather than against an element; what it does to a document is the
    /// editing model's business and happens only when there is an element to do it to.
    ///
    /// Answers whether the editing model took the event, which is what stops the same press from
    /// also taking one of the framework's other behaviours.
    ///
    /// An edit that changed the text is announced as an [`Input`](zgui_vocab::ValueChange::Input)
    /// event on the element, after the caret has been written: a view learns what a field now
    /// holds by listening, because the document is not something a view reads.
    fn edit_focused(&mut self, event: &SurfaceEvent, timestamp: Timestamp) -> bool {
        let Some(node) = self.router.interaction().focus.focused() else {
            if let SurfaceEvent::Ime(ime) = event {
                self.ime.observe(ime);
            }
            return false;
        };
        let edited = match event {
            SurfaceEvent::Key {
                state: zgui_vocab::KeyState::Pressed,
                event: key,
                modifiers,
                ..
            } => {
                // Vertical motion is answered here rather than in the editing model, because the
                // model is deliberately layout-free and which offset is "one line up" is a
                // question only the lines this frame laid out can answer.
                if let Some((down, extend)) = Self::vertical_motion(key, *modifiers) {
                    self.move_caret_vertically(node, down, extend)
                } else {
                    let document = self.document.borrow();
                    let edited = self.editors.key(&document, node, key, *modifiers);
                    drop(document);
                    // Any other key ends the run of vertical motions, so the next one aims for
                    // the column the caret is actually in.
                    if edited.handled {
                        self.vertical_goal = None;
                    }
                    edited
                }
            }
            SurfaceEvent::Ime(ime) => {
                self.ime.observe(ime);
                let document = self.document.borrow();
                self.editors.ime(&document, node, ime)
            }
            _ => return false,
        };
        if let Some(selection) = edited.selection.clone() {
            self.host.write_selection(node, selection);
        }
        // A cut or a copy asks for text to be put on the clipboard. It is recorded rather than
        // written here because the clipboard is reachable only from the turn of the loop that holds
        // the platform context, and this is inside a frame. Dropping it is a cut that deletes the
        // text and copies nothing, which loses what the user asked to keep.
        if let Some(text) = edited.clipboard.clone() {
            self.clipboard.push(text);
        }
        if edited.handled {
            // The caret moved, so the blink starts again from now: a caret that kept its own phase
            // would be dark for half the keystrokes that produced it.
            self.carets.restart(self.clock.now());
            self.report_caret();
        }
        // After the caret, so that a listener reading the field's selection sees where this edit
        // left it rather than where the one before it did. Provisional text counts: what an input
        // method is showing *is* what the field holds right now, and a value bound to it that
        // ignored composition would jump from the old text to the committed one with nothing in
        // between.
        if let Some(value) = edited.value {
            self.report_input(node, value, edited.selection, timestamp);
        }
        edited.handled
    }

    /// The vertical motion a key press asks for, or nothing when it asks for none.
    ///
    /// Plain and shifted arrows only: an arrow held with a command modifier belongs to whatever
    /// binds it, and answering it here would type over a shortcut.
    fn vertical_motion(
        event: &zgui_vocab::KeyEvent,
        modifiers: Modifiers,
    ) -> Option<(bool, bool)> {
        if modifiers.control() || modifiers.alt() || modifiers.meta() {
            return None;
        }
        let down = match event.key {
            zgui_vocab::Key::Named(zgui_vocab::NamedKey::ArrowDown) => true,
            zgui_vocab::Key::Named(zgui_vocab::NamedKey::ArrowUp) => false,
            _ => return None,
        };
        Some((down, modifiers.shift()))
    }

    /// Moves the caret one line up or down, in the lines this frame laid out.
    ///
    /// A motion past the first or last line goes to the text's own edge, which is also the whole
    /// of what the keys do in a single-line field. The column aimed for is kept across the run of
    /// motions, so stepping through a short line does not lose it.
    fn move_caret_vertically(
        &mut self,
        node: zgui_dom::NodeKey,
        down: bool,
        extend: bool,
    ) -> crate::editing::Edited {
        let Some(selection) = self.editors.selection(node) else {
            return crate::editing::Edited::default();
        };
        let stepped = {
            let layout = self.layout.borrow();
            crate::caret::Located::of(&layout, &self.text, node).and_then(|located| {
                located.line_step(
                    selection.focus,
                    selection.affinity,
                    down,
                    self.vertical_goal,
                )
            })
        };
        let (focus, affinity) = match stepped {
            Some((focus, affinity, goal)) => {
                self.vertical_goal = Some(goal);
                (focus, affinity)
            }
            None => {
                // No line in that direction: the edge of the text, which ends the run.
                self.vertical_goal = None;
                let focus = if down {
                    self.editors.value(node).map_or(0, |text| text.len())
                } else {
                    0
                };
                (focus, zgui_edit::Affinity::Downstream)
            }
        };
        let anchor = if extend { selection.anchor } else { focus };
        let document = self.document.borrow();
        self.editors.place(&document, node, anchor, focus, affinity)
    }

    /// Tells the surface where the caret is now, after an edit moved it.
    ///
    /// Separate from [`report_text_input`](Window::report_text_input), which answers the question
    /// focus asks — *is* text being typed, and where. This one answers the question typing asks,
    /// and it is the one that has to be cheap: it is reached on every keystroke, and the surface is
    /// told only when the answer really changed.
    pub(crate) fn report_caret(&mut self) {
        let Some(node) = self.ime.target() else {
            return;
        };
        let Some(area) = self.caret_area(node) else {
            return;
        };
        if let Some(zgui_input::ime::Told::Enabled(area)) = self.ime.caret_moved(area) {
            self.surface.set_text_input(Some(area));
        }
    }

    /// Where an editable element's caret is reported to be.
    ///
    /// The caret's own rectangle when this frame planned one, so a candidate window opens beside
    /// the insertion point. A field that has not been through a frame yet, or one whose model has
    /// not been attached, has no such rectangle, and the honest answer for it is the box its text
    /// is in: a candidate window beside the field rather than beside the insertion point is
    /// visible, and is not wrong about which surface or which element is being typed into.
    fn caret_area(&self, node: zgui_dom::NodeKey) -> Option<zgui_platform::TextInput> {
        if let Some(caret) = self.caret_rect() {
            return Some(zgui_input::ime::caret_area(
                caret.origin,
                caret.size.height,
                zgui_geom::Scale::new(self.scale),
                zgui_platform::TextInputPurpose::Normal,
            ));
        }
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(node).first()?;
        let box_ = layout.layout_of(first)?.border_box();
        Some(zgui_input::ime::caret_area(
            box_.origin,
            box_.size.height,
            zgui_geom::Scale::new(self.scale),
            zgui_platform::TextInputPurpose::Normal,
        ))
    }

    /// Tells the surface whether text is being typed, and where.
    ///
    /// Called when focus moves. Until an input method is told text input is wanted it starts no
    /// composition at all, so a field that never reports this is a field a Japanese keyboard
    /// cannot type into.
    ///
    /// The area reported is [`caret_area`](Window::caret_area)'s, which is the same rectangle
    /// typing reports. Answering with the element's box instead would be a caret that jumps back
    /// to the corner of the field every time the surface hands the keyboard back — the candidate
    /// window of a person who alt-tabbed away mid-word opens at the start of the field they are
    /// halfway along.
    pub(crate) fn report_text_input(&mut self) {
        let focused = self.router.interaction().focus.focused();
        let editable = focused.filter(|node| {
            let document = self.document.borrow();
            crate::editing::Editors::is_editable(&document, *node)
        });
        let area = editable.and_then(|node| self.caret_area(node));
        if let Some(told) = self.ime.focused(editable, area) {
            self.surface.set_text_input(match told {
                zgui_input::ime::Told::Enabled(area) => Some(area),
                zgui_input::ime::Told::Disabled => None,
            });
        }
    }

    /// Carries out the framework's own behaviour for one event, when no handler refused it.
    fn carry_out_default(&mut self, default: zgui_input::FrameworkDefault, timestamp: Timestamp) {
        use zgui_input::FrameworkDefault;
        match default {
            FrameworkDefault::Focus { node, source } => self.move_focus(node, source, timestamp),
            FrameworkDefault::Activate(node) => {
                self.synthesize(
                    zgui_view_dom::id::to_view(node),
                    EventKind::Click,
                    timestamp,
                );
            }
            FrameworkDefault::Scroll {
                container,
                delta,
                phase,
            } => self.scroll_by(container, delta, phase),
            FrameworkDefault::ScrollAlong {
                container,
                axis,
                to,
            } => self.scroll_along(container, axis, to),
            FrameworkDefault::ScrollPage {
                container,
                axis,
                forward,
            } => self.scroll_page(container, axis, forward),
            FrameworkDefault::MoveFocus(direction) => self.step_focus(direction, timestamp),
            // A behaviour this build has never heard of is not guessed at. Doing nothing is what a
            // handler that cancelled the default would have produced anyway.
            _ => {}
        }
    }

    /// Carries out every command a handler issued, and everything those in turn issue.
    ///
    /// Bounded: a command that issues a command that issues a command settles or is dropped, so a
    /// handler that focuses whatever gains focus cannot hold the frame.
    pub(crate) fn carry_out_commands(&mut self, timestamp: Timestamp) {
        for _ in 0..8u8 {
            // Before the commands are taken, because a focus move made by the event that has just
            // finished has to be announced before anything that move causes is acted on — and
            // because a handler for it issues commands of its own, which this loop then drains.
            self.announce_focus(timestamp);
            // The crossings a frame's own re-hit found: content that moved under a cursor that did
            // not. Announced from the same loop, because a handler for one of them issues commands
            // exactly as a handler for anything else does.
            self.announce_crossings(timestamp);
            let commands = self.host.take_commands();
            if commands.is_empty() {
                return;
            }
            for command in commands {
                self.carry_out(command, timestamp);
            }
        }
        self.announce_focus(timestamp);
        tracing::warn!(
            target: "zgui::input",
            "the commands a dispatch issued did not settle in eight rounds; the rest are dropped"
        );
        let _ = self.host.take_commands();
    }

    /// Puts the keyboard inside every surface that trapped focus and asked to be entered.
    ///
    /// Runs after layout, and that is the whole of why it is a stage of its own. A trap goes up
    /// from a render effect, while the surface it confines is still being built: it has no boxes
    /// yet, so nothing inside it can be focused and asking there answers with nothing. By this
    /// point the surface has been measured and its first control is somewhere on the screen.
    pub(crate) fn enter_owed_focus_traps(&mut self, timestamp: Timestamp) {
        if !self.host.owes_focus() {
            return;
        }
        self.host.enter_owed_traps();
        self.carry_out_commands(timestamp);
    }

    /// Carries out one command.
    fn carry_out(&mut self, command: Command, timestamp: Timestamp) {
        match command {
            Command::Focus(node) => {
                let key = node.and_then(zgui_view_dom::id::to_document);
                self.move_focus(key, zgui_input::FocusSource::Script, timestamp);
            }
            Command::CapturePointer(node) => {
                if let Some(key) = zgui_view_dom::id::to_document(node) {
                    let pointer = zgui_vocab::PointerId::MOUSE;
                    self.router.capture_mut().set(pointer, key);
                }
            }
            Command::ReleasePointer(node) => {
                if let Some(key) = zgui_view_dom::id::to_document(node) {
                    self.router.capture_mut().release_node(key);
                }
            }
            Command::Select { node, range } => {
                let Some(key) = zgui_view_dom::id::to_document(node) else {
                    return;
                };
                let edited = {
                    let document = self.document.borrow();
                    self.editors.select(&document, key, range)
                };
                // The model clamps to the text it holds, so what it settled on is what the record
                // has to say too: two answers here is a field that types where nothing is drawn.
                if let Some(selection) = edited.selection {
                    self.host.write_selection(key, selection);
                }
            }
            Command::SetValue { node, text } => {
                let Some(key) = zgui_view_dom::id::to_document(node) else {
                    return;
                };
                let loaded = {
                    let document = self.document.borrow();
                    self.editors.load(&document, key, &text)
                };
                // A value that was already there moved nothing, and writing the caret back would
                // undo a selection made in the same frame by something that ran before this.
                if let Some(selection) = loaded.selection {
                    self.host.write_selection(key, selection);
                }
                // No value event: an application that wrote this value knows what it wrote, and
                // announcing it back would drive the signal that produced it — which is a loop
                // whenever the two disagree about anything at all, including nothing.
                if loaded.changed {
                    self.report_caret();
                }
            }
            Command::InstallStylesheet { name, css } => self.install_view_sheet(&name, &css),
            Command::RemoveStylesheet { name } => self.remove_view_sheet(&name),
            Command::Synthesize { node, event } => self.synthesize(node, event, timestamp),
            Command::Scroll {
                node,
                target,
                behavior,
            } => self.carry_out_scroll(node, target, behavior),
            Command::FreezeScrolling(frozen) => self.freeze_scrolling(frozen),
        }
    }

    /// Moves focus, writing the state bits that decide whether a ring is shown.
    ///
    /// Leaving a field that was typed into announces its value as settled, once, which is the
    /// moment a form validates on. It is announced *after* focus has moved, so a handler that
    /// reads what holds focus sees the answer the user's own action produced rather than the one
    /// they were leaving.
    pub(crate) fn move_focus(
        &mut self,
        node: Option<zgui_dom::NodeKey>,
        source: zgui_input::FocusSource,
        timestamp: Timestamp,
    ) {
        let leaving = self.router.interaction().focus.focused();
        {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let filter = self.engine.filter();
            let world = zgui_input::World {
                document: &document,
                layout: &layout,
                hit: &self.hit,
                clips: &self.scene.clips,
                spatial: &self.scene.spatial,
                scale: zgui_geom::Scale::new(self.scale),
                filter: &filter,
            };
            // A script's move borrows the ring decision from how the user last interacted; the
            // cause recorded below stays programmatic, because *why* focus moved is a different
            // question from whether to draw a ring for it.
            let ring_source = match source {
                zgui_input::FocusSource::Script => self.focus_modality,
                other => other,
            };
            self.router.focus(&world, node, ring_source);
        }
        let arrived = self.router.interaction().focus.focused();
        self.host.publish_focus(arrived);
        self.attach_caret(arrived);
        self.report_text_input();
        if leaving != arrived {
            self.pending_focus.push(FocusMoved {
                leaving,
                arrived,
                cause: match source {
                    zgui_input::FocusSource::Pointer => FocusCause::Pointer,
                    zgui_input::FocusSource::Keyboard => FocusCause::Keyboard,
                    // A source this build has not heard of is the program's doing as far as a ring
                    // is concerned, which is the reading that shows one rather than hiding it.
                    _ => FocusCause::Programmatic,
                },
            });
        }
        if let Some(left) = leaving.filter(|left| Some(*left) != arrived) {
            self.report_change(left, timestamp);
        }
    }

    /// Tells the document about every focus move that has happened since it was last asked.
    ///
    /// Focus moving is not only a bit of state. A tab strip that shows the panel the arrows land
    /// on, a radio group that answers the arrow keys, a menu that tracks which item the keyboard is
    /// on, a field that validates when it is left, a tooltip raised for somebody who reached its
    /// control without a pointer — every one of those is written as a handler for focus arriving or
    /// leaving, and none of them runs unless the events are dispatched. A window that moved the bit
    /// and said nothing left all of them working under a pointer and dead under a keyboard, which is
    /// the one kind of breakage no amount of using the program finds.
    ///
    /// **Deferred rather than immediate**, and that is the whole reason this is a queue. Focus moves
    /// as the framework's own default for an event that is still being carried out, with that
    /// event's change batch open on the document; dispatching from there would begin one dispatch
    /// inside another and re-enter the batch. So the move is recorded where it happens and
    /// announced from the same place every other consequence of a handler is carried out.
    ///
    /// Departure first, then arrival: a handler that asks what holds focus sees where focus is,
    /// not where it was on its way through.
    pub(crate) fn announce_focus(&mut self, timestamp: Timestamp) {
        // Taken rather than drained in place: a handler for one of these can move the focus again,
        // and the move it makes belongs to the next round rather than to this one.
        for moved in core::mem::take(&mut self.pending_focus) {
            let other = |key: Option<zgui_dom::NodeKey>| {
                key.map(|key| zgui_vocab::NodeId(zgui_view_dom::id::to_view(key).as_u64()))
            };
            if let Some(left) = moved.leaving {
                let payload = Payload::Focus(FocusEvent {
                    related: other(moved.arrived),
                    cause: moved.cause,
                });
                self.dispatch_synthetic(left, EventKind::FocusOut, payload, timestamp);
            }
            if let Some(gained) = moved.arrived {
                let payload = Payload::Focus(FocusEvent {
                    related: other(moved.leaving),
                    cause: moved.cause,
                });
                self.dispatch_synthetic(gained, EventKind::FocusIn, payload, timestamp);
            }
        }
    }

    /// Gives whatever gained focus a caret, and takes it away from whatever lost it.
    ///
    /// The model is attached here rather than on the first keystroke, because a caret is what tells
    /// somebody that a field is the one they are about to type into: a field that showed nothing
    /// until its first character had already been typed would be indistinguishable from one that
    /// does not have focus at all.
    ///
    /// Where the caret goes is whatever was already recorded for that element, so returning to a
    /// field puts the caret back where it was left rather than at the front of the text.
    fn attach_caret(&mut self, node: Option<zgui_dom::NodeKey>) {
        let editable = node.filter(|node| {
            let document = self.document.borrow();
            crate::editing::Editors::is_editable(&document, *node)
        });
        let Some(node) = editable else {
            self.carets.stop();
            return;
        };
        let held = self.host.selection_of(node).unwrap_or(0..0);
        {
            let document = self.document.borrow();
            self.editors.place(
                &document,
                node,
                held.start,
                held.end,
                zgui_edit::Affinity::Upstream,
            );
        }
        // Read back rather than taken from the response: a model that was already where it was
        // asked to go reports no *change*, and a record written only on a change is empty for the
        // commonest case there is — a fresh field, whose caret is at zero and stays there.
        if let Some(selection) = self.editors.selection(node) {
            self.host.write_selection(node, selection.range());
        }
        self.carets.restart(self.clock.now());
    }

    /// Moves focus sequentially, which is what the tab key asks for.
    fn step_focus(&mut self, direction: zgui_input::FocusDirection, timestamp: Timestamp) {
        let (sequence, current, wrap) = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let trap = self.host.active_trap();
            let root = trap
                .and_then(|(root, _)| zgui_view_dom::id::to_document(root))
                .or_else(|| {
                    document
                        .root_index()
                        .map(|index| document.store().key_of(index))
                });
            let Some(root) = root else {
                return;
            };
            let sequence =
                zgui_input::focus::order::focusables(document.store(), Some(&layout), root);
            (
                sequence,
                self.router.interaction().focus.focused(),
                trap.is_some_and(|(_, options)| options.wrap),
            )
        };
        let moved = zgui_input::focus::order::step(&sequence, current, direction, wrap);
        self.move_focus(moved, zgui_input::FocusSource::Keyboard, timestamp);
    }

    /// Dispatches an event on a node through the ordinary capture, target and bubble path.
    pub(crate) fn synthesize(
        &mut self,
        node: zgui_view::NodeId,
        event: EventKind,
        timestamp: Timestamp,
    ) {
        let Some(key) = zgui_view_dom::id::to_document(node) else {
            return;
        };
        // A synthesised activation is a pointer event at wherever the pointer is, because that is
        // what a handler reads off it. With no pointer on the surface it is at the origin.
        let position = self
            .router
            .pointers()
            .all()
            .next()
            .map(|(_, point)| {
                zgui_geom::Point::new(
                    zgui_geom::CssPx(point.x.0 / self.scale),
                    zgui_geom::CssPx(point.y.0 / self.scale),
                )
            })
            .unwrap_or(zgui_geom::Point::new(
                zgui_geom::CssPx(0.0),
                zgui_geom::CssPx(0.0),
            ));
        let payload = Payload::Pointer(zgui_vocab::PointerEvent::mouse(position));
        self.dispatch_synthetic(key, event, payload, timestamp);
    }

    /// Dispatches an event this process produced, down the path a real one would have taken.
    ///
    /// The path is the point. A framework that carried an activation out directly — calling the
    /// component's own callback, or writing its state — would reach a different set of listeners
    /// from the one a click reaches, and every wrapper that relies on capturing or on stopping
    /// propagation would work for a pointer and not for a keyboard or a screen reader.
    pub(crate) fn dispatch_synthetic(
        &mut self,
        key: zgui_dom::NodeKey,
        event: EventKind,
        payload: Payload,
        timestamp: Timestamp,
    ) {
        let steps = {
            let document = self.document.borrow();
            let chain = zgui_input::HitChain::to_root(document.store(), key);
            let mut plan = zgui_input::dispatch::Plan::default();
            zgui_input::dispatch::resolve(document.store(), &chain, event, &mut plan);
            plan.steps().to_vec()
        };
        dispatch::run_discarding(
            self,
            &steps,
            event,
            Some(key),
            &payload,
            Modifiers::NONE,
            timestamp,
        );
    }
}
