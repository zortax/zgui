//! Whether a property that only decides *when* something happens actually decides it.
//!
//! # The blind spot this closes
//!
//! Every other probe here lays a document out twice and compares the result. A document laid out
//! once has no clock in it, so nothing that describes change over time can move anything: an
//! animation that has not been ticked is an animation at time zero, and time zero is where the
//! unprobed document is too. Seventeen longhands were classified *unread* on exactly that evidence
//! while the engine ran every one of them.
//!
//! So this runs the animation stage itself. A document is styled, a change is applied to start the
//! transitions, and then the clock is moved to a series of moments — with the tick, the animation
//! cascade and the resulting computed values recorded at each. Two documents differing only in one
//! motion longhand are put through the same series, and the property has an effect exactly when the
//! two series differ.
//!
//! # Why the record is the tick's own report
//!
//! What an animation produces is a value, and the values a repaint-only animation produces are
//! never in the fragment tree at all — the whole design of the cheap tier is that they go into the
//! node's own override column and are composed over a shared lowering when the frame is painted. A
//! comparison over the tree would therefore see nothing move for `opacity` however long it ran.
//! The tick's own report is where those values are, so that is what is compared, alongside the
//! computed styles that the animation cascade rewrote.

use core::fmt::Write as _;
use std::sync::Arc;

use zgui_css::parity::observe;
use zgui_dom::{Document, NodeKind};
use zgui_geom::CssPx;
use zgui_interned::{ClassName, ElementName};
use zgui_style::{AnimationTime, SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_testkit_scene::FixedMetrics;

use crate::evidence::probe::Verdict;

/// The moments the clock is moved to, in seconds.
///
/// Chosen against the four-second durations in the fixture: one before any delay has run out, three
/// spread across the active interval, one after a single iteration has finished and one after a
/// third would have. A property that only changes *where the value is halfway through* and one that
/// only changes *whether anything is left at the end* are both visible in that series, and neither
/// is visible in a shorter one.
const SAMPLES: &[f64] = &[0.5, 1.0, 2.0, 3.0, 5.0, 13.0];

/// The style sheet every timed run starts from.
///
/// Three elements. An animation and a transition are started by different things — an animation by
/// the cascade naming it, a transition by a value changing after the element has already been
/// styled once — so each has one of its own. Both move `opacity`, which the cheap tier interpolates
/// into the tick's own report; a property the cascade has to run again for would be visible only
/// through the computed styles, and this way both records carry something.
///
/// The third transitions `align-items`, which is discrete: nothing transitions a discrete property
/// unless `transition-behavior: allow-discrete` is written, so this is the only element on which
/// that property can be seen to do anything. `visibility` looks like the obvious choice and is not
/// one — the engine interpolates it by its own special rule rather than discretely.
const SHEET: &str = "\
root { display: block; width: 400px; height: 300px }
@keyframes fade { from { opacity: 0.1 } to { opacity: 0.9 } }
.box { display: block; width: 50px; height: 20px }
.anim { animation-name: fade; animation-duration: 4s }
.trans { transition-property: opacity; transition-duration: 4s }
.disc { transition-property: align-items; transition-duration: 4s }
";

/// The declaration that starts the transitions, installed after the first styling pass.
///
/// Both halves of the change are here: the interpolable one, which transitions on its own, and the
/// discrete one, which transitions only where `transition-behavior: allow-discrete` says it may.
const TRIGGER: &str = ".trans { opacity: 0.5 }\n.disc { align-items: center }\n";

/// One property, and a declaration that sets it to something other than its initial value.
///
/// The same shape as an ordinary [`Probe`](crate::evidence::Probe), and deliberately: a row means
/// the same thing under either, and only the instrument it is run through differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedProbe {
    /// The longhand's Rust spelling, which is how a register row is keyed.
    pub property: &'static str,
    /// The declaration written into the probed document only, as a style sheet writes it.
    pub declaration: &'static str,
}

impl TimedProbe {
    /// A timed probe for one property.
    pub const fn new(property: &'static str, declaration: &'static str) -> Self {
        Self {
            property,
            declaration,
        }
    }

    /// The property's name as a style sheet writes it.
    pub fn css_name(&self) -> String {
        self.property.replace('_', "-")
    }

    /// Runs the fixture with and without the declaration and says how the two differ.
    pub fn run(&self) -> Verdict {
        let css_name = self.css_name();
        let before = run("", &css_name);
        let after = run(&format!(".probe {{ {} }}\n", self.declaration), &css_name);
        if before.values == after.values {
            return Verdict::Inert;
        }
        if before.record == after.record {
            return Verdict::Unchanged;
        }
        Verdict::Changed
    }
}

/// One timed probe per longhand that describes change over time.
///
/// Every animation and transition longhand the engine generates is here, including the four nothing
/// reads: a row claiming a property is unread is a claim this instrument can contradict, and leaving
/// those four out would be leaving the claim untested.
pub static PROBES: &[TimedProbe] = &[
    TimedProbe::new("animation_composition", "animation-composition: add"),
    TimedProbe::new("animation_delay", "animation-delay: 3s"),
    TimedProbe::new("animation_direction", "animation-direction: reverse"),
    TimedProbe::new("animation_duration", "animation-duration: 1s"),
    TimedProbe::new("animation_fill_mode", "animation-fill-mode: both"),
    TimedProbe::new("animation_iteration_count", "animation-iteration-count: 3"),
    TimedProbe::new("animation_name", "animation-name: none"),
    TimedProbe::new("animation_play_state", "animation-play-state: paused"),
    TimedProbe::new("animation_range_end", "animation-range-end: 50%"),
    TimedProbe::new("animation_range_start", "animation-range-start: 25%"),
    TimedProbe::new("animation_timeline", "animation-timeline: none"),
    TimedProbe::new(
        "animation_timing_function",
        "animation-timing-function: ease-in",
    ),
    TimedProbe::new("transition_behavior", "transition-behavior: allow-discrete"),
    TimedProbe::new("transition_delay", "transition-delay: 3s"),
    TimedProbe::new("transition_duration", "transition-duration: 1s"),
    TimedProbe::new("transition_property", "transition-property: none"),
    TimedProbe::new(
        "transition_timing_function",
        "transition-timing-function: ease-in",
    ),
];

/// What every timed probe showed, by the name a style sheet writes.
pub fn survey() -> Vec<(String, Verdict)> {
    PROBES
        .iter()
        .map(|probe| (probe.css_name(), probe.run()))
        .collect()
}

/// What one timed run produced.
struct Run {
    /// The tick reports and computed styles at every sampled moment, as text.
    record: String,
    /// The probed property's computed value on every element, so an inert declaration is caught.
    values: Vec<Option<String>>,
}

/// Styles the fixture with `extra`, starts its transitions, and moves the clock through [`SAMPLES`].
fn run(extra: &str, probed: &str) -> Run {
    zgui_css::enable_css_features();

    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    for classes in [["box", "anim"], ["box", "trans"], ["box", "disc"]] {
        let child = document.append(root, NodeKind::Element, ElementName::new("div"));
        let names: Vec<ClassName> = classes
            .iter()
            .chain(["probe"].iter())
            .map(|class| ClassName::new(class))
            .collect();
        document.set_classes(child, &names);
    }

    let mut engine = StyleEngine::new(
        &document,
        Arc::new(FixedMetrics::new()),
        Viewport::new(CssPx(400.0), CssPx(300.0)),
    );
    let mut sheets = Vec::new();
    for (origin, text) in [
        (SheetOrigin::UserAgent, crate::zdoc::build::DISPLAY_DEFAULTS),
        (SheetOrigin::Author, SHEET),
        (SheetOrigin::Author, extra),
    ] {
        let (handle, diagnostics) = engine.add_sheet(&document, origin, SheetSource::Text(text));
        assert!(
            diagnostics.is_empty(),
            "the engine dropped part of a timed fixture sheet: {diagnostics:?}",
        );
        sheets.push(handle);
    }
    engine.restyle(&mut document, None);

    // The transitions start here, at the second styling pass: a transition is a value that
    // *changed*, so the element needs a previous value to have changed from.
    let (trigger, diagnostics) =
        engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(TRIGGER));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    sheets.push(trigger);
    engine.restyle(&mut document, None);

    // Read before the clock moves, because it answers a question about the *cascade*: whether the
    // declaration reached a computed style at all. A probe whose declaration was dropped, misspelled
    // or already the initial value shows nothing either way, and counting that as "no effect" would
    // turn a typo into a parity claim.
    let values = styles(&document)
        .iter()
        .map(|style| observe::computed_value(style, probed))
        .collect();

    let mut record = String::new();
    for sample in SAMPLES {
        let report = engine.animation_tick(&document, AnimationTime(*sample));
        let _ = writeln!(record, "t={sample}");
        let mut edges: Vec<String> = report
            .edges
            .iter()
            .map(|edge| {
                format!(
                    "  edge {:?} {:?} {} {:?} {:?}",
                    edge.kind, edge.lifecycle, edge.name, edge.pseudo, edge.elapsed,
                )
            })
            .collect();
        edges.sort();
        for edge in edges {
            let _ = writeln!(record, "{edge}");
        }
        let mut rows: Vec<String> = report
            .elements
            .iter()
            .map(|element| {
                format!(
                    "  row {:?} {:?} advancing={} crossed={} placement={:?}",
                    element.properties,
                    element.values,
                    element.advancing,
                    element.crossed,
                    element.placement,
                )
            })
            .collect();
        rows.sort();
        for row in rows {
            let _ = writeln!(record, "{row}");
        }
        // An element whose animation the cheap tier cannot express is re-cascaded, which is where
        // its value comes from — so the styles are asked for after the tick that owed the cascade.
        for element in &report.elements {
            engine.mark_animation_restyle(&document, element.index);
        }
        engine.restyle(&mut document, None);
        for style in styles(&document) {
            let _ = writeln!(
                record,
                "  style opacity={:?} align-items={:?}",
                observe::computed_value(&style, "opacity"),
                observe::computed_value(&style, "align-items"),
            );
        }
    }

    Run { record, values }
}

/// Every element's computed style, in tree order.
fn styles(document: &Document) -> Vec<zgui_css::ComputedStyle> {
    let mut out = Vec::new();
    collect(document.document_node(), &mut out);
    out
}

/// One node's computed style and those of everything below it.
fn collect(node: zgui_dom::Node<'_>, out: &mut Vec<zgui_css::ComputedStyle>) {
    if let Some(style) = node.primary_style() {
        out.push(style);
    }
    let mut child = node.first_child_node();
    while let Some(current) = child {
        collect(current, out);
        child = current.next_sibling_node();
    }
}
