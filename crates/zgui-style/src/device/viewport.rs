//! What the surface looks like this frame, which parts of it changed since the last one, and what
//! each of those changes invalidates.

use rustc_hash::FxHashSet;
use style::data::ViewportUnitUsage;
use style::invalidation::element::restyle_hints::RestyleHint;
use zgui_bits::Dirty;
use zgui_dom::dirty::propagate;
use zgui_dom::{Document, DocumentStore, NodeFlags, NodeIndex, NodeKey, NodeKind};
use zgui_geom::CssPx;

use crate::device::color_scheme::ColorScheme;

/// The surface a document is being styled against.
///
/// Three quantities, because three different things in a stylesheet read them: media queries read
/// all of them, viewport units read the size, and device-pixel rounding reads the ratio.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Viewport {
    /// Width of the surface, in CSS pixels.
    pub width: CssPx,
    /// Height of the surface, in CSS pixels.
    pub height: CssPx,
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// The colour scheme the surface is presented in.
    pub scheme: ColorScheme,
}

impl Viewport {
    /// A `width` by `height` surface at one device pixel per CSS pixel, in the light scheme.
    pub fn new(width: CssPx, height: CssPx) -> Self {
        Self {
            width,
            height,
            scale: 1.0,
            scheme: ColorScheme::Light,
        }
    }

    /// The same surface at `scale` device pixels per CSS pixel.
    #[must_use]
    pub fn at_scale(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// The same surface presented in `scheme`.
    #[must_use]
    pub fn in_scheme(self, scheme: ColorScheme) -> Self {
        Self { scheme, ..self }
    }

    /// What changed between `self` and `next`.
    pub(crate) fn changes_to(self, next: Self) -> ViewportChange {
        ViewportChange {
            size: self.width != next.width || self.height != next.height,
            scale: self.scale != next.scale,
            scheme: self.scheme != next.scheme,
        }
    }
}

/// Which of a surface's three quantities moved between two frames.
///
/// They are kept apart because each invalidates something different: a size change relays the
/// document out, a scale change relays it out *and* re-rounds every box to the new device pixel
/// grid, and a scheme change changes only which rules match.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct ViewportChange {
    /// The CSS-pixel size moved.
    pub(crate) size: bool,
    /// The device pixel ratio moved.
    pub(crate) scale: bool,
    /// The colour scheme moved.
    pub(crate) scheme: bool,
}

impl ViewportChange {
    /// Whether anything moved at all.
    pub(crate) fn any(self) -> bool {
        self.size || self.scale || self.scheme
    }
}

/// The elements whose computed style resolved a viewport unit, as the cascade filed them.
///
/// A size change recascades exactly these, so its cost grows with the number of readers. Each
/// restyled element is filed or removed from its own record, so the set never misses a reader. It
/// can hold an element that has gone; the probe and the sweep drop those.
#[derive(Default, Debug)]
pub(crate) struct UnitReaders {
    /// The readers, by generation-checked name.
    keys: FxHashSet<NodeKey>,
    /// How many readers the last sweep left.
    swept_to: usize,
}

/// The size below which the registry is never swept.
const SWEEP_FLOOR: usize = 64;

impl UnitReaders {
    /// Records whether `node`'s new style reads a viewport unit.
    pub(crate) fn file(&mut self, node: NodeKey, reads: bool) {
        if reads {
            self.keys.insert(node);
        } else if !self.keys.is_empty() {
            self.keys.remove(&node);
        }
    }

    /// Drops the readers that no longer exist, once the set has doubled since the last sweep.
    ///
    /// Each sweep is then paid for by at least as many filings as it examines. An element that is
    /// alive but out of the document stays: it keeps its style, and it can come back with it.
    pub(crate) fn retire(&mut self, store: &DocumentStore) {
        if self.keys.len() <= SWEEP_FLOOR.max(self.swept_to.saturating_mul(2)) {
            return;
        }
        self.keys.retain(|key| store.get(*key).is_some());
        self.swept_to = self.keys.len();
    }

    /// Whether no reader is filed.
    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// What the viewport-unit probe did.
#[derive(Copy, Clone, Default, Debug)]
pub(crate) struct UnitProbe {
    /// How many filed readers it examined.
    pub(crate) probed: usize,
    /// How many of them it marked to cascade again.
    pub(crate) marked: usize,
}

/// Marks every filed reader whose computed style still resolves a viewport unit.
///
/// Viewport units resolve at computed-value time, so no amount of relaying out fixes a stale
/// `width: 50vw` — the elements that read one have to cascade again. The cascade files those
/// elements in `readers`, so the probe examines only them: a document with no viewport units in it
/// examines and marks nothing.
///
/// The two usages are not the same obligation. A unit read from a declaration needs only the
/// cascade re-run; one read from a container query decides *which rules match*, so that element
/// has to be matched again.
pub(crate) fn invalidate_units(document: &mut Document, readers: &mut UnitReaders) -> UnitProbe {
    let probed = readers.keys.len();
    let mut marked = Vec::new();
    readers.keys.retain(|key| {
        let Some(core) = document.store().get(*key) else {
            return false;
        };
        // An element out of the document is not styled against this device; it keeps its entry
        // for when it comes back.
        if !core.has_flags(NodeFlags::IN_DOCUMENT) {
            return true;
        }
        let node = document.node(core.index());
        // Asked read-only first: the write borrow is taken only where a unit was actually resolved.
        let hint = {
            let Some(data) = node.borrow_style_data() else {
                return false;
            };
            if !data.has_styles() {
                return false;
            }
            match data.styles.viewport_unit_usage() {
                ViewportUnitUsage::None => return false,
                ViewportUnitUsage::FromDeclaration => RestyleHint::RECASCADE_SELF,
                ViewportUnitUsage::FromQuery => RestyleHint::RESTYLE_SELF,
            }
        };
        let Some(mut data) = node.mutate_style_data() else {
            return false;
        };
        data.hint.insert(hint);
        marked.push((core.index(), hint));
        true
    });

    let probe = UnitProbe {
        probed,
        marked: marked.len(),
    };
    let store = document.store_mut();
    for (index, hint) in marked {
        let bits = if hint.contains(RestyleHint::RESTYLE_SELF) {
            Dirty::RESTYLE
        } else {
            Dirty::RECASCADE
        };
        // The hint alone is never read: the traversal descends only where something says there is
        // work below, so an element nothing leads to keeps its stale value.
        propagate::mark(store, index, bits);
    }
    probe
}

/// Marks every element as needing to be laid out again, and reports how many.
///
/// This is what a change of device pixel ratio costs. Every box is snapped to a different device
/// pixel grid, and every length that resolved against the ratio resolved against the old one, so
/// there is no subtree that can be skipped.
pub(crate) fn relayout_everything(document: &mut Document) -> usize {
    let mut elements = Vec::new();
    for_each_element(document, |_document, index| elements.push(index));
    let count = elements.len();
    let store = document.store_mut();
    for index in elements {
        propagate::mark(store, index, Dirty::RELAYOUT);
    }
    count
}

/// Calls `visit` for every element of `document`, in document order.
fn for_each_element(document: &Document, mut visit: impl FnMut(&Document, NodeIndex)) {
    let mut stack = vec![document.document_index()];
    while let Some(index) = stack.pop() {
        if document.store().core(index).kind() == NodeKind::Element {
            visit(document, index);
        }
        let mut child = document.store().core(index).last_child();
        while let Some(current) = child {
            stack.push(current);
            child = document.store().core(current).prev_sibling();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Viewport;
    use crate::device::color_scheme::ColorScheme;
    use zgui_geom::CssPx;

    #[test]
    fn each_quantity_is_reported_on_its_own() {
        let base = Viewport::new(CssPx(1280.0), CssPx(800.0));

        let wider = Viewport::new(CssPx(1400.0), CssPx(800.0));
        assert!(base.changes_to(wider).size);
        assert!(!base.changes_to(wider).scale);
        assert!(!base.changes_to(wider).scheme);

        let denser = base.at_scale(2.0);
        assert!(!base.changes_to(denser).size);
        assert!(base.changes_to(denser).scale);

        let dark = base.in_scheme(ColorScheme::Dark);
        assert!(base.changes_to(dark).scheme);
        assert!(!base.changes_to(dark).size);

        assert!(!base.changes_to(base).any());
    }
}
