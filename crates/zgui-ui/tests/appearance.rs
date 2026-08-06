//! What a control *looks like* while it is being used, read off the graphics device.
//!
//! # Why these are not the assertions beside them
//!
//! Every other fixture in this package answers a question about the document: did the state bit get
//! written, was the callback invoked, does the accessibility tree say `checked`. All of those can
//! be true of a window in which nothing on the screen ever moves, and that is not a hypothetical —
//! it is the shape of the defect this file was written for. A hover wrote `:hover`, the cascade
//! matched the hover rule, the transition interpolated to the hover colour, and the element was
//! then painted back at its resting colour for as long as the pointer stayed on it, because the
//! style that interpolation was composed over still held the value it had set off from.
//!
//! So nothing here reads the document. A control is found, the pointer is driven over it the way a
//! mouse drives one, the clock is moved on the way an output moves it, and the pixels the device
//! composed are asked what colour they are.
//!
//! # Two things the fixture itself has to get right
//!
//! **The tokens.** Every fixture below is wrapped in a [`ThemeProvider`]. Without one,
//! `var(--zui-motion-duration-fast)` resolves to nothing, the `transition` declaration is invalid at
//! computed-value time, and no transition is created at all — so a fixture that left the provider
//! out would drive the pointer correctly, assert correctly, and exercise none of the machinery that
//! was broken.
//!
//! **The clock.** A transition is sampled once per frame, so time is moved on in output-sized steps
//! and the picture is read after it has stopped moving. A fixture that asserted at the moment of the
//! click would agree with a control that reverts four frames later, which is exactly what one did.

mod desktop;
mod device;
mod painted;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use zgui::geom::{Device, DevicePx, Point};
use zgui::prelude::UnsyncCallback;
use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};

/// The page every fixture is laid out on: a flat white surface with room around the controls.
///
/// The slider is given a width because it asks for all of its parent's, and a flex column that
/// aligns its items to the start gives it none — which is a slider whose track is a few pixels
/// wide and whose fill nothing can be measured in.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 32px; gap: 32px; align-items: flex-start }
                     .track { width: 240px }";

/// How dark every channel of a pixel has to be before it counts as the solid fill.
///
/// A checked switch, a ticked checkbox and the travelled part of a slider are all painted in
/// `--zui-color-primary`, which is black on a light page. Everything else inside those controls is
/// a light neutral — the page behind them, the track they sit on, the border round them, and the
/// tick drawn *on* the fill in its foreground colour — and the nearest of those is above two
/// hundred. There is no value in between for a rounding error to land on.
const FILL_CEILING: u8 = 96;

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

/// Where a fixture leaves the references it built, so a test can find the elements it made.
///
/// A control is not reliably findable by what it says: a switch says nothing at all, and the node
/// whose text is a button's label is the *text*, whose box is the glyphs rather than the fill. So
/// each fixture binds a reference and the test reads the element back out of it.
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
    /// Panics before the view has been built, and for a control that never bound its reference —
    /// either of which would otherwise leave the assertions below measuring an arbitrary element.
    fn node(&self, which: usize) -> NodeId {
        self.0.borrow()[which]
            .get_untracked()
            .expect("the control bound its reference when it was built")
    }
}

/// What fraction of the pixels inside `node` are painted in the solid fill.
fn filled_fraction(stage: &Stage, node: NodeId) -> f32 {
    let colours = stage.colours_in(stage.rect_of(node));
    if colours.is_empty() {
        return 0.0;
    }
    let filled = colours
        .iter()
        .filter(|(red, green, blue)| {
            *red <= FILL_CEILING && *green <= FILL_CEILING && *blue <= FILL_CEILING
        })
        .count();
    filled as f32 / colours.len() as f32
}

/// A point on a control's fill rather than on whatever it says.
///
/// The middle of a button is where its text is, and text is the one part of a control whose colour
/// a background rule does not decide. Eight per cent along is inside the horizontal padding, which
/// is fill in every variant and every size.
fn on_the_fill(stage: &Stage, node: NodeId) -> Point<DevicePx, Device> {
    stage.along(node, 0.08)
}

// ---- hover ------------------------------------------------------------------------------------

/// A secondary button and somewhere beside it for the pointer to go.
fn buttons(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let button = NodeRef::new();
        built.keep(&[button]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Button(node_ref = button, variant = ButtonVariant::Secondary) {"Secondary"}
                    text {"elsewhere"}
                }
            }
        })
    }
}

#[test]
fn a_hovered_button_takes_its_hover_colour_and_holds_it_while_the_pointer_stays() {
    let built = Built::default();
    let mut stage = staged!(buttons(&built));
    let button = built.node(0);
    let sample = on_the_fill(&stage, button);
    let elsewhere = stage
        .census()
        .control("elsewhere")
        .and_then(|seen| seen.centre())
        .expect("there is somewhere else to point at");

    // Enter.
    let at_rest = stage.colour_at(sample);
    stage.move_to(stage.centre_of(button));
    stage.wait(SETTLED);
    let hovered = stage.colour_at(sample);
    assert_eq!(
        stage.running_animations(button),
        0,
        "the transition is still running, so this reading is a frame of it rather than the \
         colour the button settled at"
    );
    assert_ne!(
        hovered, at_rest,
        "the pointer is on the button and it is painted in exactly the colour it has when \
         nothing is on it"
    );

    // Stay. Time passes and nothing else happens, which is what a pointer resting on a control
    // does — and the moment the interpolated colour used to be thrown away at.
    stage.wait(SETTLED);
    assert_eq!(
        stage.colour_at(sample),
        hovered,
        "the button changed colour on its own while the pointer had not moved"
    );

    // Stay again, with the pointer re-reporting a position inside the same control, which is what a
    // real mouse does many times a second and a scripted one does once.
    stage.move_to(on_the_fill(&stage, button));
    stage.wait(SETTLED);
    assert_eq!(
        stage.colour_at(sample),
        hovered,
        "a pointer move within the same control took the hover colour away"
    );

    // Leave.
    stage.move_to(elsewhere);
    stage.wait(SETTLED);
    assert_eq!(
        stage.colour_at(sample),
        at_rest,
        "the pointer is somewhere else and the button is still painted as though it were not"
    );
    assert_eq!(
        stage.running_animations(button),
        0,
        "the button is still animating long after the pointer left it"
    );
}

#[test]
fn taking_the_pointer_off_the_surface_puts_the_button_back_the_way_it_was() {
    let built = Built::default();
    let mut stage = staged!(buttons(&built));
    let button = built.node(0);
    let sample = on_the_fill(&stage, button);

    let at_rest = stage.colour_at(sample);
    stage.move_to(stage.centre_of(button));
    stage.wait(SETTLED);
    assert_ne!(stage.colour_at(sample), at_rest, "the button hovered");

    stage.leave();
    stage.wait(SETTLED);
    assert_eq!(
        stage.colour_at(sample),
        at_rest,
        "the pointer has left the window and the button is still lit"
    );
}

// ---- press and release --------------------------------------------------------------------------

/// The three controls whose whole appearance is the answer they hold.
struct Answers {
    /// What the switch last reported.
    switched: Rc<Cell<bool>>,
    /// What the checkbox last reported.
    ticked: Rc<Cell<Checked>>,
    /// What the slider last reported.
    slid: Rc<Cell<f64>>,
    /// The switch, the checkbox and the slider, in that order.
    built: Built,
}

impl Answers {
    /// Handles for a fixture that has not been built yet.
    fn new() -> Self {
        Self {
            switched: Rc::new(Cell::new(false)),
            ticked: Rc::new(Cell::new(Checked::No)),
            slid: Rc::new(Cell::new(0.0)),
            built: Built::default(),
        }
    }

    /// A view holding the three of them, reporting into these cells.
    fn view(&self) -> impl Fn() -> AnyView + use<> {
        let switched = Rc::clone(&self.switched);
        let ticked = Rc::clone(&self.ticked);
        let slid = Rc::clone(&self.slid);
        let built = self.built.clone();
        move || {
            let switch = NodeRef::new();
            let checkbox = NodeRef::new();
            let slider = NodeRef::new();
            built.keep(&[switch, checkbox, slider]);
            let switched = Rc::clone(&switched);
            let ticked = Rc::clone(&ticked);
            let slid = Rc::clone(&slid);
            AnyView::new(view! {
                ThemeProvider {
                    column(class = "page") {
                        Switch(
                            node_ref = switch,
                            a11y:label = "Emails",
                            on_change = UnsyncCallback::new(move |next: bool| switched.set(next))
                        )
                        Checkbox(
                            node_ref = checkbox,
                            a11y:label = "Terms",
                            on_change = UnsyncCallback::new(move |next: Checked| ticked.set(next))
                        )
                        column(class = "track") {
                            Slider(
                                node_ref = slider,
                                min = 0.0,
                                max = 100.0,
                                step = 5.0,
                                label = "Volume",
                                on_change = UnsyncCallback::new(move |next: f64| slid.set(next))
                            )
                        }
                    }
                }
            })
        }
    }
}

#[test]
fn a_switch_clicked_with_a_pointer_holds_both_its_answer_and_its_colour() {
    let answers = Answers::new();
    let mut stage = staged!(answers.view());
    let switch = answers.built.node(0);

    let before = filled_fraction(&stage, switch);
    assert!(
        before < 0.1,
        "the switch is off and already mostly filled, so nothing below can tell the two states \
         apart: {before}"
    );

    stage.click(stage.centre_of(switch));
    stage.wait(SETTLED);

    assert!(
        answers.switched.get(),
        "a press and a release over the switch did not change its value"
    );
    let after = filled_fraction(&stage, switch);
    assert!(
        after > 0.3,
        "the switch reports itself on and its track is not painted in the solid fill, so the \
         only thing that says so is invisible: {before} became {after}"
    );

    // And it stays on. The failure this is written for is one that reverts the picture a few frames
    // after the click, long after any assertion made at the moment of the click.
    stage.wait(SETTLED);
    let held = filled_fraction(&stage, switch);
    assert!(
        (held - after).abs() < 0.05,
        "the switch went back to looking off while it was still on: {after} became {held}"
    );
}

#[test]
fn a_checkbox_clicked_with_a_pointer_holds_both_its_answer_and_its_fill() {
    let answers = Answers::new();
    let mut stage = staged!(answers.view());
    let checkbox = answers.built.node(1);

    let before = filled_fraction(&stage, checkbox);
    assert!(before < 0.1, "the checkbox starts unticked: {before}");

    stage.click(stage.centre_of(checkbox));
    stage.wait(SETTLED);

    assert_eq!(
        answers.ticked.get(),
        Checked::Yes,
        "a press and a release over the checkbox did not change its value"
    );
    let after = filled_fraction(&stage, checkbox);
    assert!(
        after > 0.3,
        "the checkbox reports itself ticked and its box is not filled: {before} became {after}"
    );

    stage.wait(SETTLED);
    let held = filled_fraction(&stage, checkbox);
    assert!(
        (held - after).abs() < 0.05,
        "the checkbox emptied itself again while it was still ticked: {after} became {held}"
    );
}

#[test]
fn a_slider_pressed_along_its_track_moves_its_value_and_its_fill() {
    let answers = Answers::new();
    let mut stage = staged!(answers.view());
    let slider = answers.built.node(2);

    assert_eq!(answers.slid.get(), 0.0, "the slider starts at its minimum");
    let before = filled_fraction(&stage, slider);

    // Three quarters along, which is a value no rounding can confuse with either end.
    stage.click(stage.along(slider, 0.75));
    stage.wait(SETTLED);

    let value = answers.slid.get();
    assert!(
        value > 60.0 && value < 90.0,
        "a press three quarters along the track put the slider at {value}"
    );
    let after = filled_fraction(&stage, slider);
    assert!(
        after > before + 0.05,
        "the slider holds {value} and the length of track painted in the solid fill did not \
         move: {before} became {after}"
    );

    stage.wait(SETTLED);
    let held = filled_fraction(&stage, slider);
    assert!(
        (held - after).abs() < 0.02,
        "the slider's fill retreated on its own after the press: {after} became {held}"
    );
}

#[test]
fn dragging_a_slider_carries_its_value_with_the_pointer() {
    let answers = Answers::new();
    let mut stage = staged!(answers.view());
    let slider = answers.built.node(2);

    stage.move_to(stage.along(slider, 0.25));
    stage.press();
    let pressed = answers.slid.get();
    assert!(
        pressed > 10.0 && pressed < 40.0,
        "the press landed at {pressed}"
    );

    // The pointer is captured by the press, so what follows is a drag rather than a fresh hit.
    stage.move_to(stage.along(slider, 0.9));
    let dragged = answers.slid.get();
    stage.release();
    stage.wait(SETTLED);

    assert!(
        dragged > pressed + 30.0,
        "the pointer was dragged most of the way along the track and the value went from \
         {pressed} to {dragged}"
    );
    assert_eq!(
        answers.slid.get(),
        dragged,
        "releasing the pointer moved the value again"
    );
}

// ---- what stays lit after a surface is dismissed --------------------------------------------

/// A menubar with one menu on it, and somewhere else on the page to press.
fn menubar() -> impl Fn() -> AnyView {
    || {
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Menubar {
                        MenubarMenu(value = "file") {
                            MenubarTrigger {"File"}
                            MenubarContent {MenubarItem {"New window"}}
                        }
                    }
                    text {"elsewhere"}
                }
            }
        })
    }
}

/// The control saying `text`, rather than the text node inside it.
///
/// A box's text is everything under it, so a title on a bar and the glyphs in it both say the same
/// word. The glyphs are the deepest of them and the control is the one above — which is what has to
/// be measured here, because the fill being asked about is the control's and not the text's.
fn control_saying(stage: &Stage, text: &str) -> NodeId {
    let mut saying: Vec<_> = stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .map(|node| (node.depth, node.id))
        .collect();
    saying.sort_unstable();
    let (_, id) = saying[saying.len().saturating_sub(2)];
    id
}

#[test]
fn a_menubar_title_goes_out_again_when_its_menu_is_dismissed_from_elsewhere() {
    // A menu hands the keyboard back to the title it came from as it closes, so after a press
    // somewhere else in the window the title is the focused element with no menu under it. Lit by
    // plain `:focus` it stays filled — a bar with one heading stuck down on it, over nothing.
    let mut stage = staged!(menubar());
    let trigger = control_saying(&stage, "File");
    let sample = on_the_fill(&stage, trigger);
    let elsewhere = stage
        .census()
        .control("elsewhere")
        .and_then(|seen| seen.centre())
        .expect("there is somewhere else to press");

    let at_rest = stage.colour_at(sample);
    stage.click(stage.centre_of(trigger));
    stage.wait(SETTLED);
    let open = stage.colour_at(sample);
    assert_ne!(
        open, at_rest,
        "the menu is open and its title is painted exactly as it is when it is not"
    );

    stage.click(elsewhere);
    stage.wait(SETTLED);

    assert_eq!(
        stage.focused(),
        Some(trigger),
        "the fixture is not asking anything: the menu did not hand the keyboard back, so the \
         title would go out whatever it is lit by"
    );
    assert_eq!(
        stage.colour_at(sample),
        at_rest,
        "the menu was dismissed from elsewhere and its title is still filled"
    );
}

/// A tooltip on a button, and somewhere else for the pointer to go.
fn tooltip() -> impl Fn() -> AnyView {
    || {
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Tooltip {
                        TooltipTrigger {Button(variant = ButtonVariant::Outline) {"Save"}}
                        TooltipContent(placement = {zgui_ui_primitives::Placement::BOTTOM}) {"Saves the document"}
                    }
                    text {"elsewhere"}
                }
            }
        })
    }
}

/// The arrow's own box, out of the surface the tooltip drew.
///
/// Found by its class through the census's boxes rather than by a reference, because the arrow is
/// drawn by the content component and a caller never holds one.
fn tooltip_arrow(stage: &Stage) -> Option<zgui::geom::Rect<DevicePx, Device>> {
    // Ten pixels square turned on its corner, which is a box of ten root two, and the only thing
    // that size and that shape on this page.
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text.is_empty())
        .filter_map(|node| node.rect)
        .find(|rect| {
            (rect.size.width.0 - 14.14).abs() < 1.0 && (rect.size.height.0 - 14.14).abs() < 1.0
        })
}

#[test]
fn a_tooltip_and_its_arrow_go_out_together() {
    // The arrow is absolutely positioned, so which box it is laid out and painted against is
    // whichever ancestor positions it. With a static surface that is the positioner *around* the
    // panel, one box further out — and everything the panel does to itself as a whole, the arrow
    // does not do. The exit is where that shows: the slug fades and the diamond hangs in the air
    // at full strength until the surface is unmounted.
    let mut stage = staged!(tooltip());
    let trigger = control_saying(&stage, "Save");
    let elsewhere = stage
        .census()
        .control("elsewhere")
        .and_then(|seen| seen.centre())
        .expect("there is somewhere else to point at");

    stage.move_to(stage.centre_of(trigger));
    stage.wait(SETTLED);
    let arrow = tooltip_arrow(&stage).expect("the tooltip drew its arrow");
    let on_the_arrow = Point::new(
        DevicePx(arrow.origin.x.0 + arrow.size.width.0 / 2.0),
        DevicePx(arrow.origin.y.0 + arrow.size.height.0 / 2.0),
    );
    let shown = stage.colour_at(on_the_arrow);

    // The pointer leaves, the tooltip is asked to close, and the exit is sampled part way through
    // rather than at either end: both ends agree whatever the arrow belongs to.
    stage.move_to(elsewhere);
    stage.wait(Duration::from_millis(180));
    let leaving = stage.colour_at(on_the_arrow);

    assert_ne!(
        leaving, shown,
        "the tooltip is going and the arrow is painted in exactly the colour it had while the \
         tooltip was up, so it is not leaving with it"
    );
}
