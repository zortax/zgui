//! What a control bound to an application's own signal draws when it is clicked.
//!
//! # Why these are not the assertions in `controls.rs`
//!
//! Those mount a control, send it an event and ask the document what changed — the state bit, the
//! semantics, the attribute. All of that can be true of a window in which nothing moves, and every
//! control here once was: bound to a signal, a checkbox reported the press, wrote its state bit,
//! announced itself ticked to a reader, and was painted unticked for ever. The suite was green.
//!
//! So nothing here reads a state bit. Each fixture is written the way an application writes one —
//! `checked=signal`, and no callback anywhere — the pointer is driven over the control the way a
//! mouse drives one, the pointer is taken away again so hover cannot be mistaken for the answer,
//! the clock is moved past the transition, and the pixels the device composed are compared with
//! the pixels it composed before the click. The caller's signal is checked too, because a control
//! that repaints without moving the value it was bound to is the same defect facing the other way.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui::vocab::NamedKey;
use zgui::{reactive, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};

/// The page every fixture is laid out on: a flat white surface with room around the controls.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 32px; gap: 24px; align-items: flex-start }
                     .track { width: 240px }";

/// How many of a control's pixels have to change before the screen counts as having moved.
///
/// A control is mostly its own background, and the part of it that says what it holds — a tick, a
/// dot, the moved end of a track — is a fraction of that. One per cent of the pixels inside the
/// control is far above the noise of a settled frame, which is zero, and far below what any of
/// these changes actually paints.
const MOVED: f32 = 0.01;

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Where a fixture leaves the elements it built, so a test can find them again.
#[derive(Clone, Default)]
struct Built(Rc<RefCell<Vec<NodeRef>>>);

impl Built {
    /// Records the references this build produced, replacing whatever the last one left.
    fn keep(&self, refs: &[NodeRef]) {
        *self.0.borrow_mut() = refs.to_vec();
    }

    /// The node the `which`th reference was bound to.
    ///
    /// # Panics
    ///
    /// Panics before the view has been built, and for a control that never bound its reference.
    fn node(&self, which: usize) -> NodeId {
        self.0.borrow()[which]
            .get_untracked()
            .expect("the control bound its reference when it was built")
    }
}

/// A signal made once, on the first build, and handed back on every build after it.
///
/// A view is rebuilt whenever what it depends on moves, and a fixture that made a fresh signal
/// each time would hand every assertion a value the last interaction never touched.
#[derive(Clone)]
struct Once<T: Clone + 'static>(Rc<RefCell<Option<T>>>);

impl<T: Clone + 'static> Once<T> {
    /// Nothing yet.
    fn new() -> Self {
        Self(Rc::new(RefCell::new(None)))
    }

    /// The value, made by `make` the first time and remembered after that.
    fn get_or_make(&self, make: impl FnOnce() -> T) -> T {
        let mut held = self.0.borrow_mut();
        held.get_or_insert_with(make).clone()
    }

    /// The value, for a test that is asking after the view was built.
    ///
    /// # Panics
    ///
    /// Panics before the first build, which would otherwise read a signal nothing is bound to.
    fn taken(&self) -> T {
        self.0
            .borrow()
            .clone()
            .expect("the view was built, so the signal exists")
    }
}

/// The picture inside `node`, one entry per pixel.
fn picture(stage: &Stage, node: NodeId) -> Vec<(u8, u8, u8)> {
    stage.colours_in(stage.rect_of(node))
}

/// What fraction of `node`'s pixels differ between two pictures of it.
///
/// # Panics
///
/// Panics when the two readings are of different sizes, which means the control moved or resized
/// between them and no pixel in one stands for the same pixel in the other.
fn changed(before: &[(u8, u8, u8)], after: &[(u8, u8, u8)]) -> f32 {
    assert_eq!(
        before.len(),
        after.len(),
        "the control changed size between the two readings"
    );
    assert!(!before.is_empty(), "the control covers no pixels at all");
    let differing = before
        .iter()
        .zip(after)
        .filter(|(one, two)| one != two)
        .count();
    differing as f32 / before.len() as f32
}

/// Clicks `at`, takes the pointer off the surface, and lets everything settle.
///
/// The pointer is removed because a control under a resting pointer is drawn in its hover colour,
/// and a fixture that left it there would pass on hover alone for a control whose value never
/// moved.
fn click_and_stand_back(stage: &mut Stage, at: Point<DevicePx, Device>) {
    stage.click(at);
    stage.leave();
    stage.wait(SETTLED);
    stage.repaint();
}

// ---- the page ------------------------------------------------------------------------------

/// Every control a caller binds, bound the way a caller binds one: a signal, and nothing else.
#[derive(Clone)]
struct Bound {
    /// Whether the checkbox is ticked.
    ticked: Once<RwSignal<Checked, reactive::LocalStorage>>,
    /// Whether the switch is on.
    switched: Once<RwSignal<bool, reactive::LocalStorage>>,
    /// Which billing plan is chosen.
    plan: Once<RwSignal<String, reactive::LocalStorage>>,
    /// Whether the lone toggle is pressed.
    bold: Once<RwSignal<bool, reactive::LocalStorage>>,
    /// Which alignment the group holds.
    align: Once<RwSignal<Vec<String>, reactive::LocalStorage>>,
    /// Where the slider is.
    volume: Once<RwSignal<f64, reactive::LocalStorage>>,
    /// The checkbox, the switch, the two radio items, the toggle, two group items, the slider.
    built: Built,
}

impl Bound {
    /// Handles for a fixture that has not been built yet.
    fn new() -> Self {
        Self {
            ticked: Once::new(),
            switched: Once::new(),
            plan: Once::new(),
            bold: Once::new(),
            align: Once::new(),
            volume: Once::new(),
            built: Built::default(),
        }
    }

    /// The page, with every control bound and not one callback on any of them.
    fn view(&self) -> impl Fn() -> AnyView + use<> {
        let held = self.clone();
        move || {
            let ticked = held.ticked.get_or_make(|| RwSignal::new_local(Checked::No));
            let switched = held.switched.get_or_make(|| RwSignal::new_local(false));
            let plan = held
                .plan
                .get_or_make(|| RwSignal::new_local("monthly".to_owned()));
            let bold = held.bold.get_or_make(|| RwSignal::new_local(false));
            let align = held
                .align
                .get_or_make(|| RwSignal::new_local(vec!["left".to_owned()]));
            let volume = held.volume.get_or_make(|| RwSignal::new_local(40.0));

            let checkbox = NodeRef::new();
            let switch = NodeRef::new();
            let monthly = NodeRef::new();
            let yearly = NodeRef::new();
            let toggle = NodeRef::new();
            let left = NodeRef::new();
            let right = NodeRef::new();
            let slider = NodeRef::new();
            held.built.keep(&[
                checkbox, switch, monthly, yearly, toggle, left, right, slider,
            ]);

            AnyView::new(view! {
                ThemeProvider {
                    column(class = "page") {
                        Checkbox(node_ref = checkbox, checked = ticked, a11y:label = "Terms")
                        Switch(node_ref = switch, checked = switched, a11y:label = "Emails")
                        RadioGroup(value = plan, label = "Billing") {
                            row {
                                RadioGroupItem(node_ref = monthly, value = "monthly", label = "Monthly")
                                RadioGroupItem(node_ref = yearly, value = "yearly", label = "Yearly")
                            }
                        }
                        Toggle(node_ref = toggle, pressed = bold, label = "Bold") {"B"}
                        ToggleGroup(value = align, selection = ToggleSelection::Single, label = "Align") {
                            ToggleGroupItem(node_ref = left, value = "left", label = "Left") {"L"}
                            ToggleGroupItem(node_ref = right, value = "right", label = "Right") {"R"}
                        }
                        column(class = "track") {
                            Slider(
                                node_ref = slider,
                                value = volume,
                                min = 0.0,
                                max = 100.0,
                                step = 5.0,
                                label = "Volume"
                            )
                        }
                    }
                }
            })
        }
    }
}

// ---- one test per control --------------------------------------------------------------------

#[test]
fn a_bound_checkbox_fills_itself_in_and_moves_the_signal_it_was_bound_to() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let checkbox = bound.built.node(0);
    let before = picture(&stage, checkbox);

    let at = stage.centre_of(checkbox);
    click_and_stand_back(&mut stage, at);

    assert_eq!(
        bound.ticked.taken().get_untracked(),
        Checked::Yes,
        "the click never reached the signal the checkbox was bound to"
    );
    let after = picture(&stage, checkbox);
    let moved = changed(&before, &after);
    assert!(
        moved > MOVED,
        "the checkbox holds `Yes` and its picture is the picture of an empty box: {moved} of it \
         changed"
    );

    // And it stays. The failure this is written for reverts the picture a few frames later.
    stage.wait(SETTLED);
    stage.repaint();
    assert_eq!(
        picture(&stage, checkbox),
        after,
        "the checkbox emptied itself again while it was still ticked"
    );

    // Clicking it again puts both the signal and the picture back.
    let at = stage.centre_of(checkbox);
    click_and_stand_back(&mut stage, at);
    assert_eq!(bound.ticked.taken().get_untracked(), Checked::No);
    assert_eq!(
        picture(&stage, checkbox),
        before,
        "the checkbox reports itself unticked and is still drawn ticked"
    );
}

#[test]
fn a_bound_switch_slides_its_thumb_and_moves_the_signal_it_was_bound_to() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let switch = bound.built.node(1);
    let before = picture(&stage, switch);

    let at = stage.centre_of(switch);
    click_and_stand_back(&mut stage, at);

    assert!(
        bound.switched.taken().get_untracked(),
        "the click never reached the signal the switch was bound to"
    );
    let after = picture(&stage, switch);
    let moved = changed(&before, &after);
    assert!(
        moved > MOVED,
        "the switch reports itself on and is painted exactly as it was off: {moved} of it changed"
    );

    stage.wait(SETTLED);
    stage.repaint();
    assert_eq!(
        picture(&stage, switch),
        after,
        "the switch went back to looking off while it was still on"
    );
}

#[test]
fn a_bound_radio_group_marks_the_item_that_was_clicked_and_clears_the_other() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let monthly = bound.built.node(2);
    let yearly = bound.built.node(3);
    let chosen_before = picture(&stage, monthly);
    let other_before = picture(&stage, yearly);

    let at = stage.centre_of(yearly);
    click_and_stand_back(&mut stage, at);

    assert_eq!(
        bound.plan.taken().get_untracked(),
        "yearly",
        "the click never reached the signal the group was bound to"
    );
    let taken = changed(&other_before, &picture(&stage, yearly));
    assert!(
        taken > MOVED,
        "the group holds `yearly` and the yearly item is drawn exactly as it was unchosen: \
         {taken} of it changed"
    );
    let dropped = changed(&chosen_before, &picture(&stage, monthly));
    assert!(
        dropped > MOVED,
        "the choice moved off the monthly item and the monthly item still has its dot: {dropped} \
         of it changed"
    );
}

#[test]
fn a_bound_toggle_takes_its_pressed_fill_and_moves_the_signal_it_was_bound_to() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let toggle = bound.built.node(4);
    let before = picture(&stage, toggle);

    let at = stage.centre_of(toggle);
    click_and_stand_back(&mut stage, at);

    assert!(
        bound.bold.taken().get_untracked(),
        "the click never reached the signal the toggle was bound to"
    );
    let after = picture(&stage, toggle);
    let moved = changed(&before, &after);
    assert!(
        moved > MOVED,
        "the toggle reports itself pressed and is painted exactly as it was at rest: {moved} of \
         it changed"
    );

    stage.wait(SETTLED);
    stage.repaint();
    assert_eq!(
        picture(&stage, toggle),
        after,
        "the toggle let go of its fill while it was still pressed"
    );
}

#[test]
fn a_bound_toggle_group_moves_its_fill_from_one_item_to_the_other() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let left = bound.built.node(5);
    let right = bound.built.node(6);
    let left_before = picture(&stage, left);
    let right_before = picture(&stage, right);

    let at = stage.centre_of(right);
    click_and_stand_back(&mut stage, at);

    assert_eq!(
        bound.align.taken().get_untracked(),
        vec!["right".to_owned()],
        "the click never reached the signal the group was bound to"
    );
    let taken = changed(&right_before, &picture(&stage, right));
    assert!(
        taken > MOVED,
        "the group holds `right` and the right item is drawn exactly as it was off: {taken} of it \
         changed"
    );
    let dropped = changed(&left_before, &picture(&stage, left));
    assert!(
        dropped > MOVED,
        "a single-selection group moved on and the left item is still filled: {dropped} of it \
         changed"
    );
}

#[test]
fn a_bound_slider_moves_its_fill_for_a_press_a_drag_and_a_key() {
    let bound = Bound::new();
    let mut stage = staged!(bound.view());
    let slider = bound.built.node(7);
    let volume = bound.volume.taken();
    assert_eq!(volume.get_untracked(), 40.0, "the slider starts at forty");
    let before = picture(&stage, slider);

    // A press three quarters along, which is a value no rounding can confuse with either end.
    let at = stage.along(slider, 0.75);
    click_and_stand_back(&mut stage, at);
    let pressed = volume.get_untracked();
    assert!(
        pressed > 60.0 && pressed < 90.0,
        "a press three quarters along the track left the bound signal at {pressed}"
    );
    let after_press = picture(&stage, slider);
    let moved = changed(&before, &after_press);
    assert!(
        moved > MOVED,
        "the slider holds {pressed} and the track is painted exactly as it was at forty: {moved} \
         of it changed"
    );

    // A drag: the press captures the pointer, so what follows carries the value with it.
    stage.move_to(stage.along(slider, 0.2));
    stage.press();
    stage.move_to(stage.along(slider, 0.95));
    let dragged = volume.get_untracked();
    stage.release();
    stage.leave();
    stage.wait(SETTLED);
    stage.repaint();
    assert!(
        dragged > 85.0,
        "the pointer was dragged to the far end of the track and the signal reads {dragged}"
    );
    let after_drag = picture(&stage, slider);
    let dragged_change = changed(&after_press, &after_drag);
    assert!(
        dragged_change > MOVED,
        "the drag moved the value to {dragged} and the fill did not follow it: {dragged_change} \
         of the track changed"
    );

    // And the keyboard, which is the same value through a different door.
    stage.click(stage.centre_of(slider));
    stage.leave();
    stage.wait(SETTLED);
    stage.repaint();
    let before_home = picture(&stage, slider);
    let at_middle = volume.get_untracked();
    stage.press_named(NamedKey::Home);
    stage.wait(SETTLED);
    stage.repaint();
    assert_eq!(
        volume.get_untracked(),
        0.0,
        "Home did not take the bound signal to the minimum"
    );
    let after_home = changed(&before_home, &picture(&stage, slider));
    assert!(
        after_home > MOVED,
        "the slider went from {at_middle} to nought and the emptied track was not repainted: \
         {after_home} of it changed"
    );
}
