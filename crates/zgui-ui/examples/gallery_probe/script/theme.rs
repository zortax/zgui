//! The colour scheme, and whether flipping it reaches the whole document.
//!
//! This is a claim about pixels and nothing else. The scheme is one signal, and every component
//! resolves its colours against the custom properties that signal writes — so a flip that
//! recascades only part of the document leaves a tree that is identical in every way this process
//! can inspect and a window that is half light and half dark.
//!
//! What is done here, then, is to capture the whole window before and after, and to mark the
//! rectangle of every panel so that the comparison afterwards can be made panel by panel rather
//! than over the window as a whole. A flip that changed only the masthead would move plenty of
//! pixels; it would move none inside the tenth panel.

use crate::script::find;
use crate::stage::Stage;

/// Every panel in the gallery, by the title it is headed with.
const PANELS: [&str; 18] = [
    "Button",
    "Badge, label, separator",
    "Avatar and skeleton",
    "Icon",
    "Input and textarea",
    "One-time code",
    "Form",
    "Checkbox",
    "Radio and switch",
    "Toggle",
    "Slider",
    "Alert",
    "Card",
    "Progress",
    "Accordion",
    "Collapsible",
    "Tabs",
    "Table",
];

/// Drives the theme flip.
pub(crate) fn run(stage: &mut Stage<'_>) {
    // The masthead is at the top of a page that is by now scrolled to the bottom of a document
    // eight times the height of the window, so the switch is a thousand pixels above the surface.
    // A click aimed there lands on the desktop, and a scheme that flips perfectly well when the
    // switch is pressed is reported as one that does not flip at all.
    if let Some(masthead) = stage.census().panel("zgui components").map(|node| node.id) {
        stage.reveal(masthead);
    }
    let census = stage.census();
    for title in PANELS {
        if let Some(rect) = find::panel(&census, title) {
            find::mark(stage, &format!("theme:{title}"), rect);
        }
    }
    let scheme = |stage: &Stage<'_>| -> Option<String> {
        stage
            .census()
            .nodes
            .iter()
            .find(|node| (node.text == "light" || node.text == "dark") && node.area() > 0.0)
            .map(|node| node.text.clone())
    };
    // The masthead badge names the scheme, so what the window claims to be showing is readable.
    let before = scheme(stage);
    stage.shot("theme-light");

    let Some(switch) = census
        .control("Dark")
        .and_then(|node| node.rect)
        .map(|label| {
            zgui::geom::Point::new(
                zgui::geom::DevicePx(label.origin.x.0 + label.size.width.0 + 24.0),
                zgui::geom::DevicePx(label.origin.y.0 + label.size.height.0 / 2.0),
            )
        })
    else {
        stage
            .report
            .note("ThemeProvider", "no scheme switch in the masthead");
        return;
    };
    // Where the click is aimed and what is under it, so that a scheme that does not flip and a
    // click that landed beside the switch are not the same finding.
    let under: Vec<String> = stage
        .census()
        .nodes
        .iter()
        .filter(|node| {
            node.rect.is_some_and(|rect| {
                rect.origin.x.0 <= switch.x.0
                    && switch.x.0 <= rect.origin.x.0 + rect.size.width.0
                    && rect.origin.y.0 <= switch.y.0
                    && switch.y.0 <= rect.origin.y.0 + rect.size.height.0
            })
        })
        .map(|node| {
            format!(
                "{:?} {:?}",
                node.text.chars().take(12).collect::<String>(),
                node.rect
            )
        })
        .collect();
    stage.report.note(
        "ThemeProvider",
        &format!("the switch is clicked at {switch:?}, over {under:?}"),
    );

    stage.click(switch);
    stage.settle(12);
    let after = scheme(stage);
    stage.report.check(
        "ThemeProvider",
        "the switch changes the scheme the window says it is in",
        before.as_deref() == Some("light") && after.as_deref() == Some("dark"),
        &format!("the badge went from {before:?} to {after:?}"),
    );
    stage.shot("theme-dark");

    // An overlay opened *after* the flip has to come up in the new scheme too, because it is
    // portalled onto a band of the document that the flip has to have reached as well.
    let census = stage.census();
    if let Some(panel) = find::panel(&census, "Dialog")
        && let Some(trigger) = find::at_in(&census, panel, "Rename…")
    {
        stage.click(trigger);
        stage.settle(10);
        stage.shot("theme-dark-dialog");
        if let Some(rect) = stage
            .census()
            .control("Rename project")
            .and_then(|node| node.rect)
        {
            find::mark(stage, "theme:dialog", rect);
        }
        stage.key(zgui::vocab::NamedKey::Escape);
        stage.settle(10);
    }

    // Back to light, so that the flip is shown to work in both directions.
    stage.click(switch);
    stage.settle(12);
    let back = scheme(stage);
    stage.report.check(
        "ThemeProvider",
        "the switch goes back again",
        back.as_deref() == Some("light"),
        &format!("the badge reads {back:?}"),
    );
    stage.shot("theme-light-again");
}
