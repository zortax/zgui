//! Menus, the menu bar, and the three ways of choosing from a list.
//!
//! The select is the one to watch. A closed select that shows its placeholder instead of what was
//! chosen is a defect nobody sees from inside an open list, so every claim here reads the trigger
//! *after* the list has gone rather than while it is still up.

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// Drives the menus and the lists.
pub(crate) fn run(stage: &mut Stage<'_>) {
    dropdown(stage);
    context_menu(stage);
    menubar(stage);
    select(stage);
    combobox(stage);
    command(stage);
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}

/// The dropdown menu, with its submenu, its checks and its radios.
fn dropdown(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dropdown and context menu") else {
        stage
            .report
            .note("DropdownMenu", "the panel is not laid out");
        return;
    };
    let Some(trigger) = find::at_in(&census, panel, "Account") else {
        stage.report.note("DropdownMenu", "no trigger");
        return;
    };
    stage.click(trigger);
    stage.settle(8);
    let items = [
        "Settings",
        "Billing",
        "Wrap lines",
        "Compact",
        "Export",
        "Sign out",
    ]
    .iter()
    .filter(|text| drawn(stage, text))
    .count();
    stage.report.check(
        "DropdownMenu",
        "it opens with all its items",
        items == 6,
        &format!("{items} of 6 items are laid out"),
    );
    stage.shot("menus-dropdown-open");

    // What has focus with the menu up, which is what every keyboard claim below depends on.
    stage.report.note(
        "DropdownMenu",
        &format!("with the menu open focus is on {:?}", stage.focused_text()),
    );

    // Typing a letter jumps to the item beginning with it, which is the one keyboard behaviour a
    // menu has that a list of buttons does not.
    stage.type_text("b");
    let jumped = stage.focused_text();
    stage.report.check(
        "DropdownMenu",
        "typeahead jumps to the item that starts with the letter",
        jumped == "Billing",
        &format!("typing b focused {jumped:?}"),
    );

    stage.key(NamedKey::ArrowDown);
    let walked = stage.focused_text();
    stage.report.check(
        "DropdownMenu",
        "the down arrow walks the items",
        !walked.is_empty() && walked != jumped,
        &format!("the arrow moved from {jumped:?} to {walked:?}"),
    );
    stage.report.check(
        "DropdownMenu",
        "the disabled item is never landed on",
        walked != "Team (none yet)",
        &format!("the arrow landed on {walked:?}"),
    );

    // The submenu, which is a second surface on top of the first.
    let census = stage.census();
    if let Some(export) = census.innermost("Export").and_then(|node| node.centre()) {
        stage.move_to(export);
        stage.wait(core::time::Duration::from_millis(500));
        stage.report.check(
            "MenuSub",
            "resting on the sub-trigger opens the submenu",
            drawn(stage, "As CSV") && drawn(stage, "As JSON"),
            "both submenu items are laid out",
        );
        stage.shot("menus-dropdown-submenu");
        stage.key(NamedKey::Escape);
        stage.settle(6);
        stage.report.check(
            "MenuSub",
            "Escape closes the submenu and leaves the menu",
            !drawn(stage, "As CSV") && drawn(stage, "Settings"),
            &format!(
                "the submenu is {} and the menu is {}",
                if drawn(stage, "As CSV") {
                    "open"
                } else {
                    "closed"
                },
                if drawn(stage, "Settings") {
                    "open"
                } else {
                    "closed"
                }
            ),
        );
    }

    stage.key(NamedKey::Escape);
    stage.settle(6);
    // Of the *surface*, not of the window: the command palette on the same page lists an item
    // called Settings too, so a menu that closed perfectly well reads as one that refused to.
    stage.report.check(
        "DropdownMenu",
        "Escape closes the menu",
        !stage.floating("Settings"),
        &format!(
            "with the menu dismissed, a surface saying Settings is {}",
            if stage.floating("Settings") {
                "still up"
            } else {
                "gone"
            }
        ),
    );
    stage.report.check(
        "DropdownMenu",
        "focus goes back to the trigger",
        stage.focused_text() == "Account",
        &format!("focus is on {:?}", stage.focused_text()),
    );
}

/// The context menu, which the secondary button opens.
fn context_menu(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Dropdown and context menu") else {
        return;
    };
    let Some(target) = find::at_in(&census, panel, "Right-click here.") else {
        stage.report.note("ContextMenu", "no target");
        return;
    };
    stage.right_click(target);
    stage.settle(8);
    // Of the surface again: the button-group demo has a button that also says Paste, so the
    // page-wide question answers for the page and not for the menu.
    stage.report.check(
        "ContextMenu",
        "the secondary button opens it",
        stage.floating("Paste") && stage.floating("Clear"),
        "both items are laid out on the surface",
    );
    stage.shot("menus-context");
    stage.key(NamedKey::Escape);
    stage.settle(6);
    stage.report.check(
        "ContextMenu",
        "Escape closes it",
        !stage.floating("Paste"),
        "no surface says Paste any more",
    );
}

/// The menu bar, walked left and right.
fn menubar(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Menubar") else {
        stage.report.note("Menubar", "the panel is not laid out");
        return;
    };
    let Some(file) = find::at_in(&census, panel, "File") else {
        stage.report.note("Menubar", "no File menu");
        return;
    };
    stage.click(file);
    stage.settle(8);
    // Of the *surface*, not of the window: the Button panel has two buttons that also say New,
    // and their boxes are in the document at every scroll position — so a page-wide "is New on
    // the screen" is true whether the menu opened or not, in both directions.
    let opened = stage.floating("New") && stage.floating("Open…");
    stage.report.check(
        "Menubar",
        "a click opens the menu under it",
        opened,
        &format!("{}; {}", stage.presence("New"), stage.presence("Open…")),
    );
    stage.shot("menus-menubar-file");

    // With one menu open, the right arrow moves to the next one and opens that instead. The File
    // items have to be off their surface by the time this looks: a menubar swap is instant — the
    // leaving menu has no exit animation, precisely so the two are never up together.
    stage.key(NamedKey::ArrowRight);
    stage.settle(6);
    stage.report.check(
        "Menubar",
        "the right arrow moves along the bar and swaps the open menu",
        stage.floating("Undo") && !stage.floating("New"),
        &format!(
            "the Edit items are {} and the File items are {}",
            if stage.floating("Undo") {
                "up"
            } else {
                "not up"
            },
            if stage.floating("New") {
                "still up"
            } else {
                "gone"
            }
        ),
    );
    stage.shot("menus-menubar-edit");
    stage.key(NamedKey::Escape);
    stage.settle(6);
    stage.report.check(
        "Menubar",
        "Escape closes the bar's menu",
        !stage.floating("Undo"),
        "no surface says Undo any more",
    );
}

/// The select: chosen from a list, and read back with the list closed.
fn select(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Select and combobox") else {
        stage.report.note("Select", "the panel is not laid out");
        return;
    };
    // The trigger shows what is chosen, which starts as the pound.
    let Some(trigger) = find::at_in(&census, panel, "Pound sterling") else {
        stage
            .report
            .note("Select", "the trigger does not show the chosen value");
        return;
    };
    stage.report.check(
        "Select",
        "a closed select shows what is chosen, not its placeholder",
        !drawn(stage, "Choose one"),
        "the trigger says Pound sterling and the placeholder is not laid out",
    );
    stage.click(trigger);
    stage.settle(8);
    // The two headings are asked about by where they were *placed*, and their items by whether they
    // are on the screen. A heading is a `label`, which is an inline element, and an inline element
    // is given no box of its own by this engine: the run of text inside it is laid out in the line
    // its containing block owns, and both the element and its text answer "where are you" with a
    // point and no size. The text is painted — every capture of an open list shows both headings —
    // so a claim written on the size of that box is a claim about the shape of the fragment tree
    // and not about what a person can see. Where it was placed is the strongest thing about it the
    // document will say, and it is enough to tell a heading that heads its group from one that is
    // missing, in the wrong group, or outside the list altogether.
    let census = stage.census();
    let placed = |text: &str| {
        census
            .saying(text)
            .into_iter()
            .filter_map(|node| node.rect)
            .map(|rect| (rect.origin.x.0, rect.origin.y.0))
            .next()
    };
    let items = drawn(stage, "US dollar") && drawn(stage, "Euro");
    let ordered = match (
        placed("Europe"),
        placed("Euro"),
        placed("Elsewhere"),
        placed("US dollar"),
    ) {
        (Some(europe), Some(euro), Some(elsewhere), Some(dollar)) => {
            europe.1 < euro.1 && euro.1 < elsewhere.1 && elsewhere.1 < dollar.1
        }
        _ => false,
    };
    stage.report.check(
        "Select",
        "it opens with its groups and labels",
        items && ordered,
        &format!(
            "the items are {}, and the headings are placed at Europe {:?} Elsewhere {:?} against \
             items Euro {:?} US dollar {:?}",
            if items {
                "on the screen"
            } else {
                "not laid out"
            },
            placed("Europe"),
            placed("Elsewhere"),
            placed("Euro"),
            placed("US dollar"),
        ),
    );
    // Everything the open list holds, so that a group heading missing from the picture and one
    // missing only from this process's view of the document are told apart.
    let listed: Vec<String> = stage
        .census()
        .nodes
        .iter()
        .filter(|node| {
            node.text.len() < 20 && !node.text.is_empty() && node.text.contains("uro")
                || node.text == "Elsewhere"
        })
        .map(|node| format!("{:?} {:?}", node.text, node.rect))
        .collect();
    stage.report.note(
        "Select",
        &format!("with the list up the census holds {listed:?}"),
    );
    stage.shot("menus-select-open");

    let census = stage.census();
    let Some(dollar) = census
        .nodes
        .iter()
        .filter(|node| node.text == "US dollar" && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.centre())
    else {
        stage.report.note("Select", "no US dollar item to choose");
        return;
    };
    stage.click(dollar);
    stage.settle(8);

    // Read the trigger with the list closed. This is the check that was passing vacuously before,
    // because it was made with the list open again.
    let closed = !drawn(stage, "Europe");
    let shows = drawn(stage, "US dollar");
    stage.report.check(
        "Select",
        "choosing closes the list and leaves the choice on the trigger",
        closed && shows,
        &format!(
            "the list is {} and the trigger says {}",
            if closed { "closed" } else { "still open" },
            if shows { "US dollar" } else { "something else" }
        ),
    );
    stage.shot("menus-select-chosen");
}

/// The combobox, which is a list that is searched rather than scrolled.
fn combobox(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Select and combobox") else {
        return;
    };
    let Some(field) = find::at_in(&census, panel, "Search countries") else {
        stage.report.note("Combobox", "no input");
        return;
    };
    stage.click(field);
    stage.settle(6);
    stage.type_text("ire");
    stage.settle(6);
    stage.report.check(
        "Combobox",
        "typing filters the list",
        drawn(stage, "Ireland") && !drawn(stage, "France"),
        &format!(
            "Ireland is {} and France is {}",
            if drawn(stage, "Ireland") {
                "shown"
            } else {
                "hidden"
            },
            if drawn(stage, "France") {
                "still shown"
            } else {
                "hidden"
            }
        ),
    );
    stage.shot("menus-combobox-filtered");

    stage.type_text("zzz");
    stage.settle(6);
    stage.report.check(
        "Combobox",
        "a search that matches nothing says so",
        drawn(stage, "No country by that name."),
        "the empty message is laid out",
    );
    for _ in 0..6 {
        stage.key(NamedKey::Backspace);
    }
    stage.key(NamedKey::Escape);
    stage.settle(6);
}

/// The command palette.
fn command(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Command") else {
        stage.report.note("Command", "the panel is not laid out");
        return;
    };
    let Some(field) = find::at_in(&census, panel, "Type a command…") else {
        stage.report.note("Command", "no input");
        return;
    };
    stage.report.check(
        "Command",
        "it lists what there is before anything is typed",
        drawn(stage, "New invoice") && drawn(stage, "Settings"),
        "items from both groups are laid out",
    );
    stage.click(field);
    stage.type_text("invo");
    stage.settle(6);
    stage.report.check(
        "Command",
        "typing narrows it",
        drawn(stage, "New invoice") && !drawn(stage, "Settings"),
        &format!(
            "the invoice items are {} and Settings is {}",
            if drawn(stage, "New invoice") {
                "shown"
            } else {
                "hidden"
            },
            if drawn(stage, "Settings") {
                "still shown"
            } else {
                "hidden"
            }
        ),
    );
    stage.shot("menus-command-filtered");

    stage.key(NamedKey::ArrowDown);
    stage.key(NamedKey::Enter);
    stage.settle(6);
    stage.report.check(
        "Command",
        "choosing an item runs what it is for",
        drawn(stage, "New invoice"),
        "the panel's own echo of what was chosen is laid out",
    );
    stage.shot("menus-command-chosen");
}
