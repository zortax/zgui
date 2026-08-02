//! Hundreds of open-and-close cycles on a real modal, on a real compositor, looking for the one
//! that does not close.
//!
//! A modal that occasionally refuses to unmount cannot be found by a headless test: ten cycles pass
//! there, and so does re-opening during the exit animation. What is different on a screen is when
//! the frames fall — the exit animation's last frame, the reactive flush that rebuilds the content
//! and the deferred check the presence schedules are three things whose order is decided by a real
//! refresh rate, and at two hundred and forty hertz they can fall in an order no headless clock
//! produces.
//!
//! So this opens a window, drives the real [`Drawer`] and the real nested [`Dialog`] through cycles
//! whose timings are varied on purpose, and after each cycle asks the one question that matters:
//! **does the page still take a click?** A probe control sits behind the surfaces and counts its
//! own presses. A cycle that ends with the probe deaf is a cycle whose modal is still up with its
//! focus trap installed and its layer on the stack, and that is the defect.
//!
//! Nothing is injected into the desktop. Every press and every key is a platform event handed to
//! the application exactly as the windowing backend hands one over, so the compositor decides when
//! frames happen and nothing else on the machine can receive a stray press.
//!
//! ```text
//! ZGUI_MODAL_TRACE=1 modal-stick <app-id> <cycles> [seed] 2> trace.log
//! ```
//!
//! The trace the framework writes under `ZGUI_MODAL_TRACE` is what says *why*: it carries the
//! presence state, which node the exit listeners are bound to, which node each animation end was
//! dispatched at and how many listeners it found, the layer stack at every push and pop, every
//! Escape and which layer claimed it, and every focus trap installed and released. On a stuck cycle
//! one further Escape is sent before the dump, because whether a layer answers it says immediately
//! whether the surface is still registered.

#![forbid(unsafe_code)]
#![allow(
    dead_code,
    reason = "the gallery's own source is included whole, as the document this window's frames are \
              the cost of, rather than as thirteen sections a driver presses"
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

/// Writes one driver line on the same clock the framework's trace lines carry.
fn say(fields: &str) {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    eprintln!("ZMT {at} {fields}");
}

/// Where each thing the script presses is, and how many presses the probe has counted.
///
/// Written by the window and read by the driver, both on the loop's own thread. The positions are
/// published rather than assumed because a window manager decides how big a window is, and a press
/// aimed at a remembered coordinate in a tiled window presses whatever ended up there.
#[derive(Default, Clone, Copy)]
struct Aim {
    /// The centre of the drawer's trigger, in CSS pixels.
    drawer: Option<(f32, f32)>,
    /// The centre of the dialog's trigger.
    dialog: Option<(f32, f32)>,
    /// The centre of the select inside the dialog, while the dialog is up.
    select: Option<(f32, f32)>,
    /// The centre of the probe control.
    probe: Option<(f32, f32)>,
    /// How many presses the probe has received.
    presses: u64,
}

/// What the window and the driver share.
type Shared = Arc<Mutex<Aim>>;

/// The window: the real components, and one control behind them that counts presses.
#[component]
fn Rig(
    /// Where the positions and the press count are published.
    shared: Shared,
) -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Light);
    let currency = RwSignal::new_local("gbp".to_owned());
    let drawer = NodeRef::new();
    let dialog = NodeRef::new();
    let select = NodeRef::new();
    let probe = NodeRef::new();
    let presses = RwSignal::new_local(0_u64);

    // Published from a timer rather than from an observation, because three of the four elements do
    // not exist for most of the run: the select is inside the dialog and the dialog is closed, so
    // there is nothing to observe until there is, and a poll answers "not yet" without ceremony.
    let published = Arc::clone(&shared);
    let publish = move || {
        let scale = probe.scale().max(0.001);
        let centre = |handle: NodeRef| {
            handle.window_bounds().map(|box_| {
                (
                    (box_.origin.x.0 + box_.size.width.0 / 2.0) / scale,
                    (box_.origin.y.0 + box_.size.height.0 / 2.0) / scale,
                )
            })
        };
        if let Ok(mut held) = published.lock() {
            held.drawer = centre(drawer);
            held.dialog = centre(dialog);
            held.select = centre(select);
            held.probe = centre(probe);
            held.presses = presses.get_untracked();
        }
    };
    // Held for the life of the window: dropping the handle cancels the timer, and a driver with no
    // positions published presses nothing at all.
    let polling = set_interval(Duration::from_millis(16), publish);
    on_cleanup_local(move || drop(polling));

    view! {
        ThemeProvider(scheme = scheme) {
            column(class = "rig") {
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
                                box(node_ref = select, class = "slot") {
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
                                }
                                DialogFooter {
                                    DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
                                    Button {"Rename"}
                                }
                            }
                        }
                    }
                }
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
                // The shipped gallery, so that a frame of this window costs what a frame of the
                // window the defect was seen in costs. A race between an exit animation's last
                // frame and the flush that rebuilds the content is a race whose window is measured
                // in frame time, and a document of four controls has frames nothing fits inside.
                Gallery()
            }
        }
    }
}

/// What the driver's own controls look like, over the gallery's own sheet.
const RIG_SHEET: &str = css!(
    ".rig { display: block; padding: 32px; gap: 24px; width: 100% }
     .rig-row { gap: 16px }
     .slot { display: block }
     .probe { display: block; width: 240px; height: 56px; background-color: #3b4252;
              color: #eceff4; padding: 16px }"
);

/// One thing the script does.
#[derive(Clone, Copy, Debug)]
enum Act {
    /// Press and release over the drawer's trigger.
    ClickDrawer,
    /// Press and release over the dialog's trigger.
    ClickDialog,
    /// Move focus on by one, which is how the surface inside the dialog is reached.
    ///
    /// Reached by key rather than by press because a modal surface is centred with a transform, and
    /// the box a handle reports is the one layout gave it — the untransformed one. A press aimed at
    /// that lands on the scrim and dismisses the dialog instead of opening anything inside it.
    Tab,
    /// Enter down and up, which opens whatever is focused.
    Enter,
    /// Press and release well away from anything, which lands on the scrim while one is up.
    ClickAway,
    /// Escape down and up.
    Escape,
    /// Remember what the probe has counted, before testing it.
    ProbeMark,
    /// Press the probe.
    ProbePress,
    /// Fail the cycle if the probe did not count that press.
    ProbeCheck,
    /// Several pointer moves across the window, which starts hover transitions of their own.
    ///
    /// An exit animation that is the only thing running is an exit animation whose end is the only
    /// edge in its frame. Everything about the order these are dispatched in is only visible when
    /// there is more than one.
    Jiggle,
    /// Write a marker into the trace.
    Mark(&'static str),
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

/// How many presses the probe is given before the page is called deaf.
///
/// More than one, because the first is entitled to be swallowed: a surface that is still fading out
/// is still a surface, and a press on it is a dismissal rather than a miss.
const PROBE_TRIES: u32 = 4;

/// How long a surface is left up before it is dismissed, in milliseconds.
///
/// Spread across the whole of an exit animation and beyond it on purpose: the shortest of these
/// dismisses a surface whose *enter* animation is still running, and the longest dismisses one that
/// has been still for a while, and the defect is a race between those two ends.
const HOLDS: [u64; 8] = [0, 2, 6, 12, 24, 48, 120, 260];

/// The steps of one cycle, chosen by its number so that the mixture repeats predictably.
fn cycle(number: u64, rng: &mut Rng) -> Vec<Step> {
    let hold = HOLDS[(rng.below(HOLDS.len() as u64)) as usize];
    let gap = rng.below(40) + 4;
    let mut steps = match number % 12 {
        // The plain case, at every hold: open, wait, Escape.
        0 => vec![
            Step::new(0, Act::Mark("drawer-plain")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
        ],
        // Two Escapes in quick succession, the second inside the exit animation.
        1 => vec![
            Step::new(0, Act::Mark("drawer-double-escape")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(gap, Act::Escape),
        ],
        // Re-opened while it is still leaving, then closed again.
        2 => vec![
            Step::new(0, Act::Mark("drawer-reopen")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(gap, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
        ],
        // Dismissed by a press on the scrim rather than by a key.
        3 => vec![
            Step::new(0, Act::Mark("drawer-outside")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold + 90, Act::ClickAway),
        ],
        // The dialog, plain.
        4 => vec![
            Step::new(0, Act::Mark("dialog-plain")),
            Step::new(0, Act::ClickDialog),
            Step::new(hold, Act::Escape),
        ],
        // A surface inside a surface, dismissed innermost first.
        5 => vec![
            Step::new(0, Act::Mark("dialog-nested")),
            Step::new(0, Act::ClickDialog),
            Step::new(300, Act::Tab),
            Step::new(40, Act::Enter),
            Step::new(hold + 120, Act::Escape),
            Step::new(gap, Act::Escape),
        ],
        // The same, dismissed with both Escapes back to back.
        6 => vec![
            Step::new(0, Act::Mark("dialog-nested-fast")),
            Step::new(0, Act::ClickDialog),
            Step::new(300, Act::Tab),
            Step::new(40, Act::Enter),
            Step::new(hold + 120, Act::Escape),
            Step::new(0, Act::Escape),
        ],
        // Three openings and three closings with nothing between them, which is the shortest
        // path to a close whose exit is still running when the next open rebuilds the content.
        8 => vec![
            Step::new(0, Act::Mark("drawer-burst")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(gap, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(gap, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
        ],
        // The same, with the page busy: hovers running beside the exit animation.
        9 => vec![
            Step::new(0, Act::Mark("drawer-busy")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(1, Act::Jiggle),
            Step::new(gap, Act::Jiggle),
            Step::new(gap, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(1, Act::Jiggle),
        ],
        // The dialog opened and closed from its trigger, which is behind its own scrim and is
        // therefore an outside press and a re-opening click in one gesture.
        10 => vec![
            Step::new(0, Act::Mark("dialog-toggle")),
            Step::new(0, Act::ClickDialog),
            Step::new(hold, Act::ClickDialog),
            Step::new(gap, Act::ClickDialog),
            Step::new(hold, Act::Escape),
        ],
        // Nested, with the outer one dismissed while the inner one is still leaving.
        11 => vec![
            Step::new(0, Act::Mark("dialog-nested-busy")),
            Step::new(0, Act::ClickDialog),
            Step::new(300, Act::Tab),
            Step::new(40, Act::Enter),
            Step::new(hold + 120, Act::Escape),
            Step::new(1, Act::Jiggle),
            Step::new(gap, Act::Escape),
            Step::new(1, Act::Escape),
        ],
        // One after the other, which is what a person does when they open the wrong one.
        _ => vec![
            Step::new(0, Act::Mark("drawer-then-dialog")),
            Step::new(0, Act::ClickDrawer),
            Step::new(hold, Act::Escape),
            Step::new(gap, Act::ClickDialog),
            Step::new(hold, Act::Escape),
        ],
    };
    // Every cycle ends the same way, because the question every cycle asks is the same one.
    steps.push(Step::new(420, Act::ProbeMark));
    steps.push(Step::new(0, Act::ProbePress));
    steps.push(Step::new(200, Act::ProbeCheck));
    steps
}

/// The application, with a script of presses and keys delivered to it between frames.
struct Driving {
    /// The application.
    inner: Box<dyn AppHandler>,
    /// Where the window publishes what to press, and what the probe has counted.
    shared: Shared,
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
    /// What the probe had counted when the cycle last marked it.
    marked: u64,
    /// How many presses this cycle has aimed at the probe since it marked it.
    tries: u32,
    /// What the current cycle is called, for the report.
    named: &'static str,
    /// The surface everything is delivered to.
    surface: Option<SurfaceId>,
    /// Whether a stuck cycle has been found and the dump is under way.
    stuck: bool,
}

impl Driving {
    /// Wraps `inner`, driving the window `shared` describes.
    fn new(inner: Box<dyn AppHandler>, shared: Shared, total: u64, seed: u64) -> Self {
        Self {
            inner,
            shared,
            queue: VecDeque::new(),
            due: None,
            number: 0,
            total,
            rng: Rng(seed),
            marked: 0,
            tries: 0,
            named: "none",
            surface: None,
            stuck: false,
        }
    }

    /// What the window has published.
    fn aim(&self) -> Aim {
        self.shared.lock().map(|held| *held).unwrap_or_default()
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

    /// Delivers one Escape, down and up.
    fn escape(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        self.key(cx, surface, NamedKey::Escape, KeyCode::Escape);
    }

    /// Runs whatever steps are due, and starts the next cycle when the current one runs out.
    fn script(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        if self.stuck {
            return;
        }
        loop {
            let now = Instant::now();
            if self.aim().probe.is_none() {
                self.due = Some(now + Duration::from_millis(250));
                return;
            }
            if self.queue.is_empty() {
                if self.number >= self.total {
                    self.finish(0);
                }
                self.number += 1;
                let number = self.number;
                let steps = cycle(number, &mut self.rng);
                self.queue.extend(steps);
                self.due = Some(now);
                say(&format!("cycle.begin number={number}"));
            }
            let due = *self.due.get_or_insert(now);
            if now < due {
                return;
            }
            let Some(step) = self.queue.pop_front() else {
                return;
            };
            self.perform(cx, surface, step.act);
            if self.stuck {
                return;
            }
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
            Act::Mark(name) => {
                self.named = name;
                say(&format!("cycle.kind number={} kind={name}", self.number));
            }
            Act::ClickDrawer => self.aimed(cx, surface, aim.drawer, "drawer-trigger"),
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
                self.escape(cx, surface);
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
            Act::ProbeMark => {
                self.marked = aim.presses;
                self.tries = 0;
            }
            Act::ProbePress => {
                self.tries += 1;
                self.aimed(cx, surface, aim.probe, "probe");
            }
            Act::ProbeCheck => {
                if aim.presses > self.marked {
                    return;
                }
                // A press that lands on a surface still on its way out is a press that dismisses it
                // rather than one the page heard, so the first miss proves nothing. What proves the
                // page is deaf is that no number of presses, spaced well past any exit animation,
                // ever reaches it.
                if self.tries < PROBE_TRIES {
                    say(&format!(
                        "probe.missed number={} kind={} try={}",
                        self.number, self.named, self.tries
                    ));
                    self.queue.push_front(Step::new(360, Act::ProbeCheck));
                    self.queue.push_front(Step::new(360, Act::ProbePress));
                    return;
                }
                self.caught(cx, surface);
            }
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
    }

    /// Records a stuck cycle, asks the surface one more question, and ends the run.
    ///
    /// The extra Escape is the whole of the diagnosis and costs one line: a layer that answers it
    /// is a layer still on the stack, a layer that answers and reports itself not topmost is a
    /// layer with something stuck above it, and silence means the listeners went while the surface
    /// stayed.
    fn caught(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        self.stuck = true;
        say(&format!(
            "STUCK number={} kind={} probe={} marked={} tries={}",
            self.number,
            self.named,
            self.aim().presses,
            self.marked,
            self.tries
        ));
        self.escape(cx, surface);
        let shared = Arc::clone(&self.shared);
        let number = self.number;
        let named = self.named;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            let presses = shared.lock().map(|held| held.presses).unwrap_or_default();
            say(&format!(
                "STUCK-END number={number} kind={named} presses={presses}"
            ));
            std::process::exit(7);
        });
    }

    /// Ends the run.
    fn finish(&mut self, code: i32) -> ! {
        say(&format!("run.done cycles={}", self.number));
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
        // The script's own deadline is merged into the park, so that a window with nothing else to
        // do still wakes for the next step instead of sleeping through the run.
        let policy = self.inner.idle(cx);
        match self.due {
            Some(due) => policy.merge(IdlePolicy::BlockUntil(due)),
            None => policy,
        }
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
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
        .unwrap_or_else(|| "dev.zgui.modal-stick".to_owned());
    let total: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(200);
    let seed: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(1);

    let shared: Shared = Arc::new(Mutex::new(Aim::default()));
    let driving = Arc::clone(&shared);
    zgui::app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(format!("{}\n{RIG_SHEET}", crate::shell::SHEET))
        .run_on(
            move |handler| {
                zgui_platform_winit::run(Box::new(Driving::new(handler, driving, total, seed)))
            },
            move || view! { Rig(shared = Arc::clone(&shared)) },
        )
}
