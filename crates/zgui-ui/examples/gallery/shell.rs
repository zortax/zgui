//! The chrome the gallery is laid out in: a masthead, a scheme switch and a panel per subject.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

/// One subject of the gallery, with a heading and a note above whatever it is showing.
///
/// Everything inside a panel is laid out down the page with a gap between the rows, so a section
/// only has to say what it is showing rather than how far apart to put it.
#[component]
pub(crate) fn Panel(
    /// What the panel is called.
    #[prop(into)]
    title: String,
    /// One line about what it is showing.
    #[prop(into)]
    note: String,
    /// Whether the panel takes the full width of the page rather than one column of the grid.
    #[prop(default = false)]
    wide: bool,
    /// What it is showing.
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "panel", class:panel-wide = move || wide) {
            text(class = "panel__title") {{title}}
            text(class = "panel__note") {{note}}
            column(class = "panel__body") {{children.into_view_once()}}
        }
    }
}

/// A labelled row inside a panel, for putting one variant beside its name.
#[component]
pub(crate) fn Row(
    /// What this row of examples is.
    #[prop(into)]
    label: String,
    /// The examples.
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "row") {
            text(class = "row__label") {{label}}
            row(class = "row__items") {{children.into_view_once()}}
        }
    }
}

/// A chooser for one of the [`ThemeProvider`]'s two slots.
///
/// The whole of what it does is write a [`Preset`] into a signal. That signal is one of the
/// provider's slots, and the provider re-declares its custom properties when it changes — so
/// picking "Ember" here re-colours, re-rounds and re-times every component in the window without a
/// single one of them being told, and without the other slot moving.
#[component]
pub(crate) fn ThemeChooser(
    /// What the slot is called, for a reader.
    #[prop(into)]
    label: String,
    /// Which preset is in it.
    preset: RwSignal<Preset, LocalStorage>,
    /// What the control is marked with, for a script driving the gallery.
    testid: &'static str,
) -> impl IntoView {
    let name = NodeRef::new();
    let chosen = Binding::controlled(
        Signal::derive_local(move || preset.get().name().to_owned()),
        move |value: String| {
            if let Some(picked) = Preset::from_name(&value) {
                preset.set(picked);
            }
        },
    );

    view! {
        row(class = "masthead__slot") {
            Label(node_ref = name) {{label}}
            Select(value = chosen) {
                SelectTrigger(
                    size = SelectTriggerSize::Sm,
                    labelled_by = name,
                    attr:data-testid = testid
                ) {
                    SelectValue()
                }
                SelectContent {
                    {Preset::ALL
                        .iter()
                        .copied()
                        .map(|preset| {
                            view! { SelectItem(value = preset.name()) {{preset.label()}} }
                                .into_view()
                        })
                        .collect::<Vec<_>>()}
                }
            }
        }
    }
}

/// The bar across the top: what this is, the two theme choosers, and the scheme switch.
///
/// The switch writes to the same signal the [`ThemeProvider`] reads, so flipping it replaces the
/// custom properties every component's style sheet resolves against. Nothing in the gallery
/// branches on the scheme in Rust.
#[component]
pub(crate) fn Masthead(
    /// Which scheme the gallery is presenting in.
    scheme: RwSignal<ColorScheme, LocalStorage>,
    /// Which theme is in the provider's light slot.
    light: RwSignal<Preset, LocalStorage>,
    /// Which theme is in its dark slot.
    dark_theme: RwSignal<Preset, LocalStorage>,
) -> impl IntoView {
    let name = NodeRef::new();
    let control = NodeRef::new();
    // The scheme is not a boolean, so the switch reads one out of it and writes one back in.
    let dark = Binding::controlled(
        Signal::derive_local(move || scheme.get() == ColorScheme::Dark),
        move |on: bool| {
            scheme.set(if on {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            })
        },
    );

    view! {
        row(class = "masthead") {
            column(class = "masthead__text") {
                text(class = "masthead__title") {"zgui components"}
                text(class = "masthead__note") {
                    "every component in the library, in a real window"
                }
            }
            spacer()
            ThemeChooser(label = "Light theme", preset = light, testid = "light-theme")
            ThemeChooser(label = "Dark theme", preset = dark_theme, testid = "dark-theme")
            Badge(variant = BadgeVariant::Secondary, attr:data-testid = "scheme-badge") {
                {move || scheme.get().name().to_owned()}
            }
            Label(node_ref = name, control = control) {"Dark"}
            Switch(
                node_ref = control,
                checked = dark,
                labelled_by = name,
                attr:data-testid = "scheme-switch"
            )
        }
    }
}

/// How the gallery itself is laid out. Every value in it is a design token, so the page follows
/// the theme the same way the components do.
pub(crate) const SHEET: &str = zgui::css!(
    ":root {
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-family: sans-serif;
        font-size: var(--zui-type-size-md);
        overflow: auto;
    }

    .page { gap: var(--zui-space-xl); padding: var(--zui-space-xl); }

    .masthead { align-items: center; gap: var(--zui-space-lg); flex-wrap: wrap; }
    .masthead__slot { align-items: center; gap: var(--zui-space-sm); }
    .masthead__text { gap: var(--zui-space-xs); }
    .masthead__title { font-size: var(--zui-type-size-2xl); font-weight: 700; }
    .masthead__note { font-size: var(--zui-type-size-sm); color: var(--zui-color-muted-foreground); }

    .grid {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        gap: var(--zui-space-lg);
        align-items: start;
    }

    .panel {
        gap: var(--zui-space-xs);
        padding: var(--zui-space-lg);
        border-radius: var(--zui-radius-lg);
        border: 1px solid var(--zui-color-border);
        background-color: var(--zui-color-card);
        color: var(--zui-color-card-foreground);
    }
    .panel-wide { grid-column: 1 / -1; }
    .panel__title { font-size: var(--zui-type-size-lg); font-weight: 600; }
    .panel__note { font-size: var(--zui-type-size-xs); color: var(--zui-color-muted-foreground); }
    .panel__body { gap: var(--zui-space-md); padding-top: var(--zui-space-md); }

    .row { gap: var(--zui-space-xs); }
    .row__label {
        font-size: var(--zui-type-size-xs);
        color: var(--zui-color-muted-foreground);
    }
    .row__items { gap: var(--zui-space-sm); align-items: center; flex-wrap: wrap; }

    .stack { gap: var(--zui-space-sm); }
    .field { gap: var(--zui-space-xs); }
    .pair { gap: var(--zui-space-sm); align-items: center; }
    .frame {
        border: 1px dashed var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        padding: var(--zui-space-sm);
    }
    .tall { height: 160px; }
    .wide { width: 100%; }
    /* Air for the carousel's arrows, which hang 48px outside the strip on either side. */
    .carousel-frame { padding: 0 48px; }
    /* A room for the sidebar to stand in: tall enough that its groups, footer and inset are all
       visible at once, and clipped at its rounded border so the panel slides behind an edge
       rather than over the page. */
    .sidebar-frame {
        height: 380px;
        width: 100%;
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-lg);
        overflow: hidden;
    }
    .sidebar-inset-body { padding: var(--zui-space-lg); gap: var(--zui-space-sm); }
    .sidebar-inset-title { font-size: var(--zui-type-size-lg); font-weight: 600; }
    .sidebar-inset-note {
        font-size: var(--zui-type-size-sm);
        color: var(--zui-color-muted-foreground);
    }
    /* Something to see inside the aspect-ratio box, which has no look of its own. */
    .ratio-fill {
        width: 100%;
        height: 100%;
        align-items: center;
        justify-content: center;
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-muted-foreground);
        font-size: var(--zui-type-size-sm);
    }

    /* The three ways a run leaves the glyph atlas and is drawn as filled curves instead: a
       transform that is not a translation, a size no cache should hold, and a brush that is not
       one colour. */
    .turned-frame { height: 90px; width: 220px; align-items: center; justify-content: center; }
    .turned-text {
        display: inline-block;
        transform: rotate(-20deg);
        font-size: var(--zui-type-size-2xl);
        font-weight: 700;
    }
    .display-text { display: inline-block; font-size: 120px; font-weight: 700; line-height: 1.1; }
    .gradient-text {
        display: inline-block;
        --zgui-text-fill: background;
        background-image: linear-gradient(90deg, #f43f5e, #6366f1);
        font-size: 56px;
        font-weight: 700;
    }"
);
