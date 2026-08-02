//! Checkboxes, radios, switches, toggles and sliders.
//!
//! A switch says what it is by where its thumb is, and the thumb is a box, so this can read the
//! state off the document. A checkbox and a toggle say it in colour only, so their claims are made
//! from captures instead: the rectangles are marked here and the pictures are compared afterwards.

use zgui::vocab::{Modifiers, NamedKey};

use crate::script::find;
use crate::stage::Stage;

/// Drives the choices.
pub(crate) fn run(stage: &mut Stage<'_>) {
    checkboxes(stage);
    radio_and_switch(stage);
    toggles(stage);
    slider(stage);
}

/// The checkboxes, including the one that must not move.
fn checkboxes(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Checkbox") else {
        stage.report.note("Checkbox", "the panel is not laid out");
        return;
    };
    stage.shot("choices-checkbox-before");

    let Some(target) = find::at_in(&census, panel, "I accept the terms").map(|_| {
        // The labelled box sits immediately left of its label, in the same row — so what is
        // stepped left from has to be the *label*, which is the innermost node saying those words.
        // The outermost is the row, and the row begins at the panel's own padding with the box
        // inside it: a step left from there lands in the margin, where a click reaches nothing and
        // reads exactly like a checkbox that does not answer.
        let label = census
            .innermost("I accept the terms")
            .and_then(|node| node.rect);
        label.map_or(
            zgui::geom::Point::new(
                zgui::geom::DevicePx(panel.origin.x.0 + 24.0),
                zgui::geom::DevicePx(panel.origin.y.0 + panel.size.height.0 * 0.8),
            ),
            |rect| {
                zgui::geom::Point::new(
                    zgui::geom::DevicePx(rect.origin.x.0 - 12.0),
                    zgui::geom::DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
                )
            },
        )
    }) else {
        stage
            .report
            .note("Checkbox", "no labelled checkbox to click");
        return;
    };

    stage.click(target);
    stage.shot("choices-checkbox-clicked");
    stage.report.check(
        "Checkbox",
        "clicking the box focuses it",
        !stage.focused_text().is_empty() || stage.focused().is_some(),
        &format!("focus is on {:?}", stage.focused_text()),
    );

    // The space bar is the framework activating what has focus, not a handler of the checkbox's,
    // so it is worth pressing separately from the click.
    stage.key(NamedKey::Space);
    stage.shot("choices-checkbox-spaced");

    // The label has to be a label: clicking its text has to reach the box it names.
    if let Some(label) = find::at_in(&census, panel, "I accept the terms") {
        stage.click(label);
        stage.shot("choices-checkbox-via-label");
        stage.report.check(
            "Label",
            "clicking a label moves focus to the control it names",
            stage.focused().is_some(),
            &format!("focus is on {:?}", stage.focused_text()),
        );
    }
}

/// The radio group and the switches.
fn radio_and_switch(stage: &mut Stage<'_>) {
    let Some((census, _panel)) = find::open_panel(stage, "Radio and switch") else {
        stage.report.note("RadioGroup", "the panel is not laid out");
        return;
    };
    stage.shot("choices-radio-before");

    // The radio buttons sit immediately left of their names.
    let Some(monthly) = census.control("Monthly").and_then(|node| node.rect) else {
        stage.report.note("RadioGroup", "no Monthly label");
        return;
    };
    let button = zgui::geom::Point::new(
        zgui::geom::DevicePx(monthly.origin.x.0 - 12.0),
        zgui::geom::DevicePx(monthly.origin.y.0 + monthly.size.height.0 / 2.0),
    );
    stage.click(button);
    let focused = stage.focused();
    stage.report.check(
        "RadioGroup",
        "a click focuses a radio",
        focused.is_some(),
        &format!("focus is on {focused:?}"),
    );
    // Which node the click landed on, in words, so that a group whose arrows do nothing and a
    // click that landed beside the radio are not the same report.
    let landed = stage.focused().map(|node| {
        (
            stage.census().node(node).map(|seen| seen.rect),
            stage.handles().host.focusables(node).len(),
        )
    });
    stage.report.note(
        "RadioGroup",
        &format!(
            "the click focused {:?}, whose box is {landed:?}",
            stage.focused_text()
        ),
    );

    // A radio group is walked with the arrows, and focus has to move to a different node. Which
    // arrows depends on which way the group says it is laid out, so both are reported: a group
    // that answers one and not the other is a different finding from one that answers neither.
    stage.key(NamedKey::ArrowRight);
    let moved = stage.focused();
    stage.key(NamedKey::ArrowDown);
    stage.report.note(
        "RadioGroup",
        &format!(
            "from {focused:?} the right arrow gave {moved:?} and a down arrow after it gave {:?}",
            stage.focused()
        ),
    );
    // Either arrow: which one walks a group is decided by which way the group is laid out, and
    // this one is a column. A claim that names one axis is a claim about the gallery's own layout
    // rather than about the component, and it fails on a group that answers the other arrow
    // perfectly well.
    let walked = stage.focused();
    stage.report.check(
        "RadioGroup",
        "an arrow moves to the next radio",
        walked.is_some() && walked != focused,
        &format!(
            "focus went from {focused:?} to {moved:?} on the right arrow and to {walked:?} on the \
             down arrow"
        ),
    );
    stage.shot("choices-radio-arrowed");

    // The disabled radio must not be landed on: walking past it has to skip it.
    stage.key(NamedKey::ArrowRight);
    let wrapped = stage.focused();
    stage.report.check(
        "RadioGroup",
        "walking wraps rather than stopping on the disabled one",
        wrapped.is_some(),
        &format!("focus is now {wrapped:?}"),
    );

    // The switch, which sits immediately left of the words it is labelled with.
    let census = stage.census();
    let Some(send) = census.innermost("Send me email").and_then(|node| node.rect) else {
        stage.report.note("Switch", "no Send me email label");
        return;
    };
    let switch_at = zgui::geom::Point::new(
        zgui::geom::DevicePx(send.origin.x.0 - 20.0),
        zgui::geom::DevicePx(send.origin.y.0 + send.size.height.0 / 2.0),
    );
    let Some((track, thumb)) = switch_under(stage, switch_at) else {
        stage
            .report
            .note("Switch", "no switch under the point left of the label");
        return;
    };
    find::mark(stage, "switch:send", track);
    stage.shot("choices-switch-before");
    stage.click(switch_at);
    // A switch says which way it is by where its thumb is, and the thumb gets there by transform:
    // it is drawn somewhere its layout box never goes. So the box cannot answer this, and asking
    // it is how a switch that works reads as one that does not. What can be answered from here is
    // that the click reached the control and started the movement; the two captures either side
    // are what says the thumb ended up on the other end.
    let moving = stage.animations(thumb);
    stage.shot("choices-switch-clicked");
    stage.report.check(
        "Switch",
        "clicking starts the thumb moving",
        moving > 0,
        &format!("{moving} transitions are running on the thumb after the click"),
    );
}

/// The track and the thumb of the switch under `at`.
fn switch_under(
    stage: &Stage<'_>,
    at: zgui::geom::Point<zgui::geom::DevicePx, zgui::geom::Device>,
) -> Option<(
    zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>,
    zgui::view::NodeId,
)> {
    let census = stage.census();
    let track = census
        .nodes
        .iter()
        .filter(|node| {
            node.text.is_empty()
                && node.rect.is_some_and(|rect| {
                    rect.origin.x.0 <= at.x.0
                        && at.x.0 <= rect.origin.x.0 + rect.size.width.0
                        && rect.origin.y.0 <= at.y.0
                        && at.y.0 <= rect.origin.y.0 + rect.size.height.0
                        && rect.size.width.0 > rect.size.height.0
                })
        })
        // The smallest, not the largest. Every ancestor up to the page holds the point and is
        // wider than it is tall, so the largest such box is the page — and the "thumb" found
        // inside it is whatever the smallest box in the whole window happens to be.
        .min_by(|left, right| left.area().total_cmp(&right.area()))?;
    let track_rect = track.rect?;
    let thumb = census
        .inside(track_rect)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0 && node.id != track.id)
        .min_by(|left, right| left.area().total_cmp(&right.area()))?;
    Some((track_rect, thumb.id))
}

/// The toggles, alone and in a group.
fn toggles(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Toggle") else {
        stage.report.note("Toggle", "the panel is not laid out");
        return;
    };
    stage.shot("choices-toggle-before");
    let Some(italic) = find::at_in(&census, panel, "I") else {
        stage.report.note("Toggle", "no Italic toggle");
        return;
    };
    stage.click(italic);
    stage.shot("choices-toggle-pressed");
    stage.report.check(
        "Toggle",
        "a click focuses the toggle",
        stage.focused_text() == "I",
        &format!("focus is on {:?}", stage.focused_text()),
    );
    stage.key(NamedKey::Space);
    stage.shot("choices-toggle-spaced");
}

/// The slider, which says its number beside itself.
fn slider(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Slider") else {
        stage.report.note("Slider", "the panel is not laid out");
        return;
    };
    let reading = |stage: &Stage<'_>| -> Option<f64> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter_map(|node| node.text.parse::<f64>().ok())
            .next_back()
    };
    let before = reading(stage);
    stage.report.check(
        "Slider",
        "it starts at the value it was given",
        before == Some(40.0),
        &format!("it reads {before:?}"),
    );

    // The track is the widest box in the panel; pressing three quarters along it has to move the
    // value there rather than by one step.
    let Some(track) = census
        .inside(panel)
        .into_iter()
        .filter(|node| {
            // On the page, and flat. Containment here is geometric: the bands floating surfaces
            // are placed against sit at the window's own origin with boxes of their own, so a
            // panel revealed to the top-left of the window *contains* every one of them. Asking
            // whether the candidate belongs to the page settles that whatever the window's size
            // is; a threshold on its width settles it only while the panel happens to be wider
            // than the band's box, and a narrower window puts the press on the masthead — which
            // reads exactly like a slider that does not move.
            census.on_the_page(node)
                && node.text.is_empty()
                && node
                    .rect
                    .is_some_and(|rect| rect.size.width.0 > rect.size.height.0 * 6.0)
        })
        .min_by(|left, right| {
            left.rect
                .map_or(0.0, |rect| rect.origin.y.0)
                .total_cmp(&right.rect.map_or(0.0, |rect| rect.origin.y.0))
        })
        .and_then(|node| node.rect)
    else {
        stage.report.note("Slider", "no track to press on");
        return;
    };
    find::mark(stage, "slider:track", track);
    let three_quarters = zgui::geom::Point::new(
        zgui::geom::DevicePx(track.origin.x.0 + track.size.width.0 * 0.75),
        zgui::geom::DevicePx(track.origin.y.0 + track.size.height.0 / 2.0),
    );
    stage.click(three_quarters);
    let dropped = reading(stage);
    stage.report.check(
        "Slider",
        "a press on the track moves the value to that point",
        dropped.is_some_and(|value| (value - 75.0).abs() <= 10.0),
        &format!("pressing three quarters along gave {dropped:?}"),
    );
    stage.shot("choices-slider-pressed");

    stage.key(NamedKey::ArrowRight);
    let stepped = reading(stage);
    stage.report.check(
        "Slider",
        "the right arrow moves it one step",
        stepped
            .zip(dropped)
            .is_some_and(|(after, before)| (after - before - 5.0).abs() < 0.01),
        &format!("{dropped:?} became {stepped:?} on one press, and the step is 5"),
    );

    stage.key_with(NamedKey::Home, Modifiers::NONE);
    let home = reading(stage);
    stage.report.check(
        "Slider",
        "home goes to the bottom of the range",
        home == Some(0.0),
        &format!("home gave {home:?}"),
    );

    // Dragging, which is the interaction a press and a key cannot stand in for: the pointer is
    // captured on the press and the value has to follow it out of the control's own box.
    stage.drag(
        zgui::geom::Point::new(
            zgui::geom::DevicePx(track.origin.x.0 + 4.0),
            zgui::geom::DevicePx(track.origin.y.0 + track.size.height.0 / 2.0),
        ),
        zgui::geom::Point::new(
            zgui::geom::DevicePx(track.origin.x.0 + track.size.width.0 * 0.5),
            zgui::geom::DevicePx(track.origin.y.0 + track.size.height.0 * 4.0),
        ),
    );
    let dragged = reading(stage);
    stage.report.check(
        "Slider",
        "a drag that leaves the control still moves it",
        dragged.is_some_and(|value| (value - 50.0).abs() <= 10.0),
        &format!("dragging to the middle and below gave {dragged:?}"),
    );
    stage.shot("choices-slider-dragged");
}
