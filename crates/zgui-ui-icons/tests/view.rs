//! The `Icon` component, mounted and read back.

mod harness;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::vocab::PropKey;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::{CHEVRON_DOWN, CHEVRON_UP};
use zgui_ui_icons::set::mark::{CHECK, CROSS};

use crate::harness::Harness;

#[test]
fn the_outline_reaches_the_element_as_the_path_notation_it_was_written_in() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK) });
    let icon = harness.only_child();

    let data = harness
        .window
        .dom
        .tree()
        .property(icon, PropKey::new("d"))
        .expect("the drawing carries its outline");
    assert_eq!(
        data.as_str(),
        Some(CHECK.path_data()),
        "the path was re-serialised on the way instead of being handed over unchanged"
    );
}

/// The outline is written in the icon's own square and drawn at whatever size CSS gives the
/// element, so the square has to cross with it. Without the view box the outline is drawn at its
/// own numbers in CSS pixels — a twenty-four unit mark inside a sixteen pixel box, cropped.
#[test]
fn the_square_the_outline_was_written_in_reaches_the_element_beside_it() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK) });
    let icon = harness.only_child();

    let view_box = harness
        .window
        .dom
        .tree()
        .property(icon, PropKey::new("viewBox"))
        .expect("the drawing carries the space its outline is written in");
    assert_eq!(
        view_box.as_str(),
        Some(format!("0 0 {0} {0}", CHECK.view_box()).as_str()),
    );
}

#[test]
fn the_element_is_a_drawing_rather_than_a_box_with_a_class_on_it() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK) });
    assert_eq!(
        harness.window.dom.tree().element_name(harness.only_child()),
        Some("vector".to_owned()),
        "an icon that is not a `<vector>` is not drawn by the path renderer at all"
    );
}

#[test]
fn the_size_variant_reaches_both_the_class_list_and_the_data_attribute() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK, size = IconSize::Lg) });
    let icon = harness.only_child();
    let tree = harness.window.dom.tree();

    let classes: Vec<String> = tree
        .classes(icon)
        .iter()
        .map(|class| class.as_str().to_owned())
        .collect();
    assert!(classes.iter().any(|class| class == "zui-icon"));
    assert!(classes.iter().any(|class| class == "zui-icon--lg"));
    assert_eq!(
        tree.attribute(icon, zgui::view::AttrName::new("data-size"))
            .as_deref(),
        Some("lg")
    );
    assert_eq!(
        tree.attribute(icon, zgui::view::AttrName::new("data-icon"))
            .as_deref(),
        Some("check"),
        "a style sheet cannot select one icon out of a set without this"
    );
}

#[test]
fn an_unnamed_icon_is_kept_out_of_the_accessibility_tree() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK) });
    let semantics = harness
        .window
        .dom
        .tree()
        .semantics(harness.only_child())
        .expect("an icon says what it is");

    assert!(
        semantics.flags.contains(zgui::vocab::SemanticFlags::HIDDEN),
        "a decorative icon beside a word makes a reader say that word twice"
    );
    assert_eq!(semantics.label, None);
}

#[test]
fn a_named_icon_is_an_image_a_reader_can_announce() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CROSS, label = "Close") });
    let semantics = harness
        .window
        .dom
        .tree()
        .semantics(harness.only_child())
        .expect("an icon says what it is");

    assert_eq!(semantics.role, Role::Image);
    assert_eq!(semantics.label.as_deref(), Some("Close"));
    assert!(!semantics.flags.contains(zgui::vocab::SemanticFlags::HIDDEN));
}

#[test]
fn changing_the_icon_rewrites_the_outline_without_replacing_the_element() {
    let harness = Harness::open();
    let open = harness.window.scope.with(|| RwSignal::new_local(false));
    harness.mount(move || {
        view! {
            Icon(icon = Signal::derive_local(move || {
                if open.get() { CHEVRON_UP } else { CHEVRON_DOWN }
            }))
        }
    });
    let icon = harness.only_child();
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .property(icon, PropKey::new("d"))
            .and_then(|value| value.as_str().map(str::to_owned))
            .as_deref(),
        Some(CHEVRON_DOWN.path_data())
    );

    open.set(true);
    harness.window.frame();

    assert_eq!(
        harness.only_child(),
        icon,
        "the element was replaced rather than rewritten, which loses its place in the tree"
    );
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .property(icon, PropKey::new("d"))
            .and_then(|value| value.as_str().map(str::to_owned))
            .as_deref(),
        Some(CHEVRON_UP.path_data())
    );
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .attribute(icon, zgui::view::AttrName::new("data-icon"))
            .as_deref(),
        Some("chevron-up")
    );
}

#[test]
fn the_caller_s_classes_and_attributes_survive_the_component_s_own() {
    let harness = Harness::open();
    harness.mount(|| view! { Icon(icon = CHECK, class = "mine", attr:data-testid = "tick") });
    let icon = harness.only_child();
    let tree = harness.window.dom.tree();

    assert!(
        tree.classes(icon)
            .contains(&zgui::view::ClassName::new("mine")),
        "a class the caller passed was dropped"
    );
    assert!(
        tree.classes(icon)
            .contains(&zgui::view::ClassName::new("zui-icon")),
        "the component's own class was dropped by the caller's"
    );
    assert_eq!(
        tree.attribute(icon, zgui::view::AttrName::new("data-testid"))
            .as_deref(),
        Some("tick")
    );
}

#[test]
fn the_icon_sheet_is_installed_once_however_many_icons_are_drawn() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            row {
                Icon(icon = CHECK)
                Icon(icon = CROSS)
                Icon(icon = CHEVRON_DOWN, size = IconSize::Sm)
            }
        }
    });
    assert_eq!(
        harness.window.host.stylesheet_count(),
        1,
        "three icons installed three copies of one sheet"
    );
    let css = harness
        .window
        .host
        .stylesheet("zui-icon")
        .expect("the sheet is installed under its own name");
    assert!(
        css.contains("--zui-icon-size"),
        "the sheet is the icon's, and the size token is what it is for"
    );
}
