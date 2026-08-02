//! The components that hold things rather than being operated: what they build, and what they say.

mod harness;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::vocab::{Live, SemanticFlags};
use zgui_ui::prelude::*;

use crate::harness::Harness;

#[test]
fn a_badge_is_writing_rather_than_a_control() {
    let harness = Harness::open();
    harness.mount(|| view! { Badge(variant = BadgeVariant::Destructive) {"failed"} });
    let badge = harness.only_child();

    assert_eq!(
        harness.attribute(badge, "data-variant").as_deref(),
        Some("destructive")
    );
    assert_eq!(
        harness.window.dom.tree().text_content(badge),
        "failed",
        "the badge did not put its children anywhere"
    );
    assert_eq!(
        harness.window.dom.tree().semantics(badge).map(|it| it.role),
        None,
        "a badge that claimed a role would be announced as something to operate"
    );
}

#[test]
fn a_decorative_separator_is_kept_out_of_the_accessibility_tree_and_a_real_one_is_not() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Separator()
                Separator(decorative = false, orientation = SeparatorOrientation::Vertical)
            }
        }
    });
    let rules: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-separator"))
        })
        .collect();
    assert_eq!(rules.len(), 2);

    assert!(
        harness
            .semantics(rules[0])
            .flags
            .contains(SemanticFlags::HIDDEN),
        "a rule between two headed sections is noise between two things a reader already told apart"
    );
    assert_eq!(harness.semantics(rules[1]).role, Role::Splitter);
    assert_eq!(
        harness.semantics(rules[1]).orientation,
        Some(zgui::vocab::Orientation::Vertical)
    );
    assert_eq!(
        harness.attribute(rules[1], "data-orientation").as_deref(),
        Some("vertical")
    );
}

#[test]
fn a_skeleton_says_it_is_busy_and_its_sheet_carries_the_animation_that_says_so_visually() {
    let harness = Harness::open();
    harness.mount(|| view! { Skeleton() });
    let block = harness.only_child();

    assert!(harness.semantics(block).flags.contains(SemanticFlags::BUSY));
    let css = harness
        .window
        .host
        .stylesheet("zui-skeleton")
        .expect("the skeleton installs its own sheet");
    assert!(
        css.contains("@keyframes zui-skeleton-pulse"),
        "the pulse is what means `wait` to anyone looking at it"
    );
}

#[test]
fn an_alert_is_a_live_region_unless_it_was_already_on_the_surface() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Alert {
                    AlertTitle {"Saved"}
                    AlertDescription {"Everything is up to date."}
                }
                Alert(live = false, icon = false) {
                    AlertTitle {"Read me"}
                }
            }
        }
    });
    let alerts: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-alert"))
        })
        .collect();
    assert_eq!(alerts.len(), 2);

    assert_eq!(harness.semantics(alerts[0]).role, Role::Alert);
    assert_eq!(harness.semantics(alerts[0]).live, Some(Live::Polite));
    assert_eq!(
        harness.semantics(alerts[1]).role,
        Role::Group,
        "an alert that was there when the surface opened interrupts a reader for nothing"
    );
}

#[test]
fn an_alert_s_icon_says_nothing_a_reader_has_not_already_been_told() {
    let harness = Harness::open();
    harness.mount(|| view! { Alert(variant = AlertVariant::Destructive) {AlertTitle {"Gone"}} });
    let icon = harness.find("zui-icon");

    assert_eq!(
        harness.attribute(icon, "data-icon").as_deref(),
        Some("alert-triangle"),
        "the icon follows the variant rather than being passed in"
    );
    assert!(
        harness
            .semantics(icon)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "the icon repeats the title, and a reader that met both would say it twice"
    );
}

#[test]
fn a_card_puts_its_pieces_where_a_sheet_can_find_them() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Card {
                CardHeader {
                    CardTitle {"March"}
                    CardDescription {"Due on the 28th"}
                }
                CardContent {text {"£42.00"}}
                CardFooter {Button {"Pay"}}
            }
        }
    });
    let card = harness.only_child();

    assert_eq!(harness.semantics(card).role, Role::Group);
    for part in [
        "zui-card__header",
        "zui-card__title",
        "zui-card__description",
        "zui-card__content",
        "zui-card__footer",
    ] {
        let node = harness.find(part);
        assert!(
            !harness.window.dom.tree().children(node).is_empty(),
            "`{part}` was built with nothing inside it"
        );
    }
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .text_content(harness.find("zui-card__title")),
        "March"
    );
}

#[test]
fn a_progress_bar_reports_a_number_when_it_has_one_and_nothing_when_it_does_not() {
    let harness = Harness::open();
    let done = RwSignal::new_local(Some(35.0));
    harness.mount(move || view! { Progress(value = done, max = 100.0, label = "Upload") });
    let bar = harness.only_child();

    assert_eq!(harness.semantics(bar).role, Role::ProgressIndicator);
    assert_eq!(harness.semantics(bar).numeric.value, Some(35.0));
    assert_eq!(
        harness.attribute(bar, "data-state").as_deref(),
        Some("determinate")
    );
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .custom_property(
                bar,
                zgui::view::CustomPropertyName::new("zui-progress-fraction")
            )
            .as_deref(),
        Some("35.0000%")
    );

    done.set(None);
    harness.window.frame();

    assert_eq!(
        harness.semantics(bar).numeric.value,
        None,
        "reporting zero for a bar that does not know is a claim that nothing has happened"
    );
    assert_eq!(
        harness.attribute(bar, "data-state").as_deref(),
        Some("indeterminate")
    );
}

#[test]
fn an_avatar_keeps_its_fallback_underneath_rather_than_swapping_it_out() {
    let harness = Harness::open();
    let src = RwSignal::new_local(None::<String>);
    harness.mount(move || view! { Avatar(src = src, label = "Ada Lovelace") {"AL"} });
    let avatar = harness.only_child();

    assert_eq!(
        harness.semantics(avatar).label.as_deref(),
        Some("Ada Lovelace")
    );
    assert_eq!(harness.window.dom.tree().text_content(avatar), "AL");

    src.set(Some("/avatars/ada.png".to_owned()));
    harness.window.frame();

    assert_eq!(
        harness.window.dom.tree().text_content(avatar),
        "AL",
        "the initials were taken away, so a picture that fails to load leaves an empty circle"
    );
    let picture = harness.find("zui-avatar__image");
    assert_eq!(
        harness.attribute(picture, "src").as_deref(),
        Some("/avatars/ada.png")
    );
    assert!(
        harness
            .semantics(picture)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "the picture and the avatar are two items in the tree for one thing on the surface"
    );
}

#[test]
fn every_component_installs_exactly_one_sheet_however_many_of_it_are_mounted() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Button {"a"}
                Button(variant = ButtonVariant::Ghost) {"b"}
                Badge {"c"}
                Badge(variant = BadgeVariant::Outline) {"d"}
                Separator()
                Separator()
            }
        }
    });

    let names = harness.window.host.stylesheet_names();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        names.len(),
        sorted.len(),
        "a sheet was installed under two names: {names:?}"
    );
    assert_eq!(
        harness.window.host.stylesheet_installs(),
        names.len(),
        "six components installed more sheets than there are kinds of them"
    );
}
