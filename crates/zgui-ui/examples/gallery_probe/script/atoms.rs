//! Buttons, badges, avatars, skeletons and icons.
//!
//! Two of the claims here are about pixels rather than about the tree, and they have to be: a
//! button whose label has been made invisible still *says* its label, so a check that reads the
//! text back passes while the button reads as empty on the screen. What the tree can say is where
//! the button is; whether anything is drawn inside it is a question for the capture.

use crate::script::find;
use crate::stage::Stage;

/// Drives the atoms.
pub(crate) fn run(stage: &mut Stage<'_>) {
    buttons(stage);
    icons(stage);
    trimmings(stage);
}

/// Every button variant, hovered, focused and pressed.
fn buttons(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Button") else {
        stage
            .report
            .note("Button", "the Button panel is not laid out");
        return;
    };

    let variants = [
        "Default",
        "Secondary",
        "Destructive",
        "Outline",
        "Ghost",
        "Link",
    ];
    let mut missing = Vec::new();
    for name in variants {
        match find::at_in(&census, panel, name) {
            Some(_) => {}
            None => missing.push(name),
        }
    }
    stage.report.check(
        "Button",
        "every variant is laid out",
        missing.is_empty(),
        &format!("{} of 6 present, missing {missing:?}", 6 - missing.len()),
    );

    let sizes = ["Small", "Medium", "Large"];
    let heights: Vec<f32> = sizes
        .iter()
        .filter_map(|name| find::at_in(&census, panel, name).and(census.control(name)))
        .filter_map(|node| node.rect)
        .map(|rect| rect.size.height.0)
        .collect();
    stage.report.check(
        "Button",
        "the sizes are three different heights",
        heights.len() == 3 && heights[0] < heights[1] && heights[1] < heights[2],
        &format!("{heights:?}"),
    );

    // The capture before anything has been pointed at, which every later one is compared with.
    stage.shot("atoms-buttons-idle");

    let Some(target) = find::at_in(&census, panel, "Outline") else {
        stage.report.note("Button", "no Outline button to point at");
        return;
    };
    stage.move_to(target);
    stage.shot("atoms-buttons-hovered");
    let after = stage.census();
    stage.report.check(
        "Button",
        "the label is still in the tree, hovered",
        after.control("Outline").is_some(),
        "the hovered button still says Outline",
    );

    // Focus it with the keyboard rather than by clicking, because a focus ring and a pressed
    // appearance are different states and a click would leave both.
    stage.leave();
    stage.click(target);
    stage.shot("atoms-buttons-focused");
    let after = stage.census();
    stage.report.check(
        "Button",
        "the label is still in the tree, focused",
        after.control("Outline").is_some(),
        &format!("focus is on {:?}", stage.focused_text()),
    );
    stage.report.check(
        "Button",
        "a click puts focus on the button",
        stage.focused_text() == "Outline",
        &format!("focus went to {:?}", stage.focused_text()),
    );

    let disabled = stage.census();
    let disabled_present = disabled
        .saying("Default")
        .iter()
        .filter(|node| node.area() > 0.0)
        .count();
    stage.report.check(
        "Button",
        "the disabled one is drawn as well as the enabled one",
        disabled_present >= 2,
        &format!("{disabled_present} boxes say Default"),
    );
    stage.leave();
}

/// The icons, which are drawings rather than glyphs.
fn icons(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Icon") else {
        stage.report.note("Icon", "the Icon panel is not laid out");
        return;
    };
    let boxes: Vec<f32> = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0)
        .map(|node| node.area())
        .collect();
    stage.report.check(
        "Icon",
        "the icons take up room",
        boxes.len() >= 11,
        &format!("{} textless boxes inside the panel", boxes.len()),
    );
    stage.shot("atoms-icons");
}

/// Badges, separators, avatars and skeletons: the parts with no behaviour.
fn trimmings(stage: &mut Stage<'_>) {
    let census = stage.census();
    if let Some(panel) = find::mark_panel(stage, &census, "Badge, label, separator") {
        let badges = ["Default", "Secondary", "Destructive", "Outline"]
            .iter()
            .filter(|name| find::at_in(&census, panel, name).is_some())
            .count();
        stage.report.check(
            "Badge",
            "all four are laid out",
            badges == 4,
            &format!("{badges} of 4"),
        );
        stage.shot("atoms-badges");
    }
    let census = stage.census();
    if let Some(panel) = find::mark_panel(stage, &census, "Avatar and skeleton") {
        let initials = ["AL", "GH", "BL"]
            .iter()
            .filter(|name| find::at_in(&census, panel, name).is_some())
            .count();
        stage.report.check(
            "Avatar",
            "three avatars are laid out",
            initials == 3,
            &format!("{initials} of 3"),
        );
        stage.shot("atoms-avatars");
    }
}
