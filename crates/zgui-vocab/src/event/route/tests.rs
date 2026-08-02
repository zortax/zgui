//! The order, asserted leg by leg.

use crate::event::kind::EventKind;
use crate::event::listener::{ListenerOptions, Phase};
use crate::event::route::{Listeners, Path, RouteStep, route};

/// Root, toolbar, button — with a registration of each kind on every one of them.
fn everything_everywhere() -> [&'static [ListenerOptions]; 3] {
    const BOTH: &[ListenerOptions] = &[ListenerOptions::CAPTURE, ListenerOptions::DEFAULT];
    [BOTH, BOTH, BOTH]
}

/// Where each step landed, as `(element, phase)`.
fn places(steps: &[RouteStep]) -> Vec<(usize, Phase)> {
    steps
        .iter()
        .map(|step| (step.element, step.phase))
        .collect()
}

#[test]
fn the_way_down_reaches_the_target_and_the_way_up_leaves_it() {
    let elements = everything_everywhere();
    let path = Path::new(&elements);
    let mut steps = Vec::new();
    route(EventKind::Click, &path, &mut steps);

    assert_eq!(
        places(&steps),
        vec![
            (0, Phase::Capture),
            (1, Phase::Capture),
            (2, Phase::Target),
            (2, Phase::Target),
            (1, Phase::Bubble),
            (0, Phase::Bubble),
        ],
        "down from the root, both of the target's, then back up"
    );
}

#[test]
fn every_registration_on_the_target_runs_however_it_was_registered() {
    let elements = everything_everywhere();
    let path = Path::new(&elements);
    let mut steps = Vec::new();
    route(EventKind::Click, &path, &mut steps);

    let at_target: Vec<usize> = steps
        .iter()
        .filter(|step| step.element == 2)
        .map(|step| step.registration)
        .collect();
    assert_eq!(
        at_target,
        vec![0, 1],
        "in the order they were registered, and neither of them twice"
    );
}

#[test]
fn an_event_that_does_not_bubble_still_travels_down() {
    let elements = everything_everywhere();
    let path = Path::new(&elements);
    let mut steps = Vec::new();
    route(EventKind::Scroll, &path, &mut steps);

    assert!(
        !EventKind::Scroll.bubbles(),
        "the case would be vacuous against an event that bubbles"
    );
    assert_eq!(
        places(&steps),
        vec![
            (0, Phase::Capture),
            (1, Phase::Capture),
            (2, Phase::Target),
            (2, Phase::Target),
        ],
        "an ancestor watching on the way down sees it; one watching on the way up does not"
    );
}

#[test]
fn a_target_with_no_ancestors_gets_its_own_listeners_and_nothing_else() {
    let elements: [&[ListenerOptions]; 1] = [&[ListenerOptions::DEFAULT]];
    let path = Path::new(&elements);
    let mut steps = Vec::new();
    route(EventKind::Click, &path, &mut steps);
    assert_eq!(places(&steps), vec![(0, Phase::Target)]);
}

#[test]
fn an_empty_path_resolves_to_nothing_and_clears_what_was_there() {
    let elements: [&[ListenerOptions]; 0] = [];
    let path = Path::new(&elements);
    let mut steps = vec![RouteStep {
        element: 9,
        registration: 9,
        phase: Phase::Target,
    }];
    route(EventKind::Click, &path, &mut steps);
    assert!(
        steps.is_empty(),
        "a stale answer must not survive a resolve"
    );
}

#[test]
fn an_element_that_listens_for_nothing_contributes_nothing() {
    let elements: [&[ListenerOptions]; 3] = [
        &[ListenerOptions::CAPTURE],
        &[],
        &[ListenerOptions::DEFAULT],
    ];
    let path = Path::new(&elements);
    let mut steps = Vec::new();
    route(EventKind::Click, &path, &mut steps);
    assert_eq!(
        places(&steps),
        vec![(0, Phase::Capture), (2, Phase::Target)],
        "the toolbar in the middle is silent in both directions"
    );
}

#[test]
fn the_path_helper_offers_every_registration_it_was_given() {
    let elements: [&[ListenerOptions]; 1] = [&[ListenerOptions::ONCE, ListenerOptions::PASSIVE]];
    let path = Path::new(&elements);
    let mut seen = Vec::new();
    path.each(0, EventKind::Click, &mut |position, options| {
        seen.push((position, options));
    });
    assert_eq!(
        seen,
        vec![(0, ListenerOptions::ONCE), (1, ListenerOptions::PASSIVE)]
    );
    // And an element past the end is not a panic: a caller's depth and its rows are its own to
    // keep in step, and the answer for a row that is not there is no registrations.
    let mut past = 0;
    path.each(7, EventKind::Click, &mut |_, _| past += 1);
    assert_eq!(past, 0);
}
