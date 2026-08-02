//! Whether a window that has opened and closed a surface still answers a press and a key — asked
//! of the pixels on a real screen, after every cycle, for as many cycles as it takes.
//!
//! The defect this exists to disprove is a modal that occasionally refuses to unmount: its scrim
//! stays over the page and its focus trap stays around a subtree nobody can see, so the window
//! answers nothing for the rest of the session. It appeared about once in fifteen cycles, only on a
//! real compositor, and only at a refresh rate high enough that an exit animation's last frame, the
//! flush that rebuilds the content and the deferred check a presence schedules can fall in an order
//! no headless clock produces.
//!
//! So this drives the real [`Dialog`], [`AlertDialog`], [`Sheet`], [`Drawer`] and a surface nested
//! inside another through hundreds of cycles each, varying deliberately when the close arrives —
//! inside the enter animation, immediately after opening, twice in quick succession, and with a
//! second surface opened on top — and after every single one asks the same two questions:
//!
//! * **does a press still reach an ordinary control?**
//! * **does a key still reach an ordinary text field?**
//!
//! Both are answered from the screen. Two witness swatches span the top of the window; the first
//! takes its colour from how many times the probe control has been pressed, the second from how
//! many characters the field holds. Every check screenshots the window through the compositor and
//! reads the two swatches back. A count kept in a signal would say the handler ran; only the pixels
//! say the window is still showing what the handler did.
//!
//! ```text
//! ZMV_GEOM=<x,y,w,h> modal-verify <app-id> <family> <cycles> [seed] > report.tsv 2> trace.log
//! ```
//!
//! A cycle that fails does not end the run: the count of failures over hundreds of cycles is the
//! measurement, and a run that stops at the first one cannot produce it. Every cycle writes one
//! line to standard output — its number, its shape, and what the two witnesses said — so a run can
//! be counted without being watched.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    reason = "the gallery's own source is included whole, as the document this window's frames are \
              the cost of, rather than as the few controls a driver presses"
)]

#[path = "../../../zgui-ui/examples/gallery/app.rs"]
mod app;
#[path = "../../../zgui-ui/examples/gallery/section/mod.rs"]
#[allow(
    unused_imports,
    reason = "the gallery's sections are one module; the ladder below mounts the ones it is sized by"
)]
mod section;
#[path = "../../../zgui-ui/examples/gallery/shell.rs"]
mod shell;

#[path = "../shot.rs"]
mod shot;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zgui::geom::{Css, CssPx, Point};
use zgui::platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view::NodeRef;
use zgui::vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerEvent,
};
use zgui::{component, css, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::app::GalleryProps;
use crate::shot::{At, Shot, Witness};

/// Writes one driver line on the same clock the framework's trace lines carry.
fn say(fields: &str) {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    eprintln!("ZMT {at} {fields}");
}

/// Where each thing the script presses is, and what the two witnesses have counted.
#[derive(Default, Clone, Copy)]
struct Aim {
    /// The centre of the drawer's trigger, in CSS pixels.
    drawer: Option<(f32, f32)>,
    /// The centre of the dialog's trigger.
    dialog: Option<(f32, f32)>,
    /// The centre of the alert dialog's trigger.
    alert: Option<(f32, f32)>,
    /// The centre of the sheet's trigger.
    sheet: Option<(f32, f32)>,
    /// The centre of the probe control.
    probe: Option<(f32, f32)>,
    /// The centre of the ordinary text field.
    field: Option<(f32, f32)>,
    /// The centre of the click witness swatch, in device pixels from the window's own origin.
    click_witness: Option<(f32, f32)>,
    /// The centre of the key witness swatch, in the same coordinates.
    key_witness: Option<(f32, f32)>,
    /// How many presses the probe has counted.
    presses: u64,
    /// How many characters the field holds.
    typed: u64,
}

/// What the window and the driver share.
type Shared = Arc<Mutex<Aim>>;

/// What the last turn of the loop decided, for a thread that is still awake when it is not.
///
/// A loop parked on nothing runs no callbacks, so nothing on its own thread can report that it has
/// stopped. This is written on every turn and read from a thread of its own.
#[derive(Default)]
struct Turn {
    /// What the last turn asked to park on.
    decided: String,
    /// How many turns have happened.
    turns: u64,
    /// How many deadline arrivals have been reported.
    woken: u64,
}

/// The watchdog: says what the loop was last doing, once it has stopped doing anything.
fn watch(turn: Arc<Mutex<Turn>>) {
    std::thread::spawn(move || {
        let mut last = 0;
        let mut quiet = 0;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Ok(held) = turn.lock() else { return };
            if held.turns == last {
                quiet += 1;
                if quiet % 5 == 0 {
                    say(&format!(
                        "WATCHDOG quiet={quiet}s turns={} woken={} decided={}",
                        held.turns, held.woken, held.decided
                    ));
                }
            } else {
                quiet = 0;
                last = held.turns;
            }
        }
    });
}

/// The window: the real surfaces, and two ordinary controls behind them whose state is on screen.
#[component]
fn Rig(
    /// Where the positions and the counts are published.
    shared: Shared,
) -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Light);
    let currency = RwSignal::new_local("gbp".to_owned());
    let text = RwSignal::new_local(String::new());
    let drawer = NodeRef::new();
    let dialog = NodeRef::new();
    let alert = NodeRef::new();
    let sheet = NodeRef::new();
    let probe = NodeRef::new();
    let field = NodeRef::new();
    let click_witness = NodeRef::new();
    let key_witness = NodeRef::new();
    let presses = RwSignal::new_local(0_u64);

    let published = Arc::clone(&shared);
    let publish = move || {
        let scale = probe.scale().max(0.001);
        // In device pixels, because that is what a capture of the window is measured in.
        let device = |handle: NodeRef| {
            handle.window_bounds().map(|box_| {
                (
                    box_.origin.x.0 + box_.size.width.0 / 2.0,
                    box_.origin.y.0 + box_.size.height.0 / 2.0,
                )
            })
        };
        let centre = |handle: NodeRef| device(handle).map(|(x, y)| (x / scale, y / scale));
        if let Ok(mut held) = published.lock() {
            held.click_witness = device(click_witness);
            held.key_witness = device(key_witness);
            held.drawer = centre(drawer);
            held.dialog = centre(dialog);
            held.alert = centre(alert);
            held.sheet = centre(sheet);
            held.probe = centre(probe);
            held.field = centre(field);
            held.presses = presses.get_untracked();
            held.typed = text.with_untracked(|held| held.chars().count() as u64);
        }
    };
    let polling = set_interval(Duration::from_millis(16), publish);
    on_cleanup_local(move || drop(polling));

    // The two witnesses. Four classes each, one per residue, because what has to reach the screen
    // is a change the cascade produced rather than a colour a driver wrote into a style attribute.
    let click_at = move |residue: u64| move || presses.get() % 4 == residue;
    let key_at =
        move |residue: u64| move || text.with(|held| held.chars().count() as u64 % 4) == residue;

    view! {
        ThemeProvider(scheme = scheme) {
            column(class = "rig") {
                row(class = "witness") {
                    box(
                        node_ref = click_witness,
                        class = "w",
                        class:c0 = {click_at(0)}, class:c1 = {click_at(1)},
                        class:c2 = {click_at(2)}, class:c3 = {click_at(3)}
                    )
                    box(
                        node_ref = key_witness,
                        class = "w",
                        class:k0 = {key_at(0)}, class:k1 = {key_at(1)},
                        class:k2 = {key_at(2)}, class:k3 = {key_at(3)}
                    )
                }
                row(class = "rig-row") {
                    box(node_ref = drawer, class = "slot") {
                        Drawer {
                            DrawerTrigger(variant = ButtonVariant::Outline) {"Share"}
                            DrawerContent {
                                DrawerHandle()
                                DrawerHeader {
                                    DrawerTitle {"Share this invoice"}
                                    DrawerDescription {
                                        "Anyone with the link can view it."
                                    }
                                }
                                DrawerFooter {DrawerClose {"Done"}}
                            }
                        }
                    }
                    box(node_ref = dialog, class = "slot") {
                        Dialog {
                            DialogTrigger {"Rename…"}
                            DialogContent {
                                DialogHeader {
                                    DialogTitle {"Rename project"}
                                    DialogDescription {
                                        "Everyone on the team will see the new name."
                                    }
                                }
                                Input(placeholder = "Project name", label = "Project name")
                                Select(value = currency) {
                                    SelectTrigger(a11y:label = "Currency") {
                                        SelectValue(placeholder = "Choose one")
                                    }
                                    SelectContent {
                                        SelectItem(value = "gbp") {"Pound sterling"}
                                        SelectItem(value = "eur") {"Euro"}
                                        SelectItem(value = "usd") {"US dollar"}
                                    }
                                }
                                DialogFooter {
                                    DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
                                    Button {"Rename"}
                                }
                            }
                        }
                    }
                    box(node_ref = alert, class = "slot") {
                        AlertDialog {
                            AlertDialogTrigger {"Delete…"}
                            AlertDialogContent {
                                AlertDialogHeader {
                                    AlertDialogTitle {"Delete this project?"}
                                    AlertDialogDescription {
                                        "This cannot be undone."
                                    }
                                }
                                AlertDialogFooter {
                                    AlertDialogCancel {"Keep it"}
                                    AlertDialogAction {"Delete"}
                                }
                            }
                        }
                    }
                    box(node_ref = sheet, class = "slot") {
                        Sheet {
                            SheetTrigger(variant = ButtonVariant::Outline) {"Filters"}
                            SheetContent(side = SheetSide::Right) {
                                SheetHeader {
                                    SheetTitle {"Filters"}
                                    SheetDescription {"Narrow the list down."}
                                }
                                SheetFooter {SheetClose {"Apply"}}
                            }
                        }
                    }
                }
                row(class = "rig-row") {
                    control(
                        node_ref = probe,
                        class = "probe",
                        tabindex = {Focus::Sequential},
                        a11y:role = {Role::Button},
                        a11y:label = "Probe",
                        on:click = move |_| presses.set(presses.get_untracked() + 1)
                    ) {
                        "probe"
                    }
                    box(node_ref = field, class = "field") {
                        Input(value = text, placeholder = "Notes", label = "Notes")
                    }
                }
                // The shipped gallery, so that a frame of this window costs what a frame of the
                // window the defect was seen in costs.
                Gallery()
            }
        }
    }
}

/// What the driver's own controls look like, over the gallery's own sheet.
///
/// The witness colours are the corners of the cube on purpose: a swatch read back through a
/// compositor, a scale factor and a PNG encoder is still unmistakably one of eight.
const RIG_SHEET: &str = css!(
    ".rig { display: block; padding: 0; gap: 20px; width: 100% }
     .witness { gap: 0; width: 100%; height: 120px }
     .w { display: block; height: 120px; flex-grow: 1; background-color: #808080 }
     .c0 { background-color: #ff0000 } .c1 { background-color: #00ff00 }
     .c2 { background-color: #0000ff } .c3 { background-color: #ffff00 }
     .k0 { background-color: #00ffff } .k1 { background-color: #ff00ff }
     .k2 { background-color: #ffffff } .k3 { background-color: #000000 }
     .rig-row { gap: 16px; padding-left: 32px; padding-right: 32px }
     .slot { display: block }
     .field { display: block; width: 300px }
     .probe { display: block; width: 240px; height: 56px; background-color: #3b4252;
              color: #eceff4; padding: 16px }"
);

/// A dismissal whose animation never finishes, for the run that asks what happens then.
///
/// Everything about unmounting a surface waits on an animation end or a transition end, and the
/// question a safety net exists to answer is what the window does when one never arrives. Half a
/// minute is not "never", but it is longer than any run here and far longer than the second the
/// library gives a dismissal before it finishes it regardless — so a window that recovers under
/// this sheet is one that recovered without the event.
///
/// Two classes deep so that it outranks the library's own rule for the surface.
const BROKEN_SHEET: &str = css!(
    ".zui-overlay-scope .zui-surface { transition-duration: 30s; animation-duration: 30s }
     .zui-overlay-layer .zui-overlay-scrim { transition-duration: 30s; animation-duration: 30s }"
);

/// Which surface a cycle drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    /// The drawer.
    Drawer,
    /// The dialog.
    Dialog,
    /// The alert dialog.
    Alert,
    /// The sheet.
    Sheet,
    /// A surface opened inside another surface.
    Nested,
}

impl Family {
    /// The name a command line gives it.
    fn parse(name: &str) -> Option<Self> {
        match name {
            "drawer" => Some(Self::Drawer),
            "dialog" => Some(Self::Dialog),
            "alert" => Some(Self::Alert),
            "sheet" => Some(Self::Sheet),
            "nested" => Some(Self::Nested),
            _ => None,
        }
    }

    /// How it reads in the report.
    const fn name(self) -> &'static str {
        match self {
            Self::Drawer => "drawer",
            Self::Dialog => "dialog",
            Self::Alert => "alert",
            Self::Sheet => "sheet",
            Self::Nested => "nested",
        }
    }
}

/// One thing the script does.
#[derive(Clone, Copy, Debug)]
enum Act {
    /// Press and release over the trigger of the family being driven.
    ClickTrigger,
    /// Press and release over the dialog's trigger, whatever family is running.
    ClickDialog,
    /// Move focus on by one, which is how a surface inside a surface is reached.
    Tab,
    /// Enter down and up, which opens whatever is focused.
    Enter,
    /// Press and release well away from anything, which lands on the scrim while one is up.
    ClickAway,
    /// Escape down and up.
    Escape,
    /// Several pointer moves across the window, which start hover transitions of their own.
    Jiggle,
    /// Remember what the two witnesses say, before testing them.
    Mark,
    /// Press the probe control.
    PressProbe,
    /// Read the click witness back off the screen.
    CheckClick,
    /// Press the text field, which is how the keyboard gets there.
    PressField,
    /// Type one character.
    TypeChar,
    /// Read the key witness back off the screen.
    CheckKey,
    /// Write a marker into the trace.
    Named(&'static str),
}

/// One step: what to do, and how long to wait before doing it.
#[derive(Clone, Copy, Debug)]
struct Step {
    /// How long after the previous step this one runs.
    after: Duration,
    /// What it does.
    act: Act,
}

impl Step {
    /// A step `millis` after the previous one.
    const fn new(millis: u64, act: Act) -> Self {
        Self {
            after: Duration::from_millis(millis),
            act,
        }
    }
}

/// A small deterministic generator, so a run that catches something can be run again.
struct Rng(u64);

impl Rng {
    /// The next number below `bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.0 >> 33) % bound
    }
}

/// How many times a witness is asked before the window is called deaf.
///
/// More than one, because the first press is entitled to be swallowed: a surface that is still
/// fading out is still a surface, and a press on it is a dismissal rather than a miss.
const TRIES: u32 = 4;

/// How long a surface is left up before it is dismissed, in milliseconds.
///
/// The shortest of these dismisses a surface whose *enter* animation is still running and the
/// longest one that has been still for a while, because the defect is a race between those ends.
const HOLDS: [u64; 8] = [0, 2, 6, 12, 24, 48, 120, 260];

/// The steps of one cycle of `family`, chosen by its number so the mixture repeats predictably.
fn cycle(family: Family, number: u64, rng: &mut Rng) -> (&'static str, Vec<Step>) {
    let hold = HOLDS[(rng.below(HOLDS.len() as u64)) as usize];
    let gap = rng.below(40) + 4;
    let (kind, mut steps) = if family == Family::Nested {
        // A surface inside a surface, reached by keyboard: a modal surface is centred with a
        // transform and the box a handle reports is the untransformed one, so a press aimed at it
        // lands on the scrim and dismisses the outer surface instead of opening anything inside.
        match number % 4 {
            0 => (
                "nested-plain",
                vec![
                    Step::new(0, Act::ClickDialog),
                    Step::new(300, Act::Tab),
                    Step::new(40, Act::Enter),
                    Step::new(hold + 120, Act::Escape),
                    Step::new(gap + 200, Act::Escape),
                ],
            ),
            1 => (
                "nested-fast",
                vec![
                    Step::new(0, Act::ClickDialog),
                    Step::new(300, Act::Tab),
                    Step::new(40, Act::Enter),
                    Step::new(hold + 120, Act::Escape),
                    Step::new(0, Act::Escape),
                    Step::new(gap, Act::Escape),
                ],
            ),
            2 => (
                "nested-busy",
                vec![
                    Step::new(0, Act::ClickDialog),
                    Step::new(300, Act::Tab),
                    Step::new(40, Act::Enter),
                    Step::new(hold + 120, Act::Escape),
                    Step::new(1, Act::Jiggle),
                    Step::new(gap, Act::Escape),
                    Step::new(1, Act::Escape),
                ],
            ),
            _ => (
                "nested-away",
                vec![
                    Step::new(0, Act::ClickDialog),
                    Step::new(300, Act::Tab),
                    Step::new(40, Act::Enter),
                    Step::new(hold + 120, Act::Escape),
                    Step::new(gap + 200, Act::ClickAway),
                ],
            ),
        }
    } else {
        match number % 6 {
            // The plain case, at every hold: open, wait, Escape.
            0 => (
                "plain",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                ],
            ),
            // Two Escapes in quick succession, the second inside the exit animation.
            1 => (
                "double-escape",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                    Step::new(gap, Act::Escape),
                ],
            ),
            // Re-opened while it is still leaving, then closed again.
            2 => (
                "reopen",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                    Step::new(gap, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                ],
            ),
            // Dismissed by a press on the scrim rather than by a key.
            //
            // An alert dialog is the one surface that does not answer an outside press — a
            // destructive choice must not be dismissed by a stray click — so for that one the
            // press is delivered anyway, and the surface has to survive it and still close on
            // the key that follows.
            3 if family == Family::Alert => (
                "outside-refused",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold + 90, Act::ClickAway),
                    Step::new(60, Act::Escape),
                ],
            ),
            3 => (
                "outside",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold + 90, Act::ClickAway),
                ],
            ),
            // Three openings and three closings with nothing between them, which is the shortest
            // path to a close whose exit is still running when the next open rebuilds the content.
            4 => (
                "burst",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                    Step::new(gap, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                    Step::new(gap, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                ],
            ),
            // A second surface opened on top of this one, and both dismissed.
            _ => (
                "over",
                vec![
                    Step::new(0, Act::ClickTrigger),
                    Step::new(hold, Act::Escape),
                    Step::new(1, Act::Jiggle),
                    Step::new(gap, Act::ClickDialog),
                    Step::new(300, Act::Tab),
                    Step::new(40, Act::Enter),
                    Step::new(hold + 120, Act::Escape),
                    Step::new(gap + 200, Act::Escape),
                ],
            ),
        }
    };
    // Every cycle ends the same way, because the question every cycle asks is the same one.
    steps.push(Step::new(420, Act::Mark));
    steps.push(Step::new(0, Act::PressProbe));
    steps.push(Step::new(260, Act::CheckClick));
    steps.push(Step::new(60, Act::PressField));
    steps.push(Step::new(120, Act::TypeChar));
    steps.push(Step::new(260, Act::CheckKey));
    (kind, steps)
}

/// The application, with a script of presses and keys delivered to it between frames.
struct Driving {
    /// The application.
    inner: Box<dyn AppHandler>,
    /// Where the window publishes what to press, and what the witnesses say.
    shared: Shared,
    /// Which surface this run drives.
    family: Family,
    /// The steps still to run in the current cycle.
    queue: VecDeque<Step>,
    /// When the next step is due.
    due: Option<Instant>,
    /// Which cycle is running.
    number: u64,
    /// How many cycles the run does.
    total: u64,
    /// The generator the holds and gaps come from.
    rng: Rng,
    /// What the probe had counted when the cycle marked it.
    marked_presses: u64,
    /// What the field held when the cycle marked it.
    marked_typed: u64,
    /// How many presses this cycle has aimed at the current witness.
    tries: u32,
    /// What the current cycle is called, for the report.
    named: &'static str,
    /// How many cycles failed the click witness.
    click_failures: u64,
    /// How many cycles failed the key witness.
    key_failures: u64,
    /// Whether the current cycle has already been counted as failed.
    failed: bool,
    /// How the window is screenshotted.
    shot: Shot,
    /// The surface everything is delivered to.
    surface: Option<SurfaceId>,
    /// How big that surface is, in device pixels, which is what a capture is measured against.
    size: Option<(f32, f32)>,
    /// When the script last did anything, so a run that has stopped says so rather than hanging.
    stepped: Instant,
    /// What the last turn of the loop decided to park on.
    turn: Arc<Mutex<Turn>>,
}

impl Driving {
    /// Wraps `inner`, driving the window `shared` describes.
    fn new(
        inner: Box<dyn AppHandler>,
        shared: Shared,
        family: Family,
        total: u64,
        seed: u64,
    ) -> Self {
        Self {
            inner,
            shared,
            family,
            queue: VecDeque::new(),
            due: None,
            number: 0,
            total,
            rng: Rng(seed),
            marked_presses: 0,
            marked_typed: 0,
            tries: 0,
            named: "none",
            click_failures: 0,
            key_failures: 0,
            failed: false,
            shot: Shot::from_environment(),
            surface: None,
            size: None,
            stepped: Instant::now(),
            turn: {
                let turn = Arc::new(Mutex::new(Turn::default()));
                watch(Arc::clone(&turn));
                turn
            },
        }
    }

    /// What the window has published.
    fn aim(&self) -> Aim {
        self.shared.lock().map(|held| *held).unwrap_or_default()
    }

    /// Where in a capture of the window a swatch published at `centre` is.
    fn at(&self, centre: Option<(f32, f32)>) -> Option<At> {
        let (x, y) = centre?;
        let (width, height) = self.size?;
        (width > 1.0 && height > 1.0).then_some(At {
            x: x / width,
            y: y / height,
        })
    }

    /// Delivers a press and a release over `at`.
    fn click(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, at: Point<CssPx, Css>) {
        for action in [
            PointerAction::Moved,
            PointerAction::Pressed,
            PointerAction::Released,
        ] {
            let event = SurfaceEvent::Pointer {
                action,
                event: PointerEvent::mouse(at),
                modifiers: Modifiers::NONE,
                timestamp: cx.clock().timestamp(),
            };
            self.inner.surface_event(cx, surface, event);
        }
    }

    /// Delivers one named key, down and up.
    fn key(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, named: NamedKey, code: KeyCode) {
        for state in [KeyState::Pressed, KeyState::Released] {
            let event = SurfaceEvent::Key {
                state,
                event: KeyEvent::named(named, PhysicalKey::Code(code)),
                modifiers: Modifiers::NONE,
                timestamp: cx.clock().timestamp(),
            };
            self.inner.surface_event(cx, surface, event);
        }
    }

    /// Delivers one character, down and up, wherever the keyboard is.
    fn character(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, what: char) {
        let event = KeyEvent::character(what.to_string());
        for state in [KeyState::Pressed, KeyState::Released] {
            self.inner.surface_event(
                cx,
                surface,
                SurfaceEvent::Key {
                    state,
                    event: event.clone(),
                    modifiers: Modifiers::NONE,
                    timestamp: cx.clock().timestamp(),
                },
            );
        }
    }

    /// Runs whatever steps are due, and starts the next cycle when the current one runs out.
    fn script(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        loop {
            let now = Instant::now();
            if self.aim().probe.is_none() || !self.shot.ready() {
                if self.stepped.elapsed() > Duration::from_secs(2) {
                    say(&format!(
                        "STALL.gate probe={} ready={} since={:?}",
                        self.aim().probe.is_some(),
                        self.shot.ready(),
                        self.stepped.elapsed()
                    ));
                }
                self.due = Some(now + Duration::from_millis(250));
                return;
            }
            if self.queue.is_empty() {
                if self.number >= self.total {
                    self.finish();
                }
                self.number += 1;
                let (kind, steps) = cycle(self.family, self.number, &mut self.rng);
                self.named = kind;
                self.failed = false;
                self.queue.extend(steps);
                self.due = Some(now);
                say(&format!(
                    "cycle.begin number={} family={} kind={kind}",
                    self.number,
                    self.family.name()
                ));
            }
            let due = *self.due.get_or_insert(now);
            if now < due {
                return;
            }
            let Some(step) = self.queue.pop_front() else {
                return;
            };
            self.perform(cx, surface, step.act);
            self.stepped = Instant::now();
            let next = self.queue.front().map_or(Duration::ZERO, |step| step.after);
            self.due = Some(Instant::now() + next);
            if !next.is_zero() {
                return;
            }
        }
    }

    /// Does one thing.
    fn perform(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, act: Act) {
        let aim = self.aim();
        match act {
            Act::Named(name) => self.named = name,
            Act::ClickTrigger => {
                let at = match self.family {
                    Family::Drawer => aim.drawer,
                    Family::Dialog | Family::Nested => aim.dialog,
                    Family::Alert => aim.alert,
                    Family::Sheet => aim.sheet,
                };
                self.aimed(cx, surface, at, "trigger");
            }
            Act::ClickDialog => self.aimed(cx, surface, aim.dialog, "dialog-trigger"),
            Act::Tab => {
                say("act at=tab");
                self.key(cx, surface, NamedKey::Tab, KeyCode::Tab);
            }
            Act::Enter => {
                say("act at=enter");
                self.key(cx, surface, NamedKey::Enter, KeyCode::Enter);
            }
            Act::ClickAway => self.aimed(cx, surface, aim.probe, "away"),
            Act::Escape => {
                say("act at=escape");
                self.key(cx, surface, NamedKey::Escape, KeyCode::Escape);
            }
            Act::Jiggle => {
                let Some((x, y)) = aim.dialog else { return };
                for step in 0..6 {
                    let at = Point::new(CssPx(x + step as f32 * 37.0), CssPx(y + 11.0));
                    let event = SurfaceEvent::Pointer {
                        action: PointerAction::Moved,
                        event: PointerEvent::mouse(at),
                        modifiers: Modifiers::NONE,
                        timestamp: cx.clock().timestamp(),
                    };
                    self.inner.surface_event(cx, surface, event);
                }
            }
            Act::Mark => {
                self.marked_presses = aim.presses;
                self.marked_typed = aim.typed;
                self.tries = 0;
            }
            Act::PressProbe => {
                self.tries += 1;
                self.aimed(cx, surface, aim.probe, "probe");
            }
            Act::CheckClick => {
                // Both halves of the question, and both are needed. That the count advanced says
                // the press reached the handler; that the swatch is the colour the count implies
                // says the window is showing what the handler did.
                let want = Witness::click(aim.presses % 4);
                let Some(at) = self.at(aim.click_witness) else {
                    self.queue.push_front(Step::new(200, Act::CheckClick));
                    return;
                };
                if aim.presses > self.marked_presses && self.shot.reads(Witness::CLICK, at, want) {
                    self.tries = 0;
                    return;
                }
                if self.tries < TRIES {
                    say(&format!(
                        "witness.missed which=click number={} try={}",
                        self.number, self.tries
                    ));
                    self.queue.push_front(Step::new(360, Act::CheckClick));
                    self.queue.push_front(Step::new(300, Act::PressProbe));
                    return;
                }
                self.click_failures += 1;
                self.failed = true;
                say(&format!(
                    "DEAF which=click number={} kind={} presses={} marked={}",
                    self.number, self.named, aim.presses, self.marked_presses
                ));
                self.recover(cx, surface);
                self.tries = 0;
            }
            Act::PressField => {
                self.tries += 1;
                self.aimed(cx, surface, aim.field, "field");
            }
            Act::TypeChar => {
                say("act at=type");
                self.character(cx, surface, 'x');
                say("act.done at=type");
            }
            Act::CheckKey => {
                let want = Witness::key(aim.typed % 4);
                let Some(at) = self.at(aim.key_witness) else {
                    self.queue.push_front(Step::new(200, Act::CheckKey));
                    return;
                };
                if aim.typed > self.marked_typed && self.shot.reads(Witness::KEY, at, want) {
                    self.report(true);
                    return;
                }
                if self.tries < TRIES {
                    say(&format!(
                        "witness.missed which=key number={} try={}",
                        self.number, self.tries
                    ));
                    self.queue.push_front(Step::new(360, Act::CheckKey));
                    self.queue.push_front(Step::new(120, Act::TypeChar));
                    self.queue.push_front(Step::new(300, Act::PressField));
                    return;
                }
                self.key_failures += 1;
                say(&format!(
                    "DEAF which=key number={} kind={} typed={} marked={}",
                    self.number, self.named, aim.typed, self.marked_typed
                ));
                self.recover(cx, surface);
                self.report(false);
            }
        }
    }

    /// Writes the cycle's line, and remembers whether it passed.
    fn report(&mut self, passed: bool) {
        println!(
            "{}\t{}\t{}\t{}",
            self.number,
            self.family.name(),
            self.named,
            if passed && !self.failed { "ok" } else { "DEAF" }
        );
    }

    /// Tries to get the window back to a state the next cycle can start from.
    ///
    /// A run that stops at the first failure cannot count them, and a run that carries on into a
    /// window still covered by a surface counts one failure many times. So a failed cycle is given
    /// several Escapes and a press well away from anything before the next one starts.
    fn recover(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        for _ in 0..4 {
            self.key(cx, surface, NamedKey::Escape, KeyCode::Escape);
        }
        let aim = self.aim();
        if let Some((x, y)) = aim.probe {
            self.click(cx, surface, Point::new(CssPx(x), CssPx(y)));
        }
    }

    /// Presses what `at` names, or reports that the window has not published it yet.
    fn aimed(
        &mut self,
        cx: &dyn PlatformCx,
        surface: SurfaceId,
        at: Option<(f32, f32)>,
        what: &str,
    ) {
        let Some((x, y)) = at else {
            say(&format!("act at={what} missing=true"));
            return;
        };
        say(&format!("act at={what} x={x:.1} y={y:.1}"));
        self.click(cx, surface, Point::new(CssPx(x), CssPx(y)));
        say(&format!("act.done at={what}"));
    }

    /// Ends the run.
    fn finish(&mut self) -> ! {
        println!(
            "# family={} cycles={} click-failures={} key-failures={} shots={} shot-errors={}",
            self.family.name(),
            self.number,
            self.click_failures,
            self.key_failures,
            self.shot.taken(),
            self.shot.errors()
        );
        say(&format!(
            "run.done family={} cycles={} click-failures={} key-failures={}",
            self.family.name(),
            self.number,
            self.click_failures,
            self.key_failures
        ));
        let code = i32::from(self.click_failures + self.key_failures > 0);
        std::process::exit(code);
    }
}

impl AppHandler for Driving {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        self.surface = Some(surface);
        if let SurfaceEvent::Resized(size) = &event {
            self.size = Some((size.width.0, size.height.0));
        }
        let redraw = matches!(event, SurfaceEvent::RedrawRequested);
        self.inner.surface_event(cx, surface, event);
        if redraw {
            self.script(cx, surface);
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        let inner = self.inner.idle(cx);
        // A deadline already past is not installed by the loop at all — it would wake instantly
        // for ever — so the loop parks indefinitely and a script waiting on it never runs again.
        let now = Instant::now();
        let merged = match self.due {
            Some(due) => inner.merge(IdlePolicy::BlockUntil(
                due.max(now + Duration::from_millis(1)),
            )),
            None => inner,
        };
        // A deadline that expires between the moment it is computed and the moment the loop
        // installs it is dropped, and the loop then parks on nothing at all — the window stops
        // for good. That is a defect in the platform adapter and not in what this run is about,
        // so a deadline close enough to be lost that way is turned into a turn of the loop
        // instead. Spinning delays nothing: the moment is serviced as soon as it passes.
        let policy = match merged.deadline() {
            Some(at) if at.saturating_duration_since(now) < Duration::from_millis(2) => {
                IdlePolicy::Spin
            }
            _ => merged,
        };
        if let Ok(mut held) = self.turn.lock() {
            held.turns += 1;
            held.decided = format!(
                "inner={inner:?} policy={policy:?} in={:?} queued={} due_in={:?}",
                policy
                    .deadline()
                    .map(|at| at.saturating_duration_since(now)),
                self.queue.len(),
                self.due.map(|due| due.saturating_duration_since(now)),
            );
        }
        policy
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        if let Ok(mut held) = self.turn.lock() {
            held.woken += 1;
        }
        self.inner.deadline_reached(cx);
        if let Some(surface) = self.surface {
            self.script(cx, surface);
        }
    }
}

/// Opens the window and drives it.
fn main() -> Result<(), zgui::Error> {
    let mut args = std::env::args().skip(1);
    let id = args
        .next()
        .unwrap_or_else(|| "dev.zgui.modal-verify".to_owned());
    let family = args
        .next()
        .as_deref()
        .and_then(Family::parse)
        .unwrap_or(Family::Drawer);
    let total: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(200);
    let seed: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(1);
    let broken = args.next().is_some_and(|arg| arg == "broken");
    let sheet = if broken {
        format!("{}\n{RIG_SHEET}\n{BROKEN_SHEET}", crate::shell::SHEET)
    } else {
        format!("{}\n{RIG_SHEET}", crate::shell::SHEET)
    };

    let shared: Shared = Arc::new(Mutex::new(Aim::default()));
    let driving = Arc::clone(&shared);
    zgui::app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(sheet)
        .run_on(
            move |handler| {
                zgui_platform_winit::run(Box::new(Driving::new(
                    handler, driving, family, total, seed,
                )))
            },
            move || view! { Rig(shared = Arc::clone(&shared)) },
        )
}
