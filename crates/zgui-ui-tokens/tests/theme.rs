//! A theme provider driven through real frames.
//!
//! Every case here mounts the real component into a window, runs frames, and asks the host what
//! actually reached it. Nothing is hand-built: if the provider stopped installing a sheet, stopped
//! replacing it, or started installing one per instance, these fail.

use zgui::prelude::*;
use zgui::reactive::{Patch, RwSignal, Store};
use zgui::view;
use zgui_testkit_view::Window;
use zgui_ui_tokens::prelude::*;

/// Builds `view` into `window` and mounts it, returning what has to be held to keep it there.
fn mount<V: IntoView + 'static>(window: &Window, view: impl FnOnce() -> V) -> Box<dyn Anchor> {
    let mut built = window
        .scope
        .with(|| view().into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom_handle, window.root, None);
    window.frame();
    Box::new(built)
}

#[test]
fn mounting_a_provider_installs_one_root_sheet_carrying_every_token() {
    let window = Window::open();
    let _view = mount(&window, || {
        view! { ThemeProvider {box(class = "page")} }
    });

    assert_eq!(window.host.stylesheet_names(), [THEME_SHEET]);
    let css = window
        .host
        .stylesheet(THEME_SHEET)
        .expect("the provider installed it");
    assert!(css.starts_with(":root {"), "{css}");
    for property in Theme::properties() {
        assert!(css.contains(property), "{property} never reached the sheet");
    }
}

#[test]
fn a_second_instance_of_a_component_does_not_install_a_second_sheet() {
    // The property that makes a component library affordable: a hundred cards install one sheet.
    let window = Window::open();
    let _view = mount(&window, || {
        view! {
            box {
                ThemeProvider {box(class = "a")}
            }
        }
    });
    let after_first = window.host.stylesheet_installs();

    // Re-running the same install, exactly as a second instance's body would.
    window.scope.with(|| {
        let css = window
            .host
            .stylesheet(THEME_SHEET)
            .expect("the first one installed it");
        install_stylesheet(THEME_SHEET, &css);
    });

    assert_eq!(window.host.stylesheet_installs(), after_first);
    assert_eq!(window.host.stylesheet_count(), 1);
}

#[test]
fn changing_the_scheme_replaces_the_one_sheet_rather_than_adding_another() {
    let window = Window::open();
    let scheme = window
        .scope
        .with(|| RwSignal::new_local(ColorScheme::Light));
    let _view = mount(&window, move || {
        view! { ThemeProvider(scheme = Signal::from(scheme)) {box()} }
    });

    let light = window.host.stylesheet(THEME_SHEET).expect("installed");
    assert!(!light.contains("@media"), "a pinned scheme writes no query");
    let installs = window.host.stylesheet_installs();

    scheme.set(ColorScheme::System);
    window.frame();

    let system = window
        .host
        .stylesheet(THEME_SHEET)
        .expect("still installed");
    assert!(system.contains("@media (prefers-color-scheme: dark)"));
    assert_eq!(
        window.host.stylesheet_count(),
        1,
        "the sheet was replaced, so it keeps its place in the cascade"
    );
    assert_eq!(
        window.host.stylesheet_installs(),
        installs + 1,
        "exactly one replacement"
    );
}

/// Captures the theme the enclosing provider published, so a test can drive it.
#[zgui::component]
fn Probe(
    /// Where to put what it found.
    captured: RwSignal<Option<ThemeContext>, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    captured.set(use_theme());
    view! { box() }
}

#[test]
fn patching_a_theme_replaces_the_sheet_once_and_writes_the_new_token() {
    let window = Window::open();
    let captured = window.scope.with(|| RwSignal::new_local(None));
    let _view = mount(&window, move || {
        view! {
            ThemeProvider(scheme = ColorScheme::Light) {
                Probe(captured = captured)
            }
        }
    });

    let context = captured
        .get_untracked()
        .expect("the provider published one");
    assert_eq!(context.selector(), ":root");
    let before = window.host.stylesheet(THEME_SHEET).expect("installed");
    assert!(
        before.contains("--zui-scale-accent-9: #0090ff;"),
        "{before}"
    );
    let installs = window.host.stylesheet_installs();

    let mut brighter = Themes::defaults();
    brighter.light.scale.accent_9 = "rebeccapurple".to_owned();
    context.themes().patch(brighter);
    window.frame();

    let after = window
        .host
        .stylesheet(THEME_SHEET)
        .expect("still installed");
    assert!(
        after.contains("--zui-scale-accent-9: rebeccapurple;"),
        "{after}"
    );
    assert_eq!(window.host.stylesheet_count(), 1);
    assert_eq!(
        window.host.stylesheet_installs(),
        installs + 1,
        "one replacement, not a removal and an addition"
    );
}

#[test]
fn patching_a_theme_onto_itself_never_reaches_the_document() {
    let window = Window::open();
    let captured = window.scope.with(|| RwSignal::new_local(None));
    let _view = mount(&window, move || {
        view! {
            ThemeProvider(scheme = ColorScheme::Light) {
                Probe(captured = captured)
            }
        }
    });

    let context = captured
        .get_untracked()
        .expect("the provider published one");
    let installs = window.host.stylesheet_installs();
    assert!(installs > 0, "the first install has to have happened");

    context.themes().patch(Themes::defaults());
    window.frame();

    assert_eq!(
        window.host.stylesheet_installs(),
        installs,
        "a patch that changed nothing must not replace the sheet"
    );
}

#[test]
fn a_nested_provider_themes_a_region_and_leaves_the_root_sheet_alone() {
    let window = Window::open();
    let _view = mount(&window, || {
        view! {
            ThemeProvider {
                box(class = "page") {
                    ThemeProvider(light = Theme::dark(), dark = Theme::dark()) {
                        box(class = "panel")
                    }
                }
            }
        }
    });

    let names = window.host.stylesheet_names();
    assert_eq!(names.len(), 2, "{names:?}");
    let root = window.host.stylesheet(&names[0]).expect("the outer sheet");
    let region = window.host.stylesheet(&names[1]).expect("the inner sheet");
    assert!(root.starts_with(":root {"));
    assert!(
        region.starts_with(".zui-theme-1 {"),
        "a nested provider declares on its own class, not on the root: {region}"
    );
    assert!(!region.contains(":root"));
}

#[test]
fn unmounting_a_provider_takes_its_sheet_with_it() {
    let window = Window::open();
    let showing = window.scope.with(|| RwSignal::new_local(true));
    let _view = mount(&window, move || {
        view! {
            if move || showing.get() {
                ThemeProvider {box()}
            } else {}
        }
    });
    assert_eq!(window.host.stylesheet_count(), 1);

    showing.set(false);
    window.frame();

    assert_eq!(
        window.host.stylesheet_count(),
        0,
        "the sheet outlived the provider that installed it"
    );
}

#[test]
fn reading_one_token_group_does_not_wake_a_reader_of_another() {
    // The whole reason the tokens are a store rather than a signal. Written against the store
    // directly, because it is the store's promise rather than the provider's.
    use std::cell::Cell;
    use std::rc::Rc;

    zgui::reactive::install().ok();
    let scope = zgui::reactive::Mounted::new();
    let (motion_runs, color_runs) = scope.with(|| {
        let themes = Store::new(Themes::defaults());
        let motion_runs = Rc::new(Cell::new(0));
        let color_runs = Rc::new(Cell::new(0));

        let counted = Rc::clone(&motion_runs);
        let motion = zgui::reactive::RenderEffect::new(move |_| {
            let _ = themes.light().motion().get();
            counted.set(counted.get() + 1);
        });
        let counted = Rc::clone(&color_runs);
        let color = zgui::reactive::RenderEffect::new(move |_| {
            let _ = themes.light().color().get();
            counted.set(counted.get() + 1);
        });

        assert_eq!(motion_runs.get(), 1);
        assert_eq!(color_runs.get(), 1);

        let mut changed = Themes::defaults();
        changed.light.color.primary = "rebeccapurple".to_owned();
        themes.patch(changed);
        zgui::reactive::flush();

        // Held until after the flush; dropping an effect's handle stops it.
        drop((motion, color));
        (motion_runs.get(), color_runs.get())
    });
    scope.unmount();

    assert_eq!(color_runs, 2, "the colour group's reader re-ran");
    assert_eq!(motion_runs, 1, "the motion group's reader did not");
}
