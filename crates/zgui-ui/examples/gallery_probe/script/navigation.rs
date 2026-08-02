//! The trail, the pager, the site navigation and the side panel.

use zgui::vocab::{Modifiers, NamedKey};

use crate::script::find;
use crate::stage::Stage;

/// Drives the navigation components.
pub(crate) fn run(stage: &mut Stage<'_>) {
    breadcrumb_and_pager(stage);
    navigation_menu(stage);
    sidebar(stage);
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}

/// The breadcrumb, and the pager that says which page it is on.
fn breadcrumb_and_pager(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Breadcrumb and pagination") else {
        stage.report.note("Breadcrumb", "the panel is not laid out");
        return;
    };
    let trail = ["Home", "Settings", "Billing"]
        .iter()
        .filter(|text| find::at_in(&census, panel, text).is_some())
        .count();
    stage.report.check(
        "Breadcrumb",
        "the whole trail is laid out",
        trail == 3,
        &format!("{trail} of 3 crumbs"),
    );

    let reading = |stage: &Stage<'_>| -> Option<String> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .find(|node| node.text.starts_with("page "))
            .map(|node| node.text.clone())
    };
    stage.report.check(
        "Pagination",
        "it starts on the page it was given",
        reading(stage).as_deref() == Some("page 4"),
        &format!("it reads {:?}", reading(stage)),
    );
    stage.shot("navigation-pager");

    // The next and previous controls carry no text of their own, so they are found by where they
    // sit in the row rather than by what they say.
    let row: Vec<_> = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.area() > 0.0 && node.text.is_empty())
        .filter_map(|node| node.rect)
        .collect();
    let Some(last) = row
        .iter()
        .max_by(|left, right| left.origin.x.0.total_cmp(&right.origin.x.0))
    else {
        stage.report.note("Pagination", "no controls in the row");
        return;
    };
    stage.click(zgui::geom::Point::new(
        zgui::geom::DevicePx(last.origin.x.0 + last.size.width.0 / 2.0),
        zgui::geom::DevicePx(last.origin.y.0 + last.size.height.0 / 2.0),
    ));
    let after = reading(stage);
    stage.report.check(
        "Pagination",
        "the last control in the row steps the page",
        after.as_deref() == Some("page 5"),
        &format!("clicking it gave {after:?}"),
    );
    stage.shot("navigation-pager-next");
}

/// The navigation menu, which opens a section under the bar.
fn navigation_menu(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Navigation menu") else {
        stage
            .report
            .note("NavigationMenu", "the panel is not laid out");
        return;
    };
    let Some(products) = find::at_in(&census, panel, "Products") else {
        stage.report.note("NavigationMenu", "no Products trigger");
        return;
    };
    // Every claim about the links is asked of the floating surface, not of the window: the item
    // list on the composites side of the page has a badge that also says Editor, and its box is
    // in the document at every scroll position — so a page-wide "is Editor on the screen" answers
    // for the badge whatever the menu does. A closed section mounts no content at all, which is
    // the Radix behaviour, so "closed" reads as no surface saying the link.
    stage.report.check(
        "NavigationMenu",
        "it starts with nothing open",
        !stage.floating("Editor"),
        "no surface holds the Products links",
    );
    stage.click(products);
    stage.settle(8);
    stage.report.check(
        "NavigationMenu",
        "a click opens its section",
        stage.floating("Editor") && stage.floating("Renderer"),
        "the Products links are laid out on the surface",
    );
    stage.shot("navigation-menu-open");

    let census = stage.census();
    if let Some(company) = find::at_in(&census, panel, "Company") {
        stage.click(company);
        stage.settle(8);
        stage.report.check(
            "NavigationMenu",
            "opening one section closes the other",
            stage.floating("Careers") && !stage.floating("Editor"),
            &format!(
                "the Company links are {} and the Products links are {}",
                if stage.floating("Careers") {
                    "up"
                } else {
                    "not up"
                },
                if stage.floating("Editor") {
                    "still up"
                } else {
                    "gone"
                }
            ),
        );
    }
    stage.key(NamedKey::Escape);
    stage.settle(6);
    stage.report.check(
        "NavigationMenu",
        "Escape closes what is open",
        !stage.floating("Careers"),
        "no surface holds the links any more",
    );
}

/// The sidebar, and the keystroke that folds it.
fn sidebar(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Sidebar") else {
        stage.report.note("Sidebar", "the panel is not laid out");
        return;
    };
    // The sidebar is the *smallest* box holding all of its own contents. The largest is the
    // `.sidebar-frame` the demo sits in, and the frame is as wide as the panel whether the
    // sidebar is folded or not — so a width read off it is the same number either way, and a
    // sidebar that folds perfectly well is reported as one that never moved. What is its own is
    // told by what it says: the header ("Acme Studio") and the first group label ("Platform")
    // are both inside the sidebar and nothing wider than the sidebar says only those.
    let width = |stage: &Stage<'_>| -> Option<f32> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| node.text.contains("Acme Studio") && node.text.contains("Platform"))
            .filter_map(|node| node.rect)
            .map(|rect| rect.size.width.0)
            .min_by(f32::total_cmp)
    };
    let open = width(stage);
    stage.report.check(
        "Sidebar",
        "it is laid out with its groups and its items",
        drawn(stage, "Acme Studio") && drawn(stage, "Deployments") && drawn(stage, "Analytics"),
        &format!("the panel is {open:?} device pixels wide"),
    );
    stage.shot("navigation-sidebar-open");

    // Ctrl+B is the sidebar's own shortcut, and it has to work from anywhere in the window rather
    // than only when the sidebar has focus.
    let Some(document) = find::at_in(&census, panel, "The document") else {
        stage.report.note("Sidebar", "no inset to click into");
        return;
    };
    stage.click(document);
    stage.key_with(NamedKey::Space, Modifiers::NONE);
    let census = stage.census();
    let _ = census;
    stage.deliver(crate::stage::synth::character(
        "b",
        zgui::vocab::KeyState::Pressed,
        Modifiers::CONTROL,
        zgui::vocab::Timestamp::from_origin(core::time::Duration::from_millis(0)),
    ));
    stage.settle(10);
    stage.deliver(crate::stage::synth::character(
        "b",
        zgui::vocab::KeyState::Released,
        Modifiers::CONTROL,
        zgui::vocab::Timestamp::from_origin(core::time::Duration::from_millis(0)),
    ));
    stage.settle(10);
    let folded = width(stage);
    stage.report.check(
        "Sidebar",
        "Ctrl+B folds it from anywhere in the window",
        open.zip(folded)
            .is_some_and(|(open, folded)| folded < open * 0.6)
            || folded.is_none(),
        &format!("it was {open:?} wide and is now {folded:?}"),
    );
    stage.shot("navigation-sidebar-folded");

    // And the trigger, which is the pointer's way to the same thing.
    //
    // Found by where it sits relative to the inset's own text rather than by how big it is. An
    // area threshold is a number in device pixels: the same control is inside it at one window
    // size and outside it at another, and the check then aims at whatever else the filter let
    // through — an icon, a rule, the fold of the sidebar itself — and reports a trigger that does
    // not work. What does not change with the window is the order the two are laid out in: the
    // trigger is the empty control immediately above the words it sits over.
    let census = stage.census();
    let anchor = census
        .saying("The document")
        .into_iter()
        .filter(|node| node.area() > 0.0)
        .filter_map(|node| node.rect)
        .min_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        });
    let trigger = anchor.and_then(|anchor| {
        census
            .nodes
            .iter()
            .filter(|node| node.text.is_empty() && node.area() > 0.0 && census.on_the_page(node))
            .filter_map(|node| node.rect.map(|rect| (node, rect)))
            .filter(|(_, rect)| {
                let bottom = rect.origin.y.0 + rect.size.height.0;
                let overlaps = rect.origin.x.0 < anchor.origin.x.0 + anchor.size.width.0
                    && rect.origin.x.0 + rect.size.width.0 > anchor.origin.x.0;
                bottom <= anchor.origin.y.0 + 1.0 && overlaps
            })
            .max_by(|(_, left), (_, right)| {
                (left.origin.y.0 + left.size.height.0)
                    .total_cmp(&(right.origin.y.0 + right.size.height.0))
            })
            .and_then(|(node, _)| node.centre())
    });
    if let Some(trigger) = trigger {
        // Where the pointer is going, so that a trigger that does not unfold and a click that
        // landed on an icon beside it are not the same finding.
        stage
            .report
            .note("Sidebar", &format!("the trigger is clicked at {trigger:?}"));
        stage.click(trigger);
        stage.settle(10);
        let unfolded = width(stage);
        stage.report.check(
            "Sidebar",
            "the trigger unfolds it again",
            unfolded.is_some_and(|now| open.is_some_and(|was| (now - was).abs() < was * 0.4)),
            &format!("it is {unfolded:?} wide, and it started at {open:?}"),
        );
        stage.shot("navigation-sidebar-unfolded");
    }
}
