//! Delivering one event down to a node and back up again.

use zgui_geom::{Device, DevicePx, Point};
use zgui_view::{EventControl, EventCx, ListenerId, NodeId};
use zgui_vocab::{
    DefaultAction, EventKind, ListenerOptions, Listeners, Modifiers, Payload, Phase, RouteStep,
    Timestamp,
};

use crate::dom::{Handlers, RecordingDom};
use crate::host::ScriptedHost;
use crate::input::sink::{Command, Commands};
use crate::transcript::{Op, Transcript};

/// What one dispatch did.
#[derive(Debug)]
pub struct Delivered {
    /// The node the event was aimed at, if it landed on one.
    pub target: Option<NodeId>,
    /// The path it travelled, root first.
    pub path: Vec<NodeId>,
    /// Which handlers ran, in order.
    pub ran: Vec<(NodeId, Phase)>,
    /// Whether the framework's own behaviour survived every handler.
    pub default: DefaultAction,
    /// What the handlers asked for, in order.
    pub commands: Vec<Command>,
}

impl Delivered {
    /// Whether any handler ran at all.
    pub fn reached_anything(&self) -> bool {
        !self.ran.is_empty()
    }
}

/// Sends events into a tree the way a window does.
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui_geom::{DevicePx, Point, Rect, Size};
/// use zgui_interned::ElementName;
/// use zgui_testkit_view::{Dispatcher, RecordingDom, ScriptedHost};
/// use zgui_view::{DocumentId, Dom, ListenerOptions};
/// use zgui_vocab::EventKind;
///
/// let dom = RecordingDom::new(DocumentId::FIRST);
/// let host = ScriptedHost::new();
/// let root = dom.create_element(ElementName::new("root"));
/// let button = dom.create_element(ElementName::new("control"));
/// dom.insert(root, button, None);
/// host.set_border_box(
///     button,
///     Rect::new(
///         Point::new(DevicePx(0.0), DevicePx(0.0)),
///         Size::new(DevicePx(80.0), DevicePx(24.0)),
///     ),
/// );
///
/// let pressed = Rc::new(Cell::new(0));
/// let count = Rc::clone(&pressed);
/// dom.add_listener(
///     button,
///     EventKind::Click,
///     ListenerOptions::DEFAULT,
///     Rc::new(move |_| count.set(count.get() + 1)),
/// );
///
/// let dispatcher = Dispatcher::new(&dom, &host, root);
/// let delivered = dispatcher.click_at(Point::new(DevicePx(10.0), DevicePx(10.0)));
///
/// assert_eq!(delivered.target, Some(button));
/// assert_eq!(pressed.get(), 1);
/// ```
pub struct Dispatcher<'a> {
    /// The tree events are aimed into.
    dom: &'a RecordingDom,
    /// Where its boxes are.
    host: &'a ScriptedHost,
    /// The element a hit test starts from.
    root: NodeId,
    /// The handlers, taken from the tree once.
    handlers: Handlers,
    /// Where handler runs and commands are recorded.
    transcript: Transcript,
    /// Which modifiers are held down while every event this dispatcher sends is delivered.
    modifiers: Modifiers,
}

impl<'a> Dispatcher<'a> {
    /// A dispatcher over `dom`, aiming into `root`'s subtree.
    pub fn new(dom: &'a RecordingDom, host: &'a ScriptedHost, root: NodeId) -> Self {
        Self {
            dom,
            host,
            root,
            handlers: dom.handlers(),
            transcript: dom.transcript(),
            modifiers: Modifiers::NONE,
        }
    }

    /// The same dispatcher, with `modifiers` held down.
    ///
    /// A handler reads the modifiers off the event rather than off a keyboard, so this is the only
    /// way a shortcut can be exercised at all — and a harness that could not hold Shift would make
    /// every modified shortcut in a component library untestable while every unmodified one passed.
    ///
    /// ```
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    /// use zgui_interned::ElementName;
    /// use zgui_testkit_view::Window;
    /// use zgui_view::{Dom, ListenerOptions};
    /// use zgui_vocab::{EventKind, Key, Modifiers, NamedKey};
    ///
    /// let window = Window::open();
    /// let grid = window.dom.create_element(ElementName::new("box"));
    /// window.dom.insert(window.root, grid, None);
    ///
    /// let held = Rc::new(Cell::new(Modifiers::NONE));
    /// let seen = Rc::clone(&held);
    /// window.dom.add_listener(
    ///     grid,
    ///     EventKind::KeyDown,
    ///     ListenerOptions::DEFAULT,
    ///     Rc::new(move |cx| seen.set(cx.modifiers)),
    /// );
    ///
    /// window
    ///     .dispatcher()
    ///     .with_modifiers(Modifiers::SHIFT)
    ///     .key(grid, Key::Named(NamedKey::PageUp));
    /// assert!(held.get().contains(Modifiers::SHIFT));
    /// ```
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    /// Clicks at a point.
    pub fn click_at(&self, point: Point<DevicePx, Device>) -> Delivered {
        self.pointer_at(point, EventKind::Click)
    }

    /// Sends one pointer event at a point.
    pub fn pointer_at(&self, point: Point<DevicePx, Device>, kind: EventKind) -> Delivered {
        let position =
            zgui_geom::Point::new(zgui_geom::CssPx(point.x.0), zgui_geom::CssPx(point.y.0));
        let payload = Payload::Pointer(zgui_vocab::PointerEvent::mouse(position));
        match crate::input::hit::topmost(self.dom, self.host, self.root, point) {
            Some(target) => self.send_to(target, kind, payload),
            None => Delivered {
                target: None,
                path: Vec::new(),
                ran: Vec::new(),
                default: DefaultAction::Allowed,
                commands: Vec::new(),
            },
        }
    }

    /// Sends one key event to `node`, and carries out what the framework does about it.
    ///
    /// A window does two things with a key: it delivers it, and then — if every handler let the
    /// framework's own behaviour stand — it acts on it. For <kbd>Enter</kbd> and <kbd>Space</kbd>
    /// that behaviour is *activate whatever has focus*, which is dispatched as a click, and it is
    /// the whole reason an ordinary button needs no key handling at all.
    ///
    /// So it happens here too. A test that skipped it would be asserting that a component handles
    /// its own keys, which is the opposite of what a component in this framework should do, and
    /// every button written correctly would fail it.
    ///
    /// ```
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    /// use zgui_interned::ElementName;
    /// use zgui_testkit_view::Window;
    /// use zgui_view::{Dom, ListenerOptions};
    /// use zgui_vocab::{EventKind, Key, NamedKey};
    ///
    /// let window = Window::open();
    /// let button = window.dom.create_element(ElementName::new("control"));
    /// window.dom.insert(window.root, button, None);
    ///
    /// let clicks = Rc::new(Cell::new(0));
    /// let count = Rc::clone(&clicks);
    /// window.dom.add_listener(
    ///     button,
    ///     EventKind::Click,
    ///     ListenerOptions::DEFAULT,
    ///     Rc::new(move |_| count.set(count.get() + 1)),
    /// );
    ///
    /// window.dispatcher().key(button, Key::Named(NamedKey::Space));
    /// assert_eq!(clicks.get(), 1, "the space bar activates what has focus");
    /// ```
    pub fn key(&self, node: NodeId, key: zgui_vocab::Key) -> Delivered {
        let mut event = zgui_vocab::KeyEvent::named(
            zgui_vocab::NamedKey::Enter,
            zgui_vocab::PhysicalKey::Unidentified(0),
        );
        event.key = key.clone();
        event.key_without_modifiers = key.clone();
        let mut delivered = self.send_to(node, EventKind::KeyDown, Payload::Key(event.clone()));
        if delivered.default != DefaultAction::Allowed {
            return delivered;
        }
        // Editing first, and instead of activation when it takes the key: the space bar in a field
        // types a space, and a window that also activated the field with it would do both.
        if self.edit(node, &event, &mut delivered) {
            return delivered;
        }
        if activates(&key) {
            let activation = self.send_to(node, EventKind::Click, click_payload());
            delivered.ran.extend(activation.ran);
            delivered.commands.extend(activation.commands);
        }
        delivered
    }

    /// Types one key into `node` when it is editable, announcing whatever that changed.
    ///
    /// The other half of what a window does with a key, and the half a component library lives on:
    /// the model writes the text into the tree and the new value is dispatched as an input event,
    /// which is the only way anything above the tree ever learns what a field now holds.
    ///
    /// Answers whether the model took the key.
    fn edit(&self, node: NodeId, event: &zgui_vocab::KeyEvent, delivered: &mut Delivered) -> bool {
        let edited = self.dom.type_key(node, event, self.modifiers);
        if let Some(value) = edited.value {
            self.host.write_selection(node, edited.selection.clone());
            let payload = Payload::Value(zgui_vocab::ValueEvent {
                value: zgui_vocab::SharedString::from(value),
                selection: edited.selection,
                kind: zgui_vocab::ValueChange::Input,
            });
            let announced = self.send_to(node, EventKind::Input, payload);
            delivered.ran.extend(announced.ran);
            delivered.commands.extend(announced.commands);
        }
        edited.handled
    }

    /// Sends text to `node`, as the keyboard produces it.
    pub fn type_text(&self, node: NodeId, text: &str) -> Delivered {
        self.send_to(
            node,
            EventKind::Text,
            Payload::Text(zgui_vocab::TextEvent::new(text)),
        )
    }

    /// Sends one event straight at a node, without a hit test.
    ///
    /// What an accessibility action and a keyboard activation both are: an event aimed at an
    /// element rather than at a place.
    ///
    /// Every event a handler asked to be dispatched is dispatched too, after this one has
    /// finished, exactly as a window carries out the commands one dispatch produced — so
    /// [`EventCx::synthesize`](zgui_view::EventCx::synthesize) is a real dispatch here and not a
    /// line in a list. The handlers those runs reach are appended to [`Delivered::ran`], and the
    /// commands they issue to [`Delivered::commands`].
    pub fn send_to(&self, target: NodeId, kind: EventKind, payload: Payload) -> Delivered {
        let mut delivered = self.dispatch(target, kind, payload);
        self.carry_out_synthesized(&mut delivered);
        delivered
    }

    /// Dispatches every event the handlers of `delivered` asked for, and everything those ask for.
    ///
    /// Bounded, for the same reason the runtime's own loop is: a handler that synthesises the
    /// event it is handling would otherwise hold the test for ever.
    fn carry_out_synthesized(&self, delivered: &mut Delivered) {
        let mut carried = 0;
        for _ in 0..8u8 {
            let pending: Vec<(NodeId, EventKind)> = delivered.commands[carried..]
                .iter()
                .filter_map(|command| match command {
                    Command::Synthesize(node, kind) => Some((*node, *kind)),
                    _ => None,
                })
                .collect();
            carried = delivered.commands.len();
            if pending.is_empty() {
                return;
            }
            for (node, kind) in pending {
                let more = self.dispatch(node, kind, click_payload());
                delivered.ran.extend(more.ran);
                delivered.commands.extend(more.commands);
            }
        }
    }

    /// Delivers one event and nothing else, which is what everything above is built out of.
    fn dispatch(&self, target: NodeId, kind: EventKind, payload: Payload) -> Delivered {
        let path = self.path_to(target);
        let steps = self.resolve(&path, kind);

        let control = EventControl::new();
        let mut commands = Commands::with_transcript(self.transcript.clone());
        let mut ran = Vec::new();
        let mut current = None;

        for step in &steps {
            let node = step.node;
            // Between elements: a handler that asked the event to stop is honoured here, after the
            // element it was registered on has run its own handlers.
            if current != Some(node) {
                if !control.propagation().continues_to_next_element() {
                    break;
                }
                current = Some(node);
            } else if !control.propagation().continues_to_next_listener() {
                continue;
            }
            let Some(handler) = self.handlers.handler_of(node, step.listener) else {
                continue;
            };
            self.transcript.push(Op::Handler {
                node,
                event: kind.name().to_owned(),
                phase: phase_name(step.phase),
            });
            ran.push((node, step.phase));
            let mut cx = EventCx::<'_, zgui_view::AnyEvent>::new(
                kind,
                target,
                node,
                step.phase,
                self.modifiers,
                Timestamp::ORIGIN,
                &payload,
                &control,
                &mut commands,
            )
            .with_bounds({
                use zgui_view::ViewHost;
                self.host.border_box(node)
            });
            handler(&mut cx);
        }

        Delivered {
            target: Some(target),
            path,
            ran,
            default: control.default_action(),
            commands: commands.issued().to_vec(),
        }
    }

    /// The path from the dispatcher's root down to `target`, root first.
    fn path_to(&self, target: NodeId) -> Vec<NodeId> {
        use zgui_view::Dom;
        let mut path = vec![target];
        let mut node = target;
        while let Some(parent) = self.dom.parent(node) {
            path.push(parent);
            if parent == self.root {
                break;
            }
            node = parent;
        }
        path.reverse();
        path
    }

    /// Which registrations run, in order, each named by the identity it will be found again by.
    ///
    /// Resolved to identities here rather than looked up by position at call time, because a
    /// handler may register or remove a listener while the event is still travelling — dismissing a
    /// layer from inside a press on it is the ordinary case — and every position after the change
    /// has moved by the time the next step runs. That is the same reason the real resolver hands
    /// back identities, so the harness and the runtime cannot disagree about *which* handler an
    /// order names.
    fn resolve(&self, path: &[NodeId], kind: EventKind) -> Vec<Resolved> {
        let listeners = Registered {
            path,
            handlers: &self.handlers,
        };
        let mut steps = Vec::new();
        zgui_vocab::route(kind, &listeners, &mut steps);
        steps
            .into_iter()
            .filter_map(|step: RouteStep| {
                let node = *path.get(step.element)?;
                Some(Resolved {
                    node,
                    listener: self.listener_at(node, kind, step.registration)?,
                    phase: step.phase,
                })
            })
            .collect()
    }

    /// The identity of one element's `position`-th registration for `kind`.
    pub fn listener_at(
        &self,
        node: NodeId,
        kind: EventKind,
        position: usize,
    ) -> Option<ListenerId> {
        self.handlers
            .of(node, kind)
            .get(position)
            .map(|registration| registration.id)
    }
}

/// Whether a key is one the framework activates the focused element with.
///
/// The pair the specification names, and the same pair the input router acts on: everything else
/// is a key a component either handles or ignores.
fn activates(key: &zgui_vocab::Key) -> bool {
    matches!(
        key,
        zgui_vocab::Key::Named(zgui_vocab::NamedKey::Enter | zgui_vocab::NamedKey::Space)
    )
}

/// The payload a synthesised activation carries.
///
/// A pointer event at the origin, because that is what a window sends when there is no pointer on
/// the surface, and because a handler reading a position off a keyboard activation is reading
/// something that was never there.
fn click_payload() -> Payload {
    Payload::Pointer(zgui_vocab::PointerEvent::mouse(zgui_geom::Point::new(
        zgui_geom::CssPx(0.0),
        zgui_geom::CssPx(0.0),
    )))
}

/// One step of a resolved order: whose listener runs, which one, and in which leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Resolved {
    /// The element whose listener runs.
    node: NodeId,
    /// Which registration on it.
    listener: ListenerId,
    /// Which leg of the delivery this is.
    phase: Phase,
}

/// The registrations along one path, as the ordering rule reads them.
struct Registered<'a> {
    /// The path, root first.
    path: &'a [NodeId],
    /// Where the handlers are.
    handlers: &'a Handlers,
}

impl Listeners for Registered<'_> {
    fn depth(&self) -> usize {
        self.path.len()
    }

    fn each(&self, element: usize, kind: EventKind, each: &mut dyn FnMut(usize, ListenerOptions)) {
        let Some(node) = self.path.get(element) else {
            return;
        };
        for (position, registration) in self.handlers.of(*node, kind).iter().enumerate() {
            each(position, registration.options);
        }
    }
}

/// A leg's name, for a transcript.
fn phase_name(phase: Phase) -> String {
    match phase {
        Phase::Capture => "capture",
        Phase::Target => "target",
        Phase::Bubble => "bubble",
        _ => "other",
    }
    .to_owned()
}
