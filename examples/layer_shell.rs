//! A desktop shell in miniature: a bar across the top of the screen, and a pop-over it opens.
//!
//! Run it with `cargo run -p zgui-examples --example layer_shell` on a Wayland compositor that has
//! the layer shell: sway, Hyprland, niri, river or KDE Plasma. Elsewhere the windows are refused
//! and the program says so.
//!
//! What it is worth reading for:
//!
//! * `without_primary_window` starts the application with no window, and `with_context` opens the
//!   bar and the pop-over itself. Neither is the main window, so the exit policy is explicit;
//! * the bar is a shell window that reserves its 34 pixels, so ordinary windows open below it;
//! * the pop-over is opened hidden and shown on demand with `set_visible`, which costs one frame
//!   and keeps its document between openings;
//! * `use_input_region` makes the card the only part of the pop-over window that takes a press.
//!   The transparent margin around the card lets presses through to what is behind it;
//! * the pop-over takes the keyboard on demand, and closes when it loses the keyboard or when
//!   Escape is pressed. `on_focus_change` hears each edge;
//! * the menu button opens a pop-up window on the bar from the press itself. The press handler
//!   reads `input_serial` and gives it to the pop-up as its grab, so the release does not close
//!   the menu and a press outside it does.

use std::time::{Duration, Instant};

use zgui::geom::{CssPx, Edges, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::RenderEffect;

/// How long after the pop-over closes on a lost keyboard a press on the bar is ignored.
///
/// A press on the bar takes the keyboard from the pop-over, and the compositor says so before it
/// delivers the press. Without the wait, that press would open again the pop-over it just closed.
const REOPEN_GUARD: Duration = Duration::from_millis(150);

/// What the bar and the pop-over share.
#[derive(Clone, Copy)]
struct Shell {
    /// Whether the pop-over is open.
    open: RwSignal<bool>,
    /// When the pop-over last closed because it lost the keyboard.
    closed_at: RwSignal<Option<Instant>>,
}

impl Shell {
    /// Opens the pop-over, or closes it, unless it closed a moment ago.
    fn toggle(self) {
        let recent = self
            .closed_at
            .get_untracked()
            .is_some_and(|closed| closed.elapsed() < REOPEN_GUARD);
        if recent {
            self.closed_at.set(None);
            return;
        }
        self.open.update(|open| *open = !*open);
    }

    /// Closes the pop-over because it lost the keyboard.
    fn lost_focus(self) {
        if self.open.get_untracked() {
            self.open.set(false);
            self.closed_at.set(Some(Instant::now()));
        }
    }
}

/// The bar across the top of the screen.
#[component]
fn Bar() -> impl IntoView {
    let shell = expect_context::<Shell>();
    let label = move || {
        if shell.open.get() {
            "Close the pop-over"
        } else {
            "Open the pop-over"
        }
    };
    let button = NodeRef::new();
    view! {
        row(class = "bar") {
            control(class = "bar__button", on:click = move |_| shell.toggle()) {{label}}
            control(
                class = "bar__button",
                node_ref = button,
                on:pointer_down = move |_| open_menu(button)
            ) {"Menu"}
            label(class = "bar__title") {"zgui shell"}
        }
    }
}

/// Opens a menu below `button`, grabbing with the press that is being handled.
fn open_menu(button: NodeRef) {
    let bar = use_window();
    // The button's box is in device pixels, and a pop-up is placed in CSS pixels.
    let scale = bar.scale().get_untracked().max(f32::EPSILON);
    let anchor = button.window_bounds().map_or_else(
        || {
            Rect::new(
                Point::new(CssPx(0.0), CssPx(0.0)),
                Size::new(CssPx(1.0), CssPx(34.0)),
            )
        },
        |device| {
            Rect::new(
                Point::new(
                    CssPx(device.origin.x.0 / scale),
                    CssPx(device.origin.y.0 / scale),
                ),
                Size::new(
                    CssPx(device.size.width.0 / scale),
                    CssPx(device.size.height.0 / scale),
                ),
            )
        },
    );
    use_windows().open(
        WindowOptions::popup(
            &bar,
            PopupOptions::new(anchor)
                .with_constrain(Constrain::ANY)
                .with_grab(bar.input_serial())
                .with_size(180.0, 96.0),
        )
        .with_stylesheet(MENU_SHEET),
        || {
            view! {
                column(class = "menu") {
                    label(class = "menu__item") {"A menu on the bar"}
                    label(class = "menu__item") {"Press outside to close it"}
                }
            }
        },
    );
}

/// The pop-over below the bar: a card in a transparent window.
#[component]
fn Popover() -> impl IntoView {
    let shell = expect_context::<Shell>();
    let card = NodeRef::new();
    // Only the card takes a press. The window is larger than the card so that its shadow has room.
    use_input_region([card]);

    // A pop-over closes when the user moves on to something else.
    let window = use_window();
    let focus = window.on_focus_change(move |focused| {
        if !focused {
            shell.lost_focus();
        }
    });
    on_cleanup_local(move || drop(focus));

    // Nothing in the card takes focus, so Escape is heard by the window as a whole.
    let shortcut = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        card.get();
        card.window_shortcut()
    });
    on_cleanup_local(move || drop(shortcut));

    view! {
        column(class = "popover") {
            column(
                class = "card",
                node_ref = card,
                on:key_down = move |ev| {
                    if ev.key == Key::Named(NamedKey::Escape) {
                        shell.open.set(false);
                    }
                }
            ) {
                label(class = "card__title") {"A pop-over"}
                label(class = "card__hint") {"Press Escape, or press anywhere else, to close it."}
            }
        }
    }
}

/// Opens the bar and the pop-over, and keeps the pop-over shown while it is open.
fn open_shell() {
    let shell = Shell {
        open: RwSignal::new(false),
        closed_at: RwSignal::new(None),
    };
    provide_context(shell);

    let windows = use_windows();
    windows.open(
        WindowOptions::layer(
            LayerOptions::top()
                .with_anchors(Edges::new(true, true, false, true))
                .with_size(0.0, 34.0)
                .with_exclusive_zone(ExclusiveZone::Reserve(CssPx(34.0)))
                .with_namespace("bar"),
        )
        .with_stylesheet(BAR_SHEET),
        || view! { Bar() },
    );
    let popover = windows.open(
        WindowOptions::layer(
            LayerOptions::top()
                .with_anchors(Edges::new(true, false, false, true))
                .with_margin(4.0, 0.0, 0.0, 4.0)
                .with_keyboard(KeyboardInteractivity::OnDemand)
                .with_size(360.0, 240.0)
                .with_namespace("popover"),
        )
        .with_transparent(true)
        .with_visible(false)
        .with_stylesheet(POPOVER_SHEET),
        || view! { Popover() },
    );
    // Above the windows, so it lives as long as the application does.
    core::mem::forget(RenderEffect::new(move |_| {
        popover.set_visible(shell.open.get());
    }));
}

/// What the bar looks like.
const BAR_SHEET: &str = css!(
    ":root { background-color: #12141a; color: #e8ecf4; font-family: sans-serif }

    .bar {
        height: 34px;
        align-items: center;
        gap: 12px;
        padding: 0 8px;
    }

    .bar__button {
        padding: 4px 10px;
        border-radius: 6px;
        background-color: #232936;
        font-size: 13px;
    }

    .bar__button:hover { background-color: #2b3243 }

    .bar__title { font-size: 13px; color: #7d879b }"
);

/// What the menu looks like.
const MENU_SHEET: &str = css!(
    ":root { background-color: #171a22; color: #e8ecf4; font-family: sans-serif }

    .menu { gap: 4px; padding: 8px; border: 1px solid #2f3646 }

    .menu__item { padding: 6px 8px; font-size: 13px }"
);

/// What the pop-over looks like. The window is transparent, and only the card is painted.
const POPOVER_SHEET: &str = css!(
    ":root { background-color: transparent; color: #e8ecf4; font-family: sans-serif }

    .popover { padding: 12px }

    .card {
        width: 320px;
        height: 200px;
        gap: 8px;
        padding: 16px;
        border-radius: 12px;
        border: 1px solid #2f3646;
        background-color: #171a22;
        box-shadow: 0 6px 16px rgba(0, 0, 0, 0.45);
    }

    .card__title { font-size: 16px; font-weight: 700 }

    .card__hint { font-size: 13px; color: #7d879b }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.LayerShell")
        .without_primary_window()
        .with_exit_policy(ExitPolicy::Explicit)
        .with_context(open_shell)
        .run(|| ())
}
