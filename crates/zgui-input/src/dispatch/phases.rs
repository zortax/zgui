//! Turning a path and a set of registrations into the order handlers run in.

use zgui_dom::side::listeners::ListenerId;
use zgui_dom::{DocumentStore, NodeKey};
use zgui_vocab::{EventKind, ListenerOptions, Listeners, Phase};

use crate::hit::HitChain;

/// One listener to run, by name.
///
/// No handler, on purpose: what the handler *is* belongs to whoever registered it, and this crate
/// could not name one if it wanted to. The identity is enough to find it again, and identities are
/// never reused, so a registration removed while an event is in flight resolves to nothing rather
/// than to whichever registration was made afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    /// The element whose listener runs.
    pub node: NodeKey,
    /// Which registration on it.
    pub listener: ListenerId,
    /// Which leg of the delivery this is: the way down, the target, or the way up.
    pub phase: Phase,
}

/// A resolved order, kept across events so that dispatching allocates nothing after the first.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// The steps, in the order they run.
    steps: Vec<Step>,
    /// The scratch the ordering rule fills, reused for the same reason.
    scratch: Vec<zgui_vocab::RouteStep>,
}

impl Plan {
    /// An empty plan.
    pub fn new() -> Self {
        Self::default()
    }

    /// The steps, in the order they run.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Whether nothing listens for this event anywhere on the path.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The elements the plan runs a listener on, in order and without repeats.
    ///
    /// What a caller honouring `stop_propagation` compares against: propagation stops between
    /// elements, so knowing where one element's steps end and the next begins is the whole of it.
    pub fn elements(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.steps
            .iter()
            .map(|step| step.node)
            .scan(None, |last, node| {
                let repeat = *last == Some(node);
                *last = Some(node);
                Some((repeat, node))
            })
            .filter_map(|(repeat, node)| (!repeat).then_some(node))
    }
}

/// Resolves which registrations for `kind` along `chain` run, and in which order.
///
/// The plan is rebuilt from scratch, so one plan can be reused for every event of a run.
///
/// An element on the path that has been removed from the document contributes nothing and does not
/// end the walk: a chain is a snapshot, and a handler that removed an ancestor while the event was
/// travelling must not stop the event reaching the elements above it.
pub fn resolve(store: &DocumentStore, chain: &HitChain, kind: EventKind, plan: &mut Plan) {
    let path = Registrations { store, chain };
    zgui_vocab::route(kind, &path, &mut plan.scratch);
    plan.steps.clear();
    for step in &plan.scratch {
        let Some(node) = chain.path().get(step.element).copied() else {
            continue;
        };
        let Some(listener) = nth_listener(store, node, kind, step.registration) else {
            continue;
        };
        plan.steps.push(Step {
            node,
            listener,
            phase: step.phase,
        });
    }
}

/// Appends the registrations `node` holds for `kind`, delivered as though it were the target.
///
/// What a window-level shortcut gets. A key nothing has focus for travels a path one element long
/// — the document's root — so a registration anywhere below it is on no path at all and is
/// reached by being named rather than by being walked past. Every registration on the node runs,
/// however it was written: a delivery one element long has no way down and no way up to tell
/// apart, which is the same rule the target leg already follows.
///
/// Additive. Whatever the plan already holds keeps its order and runs first, so a shortcut never
/// displaces a listener the path itself found.
pub fn append(store: &DocumentStore, node: NodeKey, kind: EventKind, plan: &mut Plan) {
    for listener in listeners_for(store, node, kind) {
        plan.steps.push(Step {
            node,
            listener: listener.id,
            phase: Phase::Target,
        });
    }
}

/// A path's registrations, read straight out of the document.
struct Registrations<'a> {
    /// The document holding them.
    store: &'a DocumentStore,
    /// The path.
    chain: &'a HitChain,
}

impl Listeners for Registrations<'_> {
    fn depth(&self) -> usize {
        self.chain.depth()
    }

    fn each(&self, element: usize, kind: EventKind, each: &mut dyn FnMut(usize, ListenerOptions)) {
        let Some(node) = self.chain.path().get(element) else {
            return;
        };
        for (position, listener) in listeners_for(self.store, *node, kind).enumerate() {
            each(position, listener.options);
        }
    }
}

/// One element's registrations for one event, in registration order.
fn listeners_for(
    store: &DocumentStore,
    node: NodeKey,
    kind: EventKind,
) -> impl Iterator<Item = &zgui_dom::side::listeners::Listener> {
    store
        .columns()
        .listeners
        .get(node)
        .into_iter()
        .flat_map(zgui_dom::side::listeners::ListenerSet::iter)
        .filter(move |listener| listener.kind == kind)
}

/// The identity of one element's `position`-th registration for `kind`.
fn nth_listener(
    store: &DocumentStore,
    node: NodeKey,
    kind: EventKind,
    position: usize,
) -> Option<ListenerId> {
    listeners_for(store, node, kind)
        .nth(position)
        .map(|listener| listener.id)
}

#[cfg(test)]
mod tests {
    use zgui_dom::side::listeners::ListenerId;
    use zgui_dom::{Document, EverythingMatters, NodeIndex};
    use zgui_interned::ElementName;
    use zgui_vocab::{EventKind, ListenerOptions, Phase};

    use super::{Plan, Step, resolve};
    use crate::hit::HitChain;

    /// `root > toolbar > button`, with the registrations a test asks for.
    struct Fixture {
        document: Document,
        nodes: [NodeIndex; 3],
    }

    impl Fixture {
        fn new() -> Self {
            let document = Document::new();
            let nodes = document
                .edit(&EverythingMatters, |edit| {
                    let root = edit.create_element(ElementName::new("root"));
                    edit.insert_before(document.document_index(), root, None);
                    let toolbar = edit.create_element(ElementName::new("row"));
                    edit.insert_before(root, toolbar, None);
                    let button = edit.create_element(ElementName::new("control"));
                    edit.insert_before(toolbar, button, None);
                    [root, toolbar, button]
                })
                .expect("not poisoned");
            Self { document, nodes }
        }

        fn listen(&mut self, at: usize, kind: EventKind, options: ListenerOptions) -> ListenerId {
            let node = self.nodes[at];
            self.document
                .edit(&EverythingMatters, |edit| {
                    edit.add_listener(node, kind, options)
                })
                .expect("not poisoned")
        }

        fn plan(&self, kind: EventKind) -> Plan {
            let target = self.document.store().key_of(self.nodes[2]);
            let chain = HitChain::to_root(self.document.store(), target);
            let mut plan = Plan::new();
            resolve(self.document.store(), &chain, kind, &mut plan);
            plan
        }
    }

    #[test]
    fn the_order_is_down_to_the_target_and_back_up() {
        let mut fixture = Fixture::new();
        let down = fixture.listen(0, EventKind::Click, ListenerOptions::CAPTURE);
        let up = fixture.listen(0, EventKind::Click, ListenerOptions::DEFAULT);
        let target = fixture.listen(2, EventKind::Click, ListenerOptions::DEFAULT);

        let plan = fixture.plan(EventKind::Click);
        let root = fixture.document.store().key_of(fixture.nodes[0]);
        let button = fixture.document.store().key_of(fixture.nodes[2]);
        assert_eq!(
            plan.steps(),
            &[
                Step {
                    node: root,
                    listener: down,
                    phase: Phase::Capture
                },
                Step {
                    node: button,
                    listener: target,
                    phase: Phase::Target
                },
                Step {
                    node: root,
                    listener: up,
                    phase: Phase::Bubble
                },
            ]
        );
    }

    #[test]
    fn a_registration_for_another_event_is_not_on_the_plan() {
        let mut fixture = Fixture::new();
        fixture.listen(1, EventKind::KeyDown, ListenerOptions::DEFAULT);
        let click = fixture.listen(1, EventKind::Click, ListenerOptions::DEFAULT);

        let plan = fixture.plan(EventKind::Click);
        assert_eq!(plan.steps().len(), 1);
        assert_eq!(plan.steps()[0].listener, click);
    }

    #[test]
    fn two_registrations_on_one_element_keep_the_order_they_were_made_in() {
        let mut fixture = Fixture::new();
        let first = fixture.listen(2, EventKind::Click, ListenerOptions::DEFAULT);
        let second = fixture.listen(2, EventKind::Click, ListenerOptions::CAPTURE);

        let plan = fixture.plan(EventKind::Click);
        assert_eq!(
            plan.steps()
                .iter()
                .map(|step| step.listener)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
    }

    #[test]
    fn a_removed_registration_leaves_no_step_behind() {
        let mut fixture = Fixture::new();
        let removed = fixture.listen(1, EventKind::Click, ListenerOptions::DEFAULT);
        let kept = fixture.listen(1, EventKind::Click, ListenerOptions::DEFAULT);
        let toolbar = fixture.nodes[1];
        fixture
            .document
            .edit(&EverythingMatters, |edit| {
                assert!(edit.remove_listener(toolbar, removed));
            })
            .expect("not poisoned");

        let plan = fixture.plan(EventKind::Click);
        assert_eq!(plan.steps().len(), 1);
        assert_eq!(plan.steps()[0].listener, kept);
    }

    #[test]
    fn reusing_a_plan_leaves_nothing_of_the_previous_event_on_it() {
        let mut fixture = Fixture::new();
        fixture.listen(2, EventKind::Click, ListenerOptions::DEFAULT);
        let mut plan = fixture.plan(EventKind::Click);
        assert_eq!(plan.steps().len(), 1);

        let target = fixture.document.store().key_of(fixture.nodes[2]);
        let chain = HitChain::to_root(fixture.document.store(), target);
        resolve(
            fixture.document.store(),
            &chain,
            EventKind::KeyDown,
            &mut plan,
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn an_appended_registration_runs_and_nothing_between_it_and_the_root_does() {
        // What a window shortcut gets. The path an unfocused key travels is one element long, so
        // the registration is named rather than walked past — and naming it must not put the
        // elements above it on the route, or every key handler in the page would hear a key
        // aimed at nothing.
        let mut fixture = Fixture::new();
        let on_the_way = fixture.listen(1, EventKind::KeyDown, ListenerOptions::CAPTURE);
        let registered = fixture.listen(2, EventKind::KeyDown, ListenerOptions::CAPTURE);
        let also_registered = fixture.listen(2, EventKind::KeyDown, ListenerOptions::DEFAULT);

        let root = fixture.document.store().key_of(fixture.nodes[0]);
        let deep = fixture.document.store().key_of(fixture.nodes[2]);
        let chain = HitChain::from_path([root]);
        let mut plan = Plan::new();
        resolve(
            fixture.document.store(),
            &chain,
            EventKind::KeyDown,
            &mut plan,
        );
        assert!(plan.is_empty(), "the root itself listens for nothing");

        super::append(
            fixture.document.store(),
            deep,
            EventKind::KeyDown,
            &mut plan,
        );
        assert_eq!(
            plan.steps(),
            &[
                Step {
                    node: deep,
                    listener: registered,
                    phase: Phase::Target
                },
                Step {
                    node: deep,
                    listener: also_registered,
                    phase: Phase::Target
                },
            ],
            "every registration on the named element runs, however it was written, and the \
             listener between it and the root does not"
        );
        assert!(!plan.steps().iter().any(|step| step.listener == on_the_way));
    }

    #[test]
    fn the_elements_a_plan_touches_are_reported_in_order_and_without_repeats() {
        let mut fixture = Fixture::new();
        fixture.listen(2, EventKind::Click, ListenerOptions::DEFAULT);
        fixture.listen(2, EventKind::Click, ListenerOptions::DEFAULT);
        fixture.listen(0, EventKind::Click, ListenerOptions::DEFAULT);

        let plan = fixture.plan(EventKind::Click);
        let button = fixture.document.store().key_of(fixture.nodes[2]);
        let root = fixture.document.store().key_of(fixture.nodes[0]);
        assert_eq!(plan.elements().collect::<Vec<_>>(), vec![button, root]);
    }
}
