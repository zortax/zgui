//! The register's rows, and the probe each one is checked by.

use crate::parity::engine::{EngineStatus, status_of};
use crate::parity::gap::{inherited_svg, scrollbar_gutter, text_decoration};
use crate::parity::probe::selector_is_accepted;
use crate::parity::record::Registration;

/// One thing this build cannot do.
///
/// ```
/// use zgui_css::parity::{GAPS, GapStatus};
///
/// let has = GAPS.iter().find(|gap| gap.subject == ":has()").expect("a seeded row");
/// assert_eq!(has.status, GapStatus::OutOfReach);
/// assert!(!has.instead.is_empty(), "a row an author can act on");
/// assert!(has.holds(), "the row is still true of this build");
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Gap {
    /// What is missing, spelled the way an author would write it.
    pub subject: &'static str,
    /// Why it is missing from this build.
    pub reason: &'static str,
    /// What an application should write instead.
    ///
    /// The half of a register row that is of any use to the person who has just found out that
    /// what they wrote does nothing. A row that says only *why* a feature is missing leaves them
    /// with the same page and no way forward; this says what to reach for instead, in the same
    /// framework, today.
    pub instead: &'static str,
    /// What closing it would take.
    pub patch: &'static str,
    /// The crate that would carry the fix.
    pub owner: &'static str,
    /// How far out of reach it currently is.
    pub status: GapStatus,
    /// How the row proves it is still true.
    pub probe: GapProbe,
}

impl Gap {
    /// Whether this build still behaves the way the row says it does.
    ///
    /// A row that stops holding is not good news to be ignored — it means the thing became
    /// reachable and nothing downstream was told, so the register has to fail rather than quietly
    /// describe a build that no longer exists.
    pub fn holds(&self) -> bool {
        match self.probe {
            GapProbe::SelectorRejected(selector) => !selector_is_accepted(selector),
            GapProbe::LonghandsUnknown(rows) => rows
                .iter()
                .all(|row| status_of(&row.css_name()) == EngineStatus::Unknown),
        }
    }
}

/// How far out of reach a gap is, which is also whether it is work or a boundary.
///
/// The distinction is a decision and not a measurement: parity is defined as parity with what the
/// style engine and the vector stack actually support, so anything that would need a patched build
/// of the engine is **outside** what this framework undertakes to do, rather than a thing it has
/// not got round to. Counting the two together produces a backlog that can never be finished and a
/// number that says nothing about what is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GapStatus {
    /// Closing it would need a patched or vendored build of the style engine, and there is to be
    /// none. Accepted as the boundary; not tracked as debt.
    OutOfReach,
    /// Closing it is this framework's own work, with the engine exactly as it stands.
    NotYetImplemented,
    /// The engine's half is out of reach; there would be work here as well if it ever arrived.
    OutOfReachThenWork,
}

impl GapStatus {
    /// Whether the boundary, rather than the backlog, is what this row describes.
    pub fn is_out_of_reach(self) -> bool {
        matches!(self, Self::OutOfReach | Self::OutOfReachThenWork)
    }

    /// The row's standing in a word, for a document that groups by it.
    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfReach => "out of reach",
            Self::NotYetImplemented => "not yet implemented",
            Self::OutOfReachThenWork => "out of reach, and work here after that",
        }
    }
}

/// How a row proves it is still true of this build.
///
/// Both variants ask the engine rather than describing it, which is the difference between a
/// register that measures parity and a register that asserts it.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum GapProbe {
    /// This selector must still be rejected, taking its whole rule with it.
    SelectorRejected(&'static str),
    /// The parser must still not know the name of any of these longhands.
    LonghandsUnknown(&'static [Registration]),
}

/// The known-unreachable set.
pub static GAPS: &[Gap] = &[
    Gap {
        subject: ":has()",
        reason: "the servo selector parser answers `false` for relative selectors outright, with no \
                 preference behind it, so the rule is reported as an unexpected identifier and \
                 dropped whole — every declaration inside it with it",
        instead: "put the condition where the view already knows it: a component that renders a \
                  child conditionally knows it is doing so, so give the parent a class in the same \
                  expression — `class:has-icon=move || icon.is_some()` — and write the rule against \
                  that class. A parent that has to react to something further down hands a signal \
                  down and the child sets it",
        patch: "make the parser accept it, and add the invalidation hook that makes a match \
                survive a mutation",
        owner: "the style engine",
        status: GapStatus::OutOfReach,
        probe: GapProbe::SelectorRejected("box:has(> label)"),
    },
    Gap {
        subject: ":nth-child(An+B of S)",
        reason: "the selector-list form is hardcoded off in the same place and in the same way as \
                 `:has()`, so the `of` keyword is an unexpected token and the rule is dropped whole",
        instead: "count in the view rather than in the sheet: a list that renders its own items \
                  knows each one's index, so it can set a class on the ones a rule is meant to \
                  reach. `:nth-child()` without `of` is available and covers striping and every \
                  other position-only rule",
        patch: "make the parser accept it, and add the invalidation hook",
        owner: "the style engine",
        status: GapStatus::OutOfReach,
        probe: GapProbe::SelectorRejected("box:nth-child(2 of .item)"),
    },
    Gap {
        subject: "::first-line",
        reason: "two independent halves. The parser has no such pseudo-element, so the rule is \
                 dropped; and a first line's identity is known only after breaking, while its style \
                 changes shaping, so honouring it is a re-shape-after-break fixpoint rather than a \
                 restyle",
        instead: "a lead-in that is written as its own element — a `<text class=\"lede\">` holding \
                  the first sentence — is styled by an ordinary class and needs no pseudo-element. \
                  A drop capital is the same shape of answer: one element, floated, sized",
        patch: "add the pseudo-element to the parser and index it eagerly; then a bounded \
                re-shape-after-break loop in the inline formatting context, which is the gating half",
        owner: "the style engine, then zgui-layout",
        status: GapStatus::OutOfReachThenWork,
        probe: GapProbe::SelectorRejected("box::first-line"),
    },
    Gap {
        subject: "SVG paint: the whole inherited-SVG property group",
        reason: "all twenty-one longhands are present in the engine's sources but generated only \
                 for another engine, so the group is not an active one in this build and every \
                 declaration using one is dropped at parse time",
        instead: "say it on the drawing rather than in the sheet: `fill`, `stroke` and the rest are \
                  read from the vector document's own attributes, so an icon that has to take a \
                  colour from its surroundings takes it as a property of the view that renders it — \
                  which is what the icon set ships and what `--zgui-*` custom properties feed",
        patch: "generate the group for this engine, then read the properties where vector content \
                is emitted",
        owner: "the style engine, then zgui-paint",
        status: GapStatus::OutOfReach,
        probe: GapProbe::LonghandsUnknown(inherited_svg::REGISTERED),
    },
    Gap {
        subject: "text-decoration-thickness, text-underline-offset, text-underline-position",
        reason: "the line, its style and its colour are all generated and read; the three                  properties that say how thick it is and where it sits are generated only for                  another engine, so their names are unknown to the parser and a declaration using                  one is dropped whole",
        instead: "`text-decoration-line`, `-style` and `-color` are all read, and the line is drawn \
                  against the face's own metrics — which is where a browser starts from too. An \
                  underline that has to sit somewhere else is a border or a box of its own under \
                  the run",
        patch: "generate the three for this engine, then read them where a decoration is measured                 against the face's own metrics",
        owner: "the style engine, then zgui-text-style",
        status: GapStatus::OutOfReach,
        probe: GapProbe::LonghandsUnknown(text_decoration::REGISTERED),
    },
    Gap {
        subject: "scrollbar-gutter",
        reason: "stopping the window scrolling behind a modal surface means `overflow: hidden` on                  the root, which takes the scrollbar away and gives its gutter back to the                  content — so the page re-wraps and jumps sideways on the frame a dialog opens.                  The property that reserves the gutter is generated only for another engine, so                  its name is unknown to the parser and a declaration using it is dropped whole;                  layout's own scroll lock, which keeps the gutter a container was already                  reserving, is reachable from nothing above layout",
        instead: "keep the gutter yourself while a surface is up: `padding-right` on the scrolling \
                  element of the width the scrollbar reserves, applied by the same class that locks \
                  the scroll. A page whose scroll region is inside a fixed-size frame — which is \
                  what a desktop window usually is — never gives the gutter back and needs nothing",
        patch: "generate the property for this engine and read it where a scroll region reserves                 its gutter, or expose layout's existing lock through the view layer so a modal                 surface can take it",
        owner: "the style engine, then zgui-layout and zgui-view",
        status: GapStatus::OutOfReachThenWork,
        probe: GapProbe::LonghandsUnknown(scrollbar_gutter::REGISTERED),
    },
];
