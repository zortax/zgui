//! Capture, target and bubble, with the two things a handler can say honoured at each leg.
//!
//! This crate resolves the order and never runs a handler, so the test supplies the half that
//! runs: a walk over the resolved list that stops when a handler asks it to and skips the
//! framework's own behaviour when a handler takes responsibility for it. That is exactly the shape
//! of the caller this list is built for, and writing it here is what makes the order assertable
//! against something that behaves like the real consumer rather than against a list of names.

mod support;

use std::cell::RefCell;
use std::collections::HashMap;

use support::{Element, Fixture, Session};
use zgui_dom::side::listeners::ListenerId;
use zgui_input::{FrameworkDefault, Step};
use zgui_vocab::{DefaultAction, EventKind, ListenerOptions, Phase, PointerAction, Propagation};

/// What a handler said about the event it saw.
#[derive(Clone, Copy, Debug, Default)]
struct Says {
    /// How far the event should keep travelling.
    propagation: Propagation,
    /// Whether the framework's own behaviour should still happen.
    default: DefaultAction,
}

impl Says {
    /// A handler that says nothing.
    const NOTHING: Self = Self {
        propagation: Propagation::Continue,
        default: DefaultAction::Allowed,
    };

    /// A handler that stops the event after this element's other handlers.
    const STOPS: Self = Self {
        propagation: Propagation::Stop,
        default: DefaultAction::Allowed,
    };

    /// A handler that stops the event at once.
    const STOPS_AT_ONCE: Self = Self {
        propagation: Propagation::StopImmediate,
        default: DefaultAction::Allowed,
    };

    /// A handler that takes responsibility for the framework's behaviour.
    const PREVENTS: Self = Self {
        propagation: Propagation::Continue,
        default: DefaultAction::Prevented,
    };
}

/// The half of dispatch this crate deliberately does not own: running the handlers.
///
/// It walks the resolved list in order, calls whatever was registered under each identity, and
/// honours what each one said between steps — which is the contract the list is resolved under.
#[derive(Default)]
struct Runner {
    /// What each registration says when it runs.
    handlers: HashMap<ListenerId, Says>,
    /// Which handlers ran, in order, and in which leg.
    ran: RefCell<Vec<(&'static str, Phase)>>,
    /// The name of each registration, for the transcript.
    names: HashMap<ListenerId, &'static str>,
}

impl Runner {
    /// Registers a handler under `id`.
    fn on(&mut self, id: ListenerId, name: &'static str, says: Says) {
        self.handlers.insert(id, says);
        self.names.insert(id, name);
    }

    /// Walks `steps`, and reports whether the framework's own behaviour survived.
    fn run(&self, steps: &[Step]) -> DefaultAction {
        self.ran.borrow_mut().clear();
        let mut default = DefaultAction::Allowed;
        let mut propagation = Propagation::Continue;
        let mut current = None;
        for step in steps {
            // Between elements: a handler that asked the event to stop is honoured here, after the
            // element it was registered on has finished its own handlers.
            if current != Some(step.node) {
                if !propagation.continues_to_next_element() {
                    break;
                }
                current = Some(step.node);
            } else if !propagation.continues_to_next_listener() {
                continue;
            }
            let says = self.handlers[&step.listener];
            self.ran
                .borrow_mut()
                .push((self.names[&step.listener], step.phase));
            propagation = propagation.strongest(says.propagation);
            default = default.strongest(says.default);
        }
        default
    }

    /// The transcript of what ran.
    fn transcript(&self) -> Vec<(&'static str, Phase)> {
        self.ran.borrow().clone()
    }
}

/// `root > toolbar > control`, laid out so the control can be pressed.
fn toolbar() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("row").children(vec![Element::new("control")]),
        ]),
        "root, row { display: block; width: 300px }
         control { display: block; width: 300px; height: 30px }",
    )
}

/// Registers one listener and returns its identity.
fn listen(
    fixture: &mut Fixture,
    name: &str,
    kind: EventKind,
    options: ListenerOptions,
) -> ListenerId {
    let node = fixture.find(name);
    fixture
        .document
        .edit(&zgui_dom::EverythingMatters, |edit| {
            edit.add_listener(node, kind, options)
        })
        .expect("not poisoned")
}

#[test]
fn handlers_run_down_to_the_target_and_back_up() {
    let mut fixture = toolbar();
    let mut runner = Runner::default();
    for (element, name, options) in [
        ("root", "root-capture", ListenerOptions::CAPTURE),
        ("root", "root-bubble", ListenerOptions::DEFAULT),
        ("row", "row-capture", ListenerOptions::CAPTURE),
        ("row", "row-bubble", ListenerOptions::DEFAULT),
        ("control", "control", ListenerOptions::DEFAULT),
    ] {
        let id = listen(&mut fixture, element, EventKind::PointerDown, options);
        runner.on(id, name, Says::NOTHING);
    }

    let mut session = Session::new(fixture);
    let point = session.fixture.centre_of("control");
    let steps = session.press(point);
    runner.run(&steps);

    assert_eq!(
        runner.transcript(),
        vec![
            ("root-capture", Phase::Capture),
            ("row-capture", Phase::Capture),
            ("control", Phase::Target),
            ("row-bubble", Phase::Bubble),
            ("root-bubble", Phase::Bubble),
        ]
    );
}

#[test]
fn stopping_on_the_way_down_leaves_the_target_and_the_way_up_unrun() {
    let mut fixture = toolbar();
    let mut runner = Runner::default();
    let root = listen(
        &mut fixture,
        "root",
        EventKind::PointerDown,
        ListenerOptions::CAPTURE,
    );
    runner.on(root, "root-capture", Says::STOPS);
    for (element, name, options) in [
        ("row", "row-capture", ListenerOptions::CAPTURE),
        ("control", "control", ListenerOptions::DEFAULT),
        ("root", "root-bubble", ListenerOptions::DEFAULT),
    ] {
        let id = listen(&mut fixture, element, EventKind::PointerDown, options);
        runner.on(id, name, Says::NOTHING);
    }

    let mut session = Session::new(fixture);
    let point = session.fixture.centre_of("control");
    let steps = session.press(point);
    runner.run(&steps);

    assert_eq!(
        runner.transcript(),
        vec![("root-capture", Phase::Capture)],
        "an overlay that stops a press on the way down is what dismissal is written with"
    );
}

#[test]
fn stopping_lets_the_current_elements_other_handlers_run_and_stopping_at_once_does_not() {
    for (says, expected) in [
        (
            Says::STOPS,
            vec![("first", Phase::Target), ("second", Phase::Target)],
        ),
        (Says::STOPS_AT_ONCE, vec![("first", Phase::Target)]),
    ] {
        let mut fixture = toolbar();
        let mut runner = Runner::default();
        let first = listen(
            &mut fixture,
            "control",
            EventKind::PointerDown,
            ListenerOptions::DEFAULT,
        );
        runner.on(first, "first", says);
        let second = listen(
            &mut fixture,
            "control",
            EventKind::PointerDown,
            ListenerOptions::DEFAULT,
        );
        runner.on(second, "second", Says::NOTHING);
        let up = listen(
            &mut fixture,
            "root",
            EventKind::PointerDown,
            ListenerOptions::DEFAULT,
        );
        runner.on(up, "root-bubble", Says::NOTHING);

        let mut session = Session::new(fixture);
        let point = session.fixture.centre_of("control");
        let steps = session.press(point);
        runner.run(&steps);

        assert_eq!(runner.transcript(), expected);
    }
}

#[test]
fn preventing_the_default_leaves_the_framework_with_nothing_to_do() {
    let mut fixture = toolbar();
    let mut runner = Runner::default();
    let id = listen(
        &mut fixture,
        "control",
        EventKind::PointerDown,
        ListenerOptions::DEFAULT,
    );
    runner.on(id, "control", Says::PREVENTS);

    let mut session = Session::new(fixture);
    let point = session.fixture.centre_of("control");
    let (steps, default) = session.press_with_default(point);

    // The framework wanted to focus the control, which is what a press means when nobody objects.
    assert_eq!(
        default,
        Some(FrameworkDefault::Focus {
            node: Some(session.fixture.key("control")),
            source: zgui_input::FocusSource::Pointer,
        })
    );
    assert_eq!(
        runner.run(&steps),
        DefaultAction::Prevented,
        "and the handler took responsibility, so the caller does not carry it out"
    );
}

#[test]
fn a_press_and_release_over_one_control_activates_it_and_one_that_slides_off_does_not() {
    let mut session = Session::new(toolbar());
    let control = session.fixture.key("control");
    let on_control = session.fixture.centre_of("control");
    let elsewhere =
        zgui_geom::Point::new(on_control.x, zgui_geom::DevicePx(on_control.y.0 + 200.0));

    session.pointer_at(on_control, PointerAction::Pressed);
    assert_eq!(
        session.default_at(on_control, PointerAction::Released),
        Some(FrameworkDefault::Activate(control))
    );

    session.pointer_at(on_control, PointerAction::Pressed);
    assert_eq!(
        session.default_at(elsewhere, PointerAction::Released),
        None,
        "letting go somewhere else is how someone changes their mind mid-press"
    );
}
