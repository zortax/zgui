//! Accordions, collapsibles and tabs.
//!
//! Everything here is the same question asked three ways: is the content that should be hidden
//! actually gone from the layout, and is the content that should be there actually laid out? A
//! panel that folds by moving its content off the top of a clipped box still has it in the tree,
//! so each claim is about whether a box was produced, not about whether a node exists.

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// Drives the disclosure components.
pub(crate) fn run(stage: &mut Stage<'_>) {
    accordion(stage);
    collapsible(stage);
    tabs(stage);
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}

/// The accordion, opened and closed by pointer and by keyboard.
fn accordion(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Accordion") else {
        stage.report.note("Accordion", "the panel is not laid out");
        return;
    };
    stage.report.check(
        "Accordion",
        "the item it opened with is open",
        drawn(stage, "Within two working days, and sooner on a weekday."),
        "the shipping answer is laid out",
    );
    stage.report.check(
        "Accordion",
        "the items it did not open with are closed",
        !drawn(stage, "For thirty days, unopened."),
        "the returns answer produces no box",
    );
    stage.shot("disclosure-accordion-initial");

    let Some(returns) = find::at_in(&census, panel, "Can I send it back?") else {
        stage.report.note("Accordion", "no returns trigger");
        return;
    };
    stage.click(returns);
    // The section that was open folds away over a transition, in real time, so what is being
    // asked below has to be asked after it rather than during it.
    stage.wait(core::time::Duration::from_millis(600));
    stage.report.check(
        "Accordion",
        "clicking a trigger opens its answer",
        drawn(stage, "For thirty days, unopened."),
        "the returns answer is laid out after the click",
    );
    stage.report.check(
        "Accordion",
        "opening one closes the one that was open",
        !drawn(stage, "Within two working days, and sooner on a weekday."),
        "the shipping answer produces no box any more",
    );
    stage.shot("disclosure-accordion-opened");

    // The arrows walk the triggers, which is what makes it operable without a pointer.
    let before = stage.focused();
    stage.key(NamedKey::ArrowDown);
    let after = stage.focused();
    stage.report.check(
        "Accordion",
        "the down arrow walks to the next trigger",
        after.is_some() && after != before,
        &format!("focus went from {before:?} to {after:?}"),
    );
    stage.key(NamedKey::Enter);
    stage.report.check(
        "Accordion",
        "Enter opens what the arrows walked to",
        drawn(stage, "Anyone on the team."),
        "the support answer is laid out",
    );
    stage.shot("disclosure-accordion-keyed");
}

/// The collapsible, which is one thing rather than a set.
fn collapsible(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Collapsible") else {
        stage
            .report
            .note("Collapsible", "the panel is not laid out");
        return;
    };
    stage.report.check(
        "Collapsible",
        "it starts folded",
        !drawn(stage, "Arrives Thursday, signed for."),
        "the content produces no box",
    );
    let Some(trigger) = find::at_in(&census, panel, "Delivery details") else {
        stage.report.note("Collapsible", "no trigger");
        return;
    };
    stage.click(trigger);
    stage.settle(8);
    stage.report.check(
        "Collapsible",
        "clicking unfolds it",
        drawn(stage, "Arrives Thursday, signed for."),
        "the content is laid out",
    );
    stage.shot("disclosure-collapsible-open");
    stage.click(trigger);
    // Folding is a transition, over real time. Frames run inside one turn of the loop take none of
    // it, so a section asked whether it has folded in the frame after the click is asked while it
    // is still on its way down — and one folding perfectly well answers exactly like one that
    // ignored the click.
    stage.wait(core::time::Duration::from_millis(600));
    stage.report.check(
        "Collapsible",
        "clicking again folds it back",
        !drawn(stage, "Arrives Thursday, signed for."),
        "the content produces no box again",
    );
}

/// The tabs, whose activation follows the arrows.
fn tabs(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Tabs") else {
        stage.report.note("Tabs", "the panel is not laid out");
        return;
    };
    stage.report.check(
        "Tabs",
        "the panel it opened on is the only one showing",
        drawn(stage, "Your name, your picture and how to reach you.")
            && !drawn(stage, "Cards, invoices and the plan you are on."),
        "the profile panel is laid out and the billing one is not",
    );
    stage.shot("disclosure-tabs-profile");

    let Some(billing) = find::at_in(&census, panel, "Billing") else {
        stage.report.note("Tabs", "no billing tab");
        return;
    };
    stage.click(billing);
    stage.report.check(
        "Tabs",
        "clicking a tab shows its panel and hides the other",
        drawn(stage, "Cards, invoices and the plan you are on.")
            && !drawn(stage, "Your name, your picture and how to reach you."),
        "the billing panel is laid out and the profile one is not",
    );
    stage.shot("disclosure-tabs-billing");

    // Roving focus: one tab in the sequence, the arrows moving between them, and the disabled tab
    // never landed on.
    let before = stage.focused_text();
    stage.key(NamedKey::ArrowLeft);
    let left = stage.focused_text();
    stage.report.check(
        "Tabs",
        "the left arrow moves between tabs",
        left != before && !left.is_empty(),
        &format!("focus went from {before:?} to {left:?}"),
    );
    stage.report.check(
        "Tabs",
        "the arrows activate as they move",
        drawn(stage, "Your name, your picture and how to reach you."),
        "moving back to Profile showed the profile panel",
    );
    stage.key(NamedKey::ArrowRight);
    stage.key(NamedKey::ArrowRight);
    let past = stage.focused_text();
    stage.report.check(
        "Tabs",
        "the disabled tab is never landed on",
        past != "Team",
        &format!("two rights from Profile landed on {past:?}"),
    );
    stage.shot("disclosure-tabs-keyed");
}
