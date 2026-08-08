//! Running one property's probe and reading the answer.

use zgui_css::parity::observe;

use crate::evidence::fixture;
use crate::fragment;
use crate::zdoc::build::lay_out;

/// One property, and a declaration that sets it to something other than its initial value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// The longhand's Rust spelling, which is how a register row is keyed.
    pub property: &'static str,
    /// Declarations written into *both* documents, so that they are not what the comparison sees.
    ///
    /// Empty for nearly every property. It exists because a few longhands compute to their initial
    /// value however they are written until a second one is set — a border width is zero while the
    /// border style is `none`, and an outline width is zero while the outline style is — so a probe
    /// for one of those has to turn the other on. Putting that declaration in *both* documents is
    /// what keeps the probe a witness for the property it names: written into the probed document
    /// only, it would move the fragment tree by itself and every one of these rows would read
    /// *proven* whether or not anything read the width.
    pub context: &'static str,
    /// The declaration written into the probed document only, as a style sheet writes it.
    pub declaration: &'static str,
}

/// What running a probe showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The declaration changed something a stage after layout reads.
    Changed,
    /// The cascade took the declaration and nothing downstream did anything with it.
    Unchanged,
    /// The declaration never reached a computed style, so it shows nothing either way.
    ///
    /// A misspelled value, a property this build does not generate, or a declaration the parser
    /// dropped. Always a fault in the probe rather than a finding about the property.
    Inert,
}

/// The unprobed fixture, laid out once so that every probe can be compared against it.
pub struct Baseline {
    /// The fragment tree it produced.
    tree: String,
    /// Its computed styles.
    styles: Vec<zgui_css::ComputedStyle>,
}

impl Baseline {
    /// Lays the unprobed fixture out.
    pub fn take() -> Self {
        Self::of(&fixture::baseline())
    }

    /// Lays one particular document out as the side a probe is compared against.
    fn of(document: &crate::zdoc::Zdoc) -> Self {
        let laid = lay_out(document);
        Self {
            tree: rendering(&laid),
            styles: laid.styles(),
        }
    }
}

impl Probe {
    /// A probe for one property, against the unprobed fixture.
    pub const fn new(property: &'static str, declaration: &'static str) -> Self {
        Self {
            property,
            context: "",
            declaration,
        }
    }

    /// A probe for one property, against a fixture that already carries `context`.
    ///
    /// Both documents get `context`; only the probed one gets `declaration`. The verdict is
    /// therefore about `declaration` alone, which is the whole point — see [`Probe::context`].
    pub const fn in_context(
        property: &'static str,
        context: &'static str,
        declaration: &'static str,
    ) -> Self {
        Self {
            property,
            context,
            declaration,
        }
    }

    /// The property's name as a style sheet writes it.
    pub fn css_name(&self) -> String {
        self.property.replace('_', "-")
    }

    /// Lays the fixture out with the declaration and compares it against a fresh baseline.
    pub fn run(&self) -> Verdict {
        self.run_against(&Baseline::take())
    }

    /// The same, against a baseline that has already been laid out.
    ///
    /// The shared baseline is used only by a probe with no context; one that has a context needs a
    /// baseline carrying that context, and lays its own out.
    pub fn run_against(&self, baseline: &Baseline) -> Verdict {
        if self.context.is_empty() {
            return self.compare(baseline, &fixture::probed(self.declaration));
        }
        let before = Baseline::of(&fixture::probed(self.context));
        self.compare(
            &before,
            &fixture::probed(&format!("{}; {}", self.context, self.declaration)),
        )
    }

    /// Lays `after` out and says how it differs from `before`.
    fn compare(&self, before: &Baseline, after: &crate::zdoc::Zdoc) -> Verdict {
        let css_name = self.css_name();
        let after = lay_out(after);

        if values(&before.styles, &css_name) == values(&after.styles(), &css_name) {
            return Verdict::Inert;
        }
        if before.tree == rendering(&after) {
            return Verdict::Unchanged;
        }
        Verdict::Changed
    }
}

/// Everything about a laid-out document that a stage after layout reads.
///
/// Five outputs of the same document, because a property that moves any one of them has had an
/// effect: the fragment tree, the hit answers, the clip chains, what was shaped, and what each
/// element's style lowers to for painting. The last three are what make a corner radius, a case
/// transform and a colour answerable at all — none of them moves an edge or a hit answer, so a
/// harness that compared only the first two would report the whole painting vocabulary, and every
/// property that changes which characters are shaped, as consumed by nobody.
fn rendering(laid: &crate::zdoc::build::Laid) -> String {
    format!(
        "{}{}{}{}{}",
        fragment::full(&laid.store),
        fragment::hit_answers(laid),
        fragment::clip_chains(laid),
        fragment::inline_text(&laid.store),
        painting(&laid.styles()),
    )
}

/// What every element's computed style lowers to for painting, as stable text.
///
/// The lowering is the paint stage's whole reading of a computed style: everything that stage takes
/// from the cascade passes through it, and nothing that does not reach it can be drawn. Comparing it
/// is therefore the same question the fragment tree answers for layout, asked of the other consumer.
fn painting(styles: &[zgui_css::ComputedStyle]) -> String {
    let mut out = String::new();
    for style in styles {
        out.push_str(&format!("{:?}\n", zgui_paint::lower(style, 1.0)));
    }
    out
}

/// One property's computed value on every element of a laid-out document.
///
/// Read off every element rather than off the probed one, because finding "the probed one" would
/// need a second way of naming elements, and a mismatch between the two would look like a property
/// with no effect.
fn values(styles: &[zgui_css::ComputedStyle], css_name: &str) -> Vec<Option<String>> {
    styles
        .iter()
        .map(|style| observe::computed_value(style, css_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Probe, Verdict};

    /// The three answers are all reachable, and each one for the right reason.
    ///
    /// Without this the verdict would be a shape nobody had seen produce anything but its
    /// happiest value, and a probe runner that could only ever answer `Changed` would confirm
    /// every claim it was given.
    #[test]
    fn each_verdict_is_reachable() {
        assert_eq!(
            Probe::new("width", "width: 17px").run(),
            Verdict::Changed,
            "a width that no box takes is not a width",
        );
        assert_eq!(
            Probe::new("font_kerning", "font-kerning: none").run(),
            Verdict::Unchanged,
            "the deterministic shaper has no kerning to switch off",
        );
        assert_eq!(
            Probe::new("mask_type", "mask-type: luminance").run(),
            Verdict::Inert,
            "a declaration that computes to the initial value moves nothing to observe",
        );
    }

    /// A context is written into both documents, so it is never itself what the verdict saw.
    ///
    /// This is the control for [`Probe::in_context`], and it is the assertion that keeps ten
    /// *proven* rows from being vacuous. `border-bottom-style: solid` moves every edge below it by
    /// three pixels all on its own: if the context leaked into the probed document only, every
    /// probe carrying one would read `Changed` whatever became of the property it names. The row
    /// asserted against here is one nothing reads, so `Changed` could only have come from the
    /// context.
    #[test]
    fn a_context_is_present_on_both_sides_of_the_comparison() {
        let context = "border-bottom-style: solid";
        assert_eq!(
            Probe::in_context("border_image_outset", context, "border-image-outset: 3px").run(),
            Verdict::Unchanged,
            "the context moved the tree by itself, so no probe using one proves anything",
        );
        assert_eq!(
            Probe::new("border_image_outset", "border-image-outset: 3px").run(),
            Verdict::Unchanged,
            "the same property without a context, so the two answers are comparable",
        );
        assert_eq!(
            Probe::in_context("border_bottom_width", context, "border-bottom-width: 7px").run(),
            Verdict::Changed,
            "a width read by layout still shows through, on top of the context",
        );
    }
}
