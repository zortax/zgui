//! The fragment tree as stable text, which is what a conformance run compares.
//!
//! # Why fragments and not pixels
//!
//! A reference test asks whether two documents lay out the same. Rendering both and comparing
//! images answers that, slowly, on a graphics device, and tells whoever reads the failure only
//! that some pixels differ. Comparing the fragment trees answers the same question in microseconds,
//! on no device at all, and the failure names the box and the edge that moved. It also settles a
//! constraint that a pixel comparison would quietly erode: comparing fragments needs no markup
//! parser anywhere, not even a test-only one.
//!
//! # What a projection deliberately leaves out
//!
//! A test and its reference reach the same layout by different routes — different properties,
//! often a different number of elements — so a comparison over styles or over identifiers would
//! fail on every passing test. What is compared is therefore *geometry and content*: what each
//! fragment is, where its edges are, and what text it carries. Identifiers, clip chains, paint
//! caches and flags are all excluded, because two documents that agree about every edge have
//! passed whatever the test was about.

use core::fmt::Write as _;

use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_layout::tree::store::LayoutStore;
use zgui_layout::{BoxKey, Fragment, FragmentKind};
use zgui_testkit_scene::text::number;

/// The whole fragment tree of a laid-out document, rendered.
///
/// Stable: the same tree renders the same bytes on every run, because the walk is the box tree's
/// own child order and every number goes through one formatter.
pub fn project(store: &LayoutStore) -> String {
    let mut out = String::new();
    match store.root() {
        None => out.push_str("no root\n"),
        Some(root) => write_box(store, root, 0, Detail::Geometry, &mut out),
    }
    out
}

/// Everything about the fragment tree that a stage after layout reads, rendered.
///
/// Where [`project`] answers *"do these two documents lay out the same"*, this answers *"would
/// anything downstream behave differently"*. It carries the identifiers, the flags, the ink extents
/// and the clip and transform chains as well as the edges, because a property whose only effect is
/// to establish a stacking context or to widen an ink rectangle has still had an effect, and a
/// comparison blind to that would report it as having none.
pub fn full(store: &LayoutStore) -> String {
    let mut out = String::new();
    match store.root() {
        None => out.push_str("no root\n"),
        Some(root) => write_box(store, root, 0, Detail::Full, &mut out),
    }
    out
}

/// What hit testing answers over a grid of sample points.
///
/// The fragment tree says where everything is; it does not say what is *under a point*, which is a
/// different answer computed from the clip chain, each fragment's own transform and its
/// `pointer-events`. A property that only changes that answer — and `pointer-events` is exactly one
/// — would look inert to a comparison of the tree alone, which is how a working feature ends up
/// recorded as having no effect.
pub fn hit_answers(laid: &crate::zdoc::build::Laid) -> String {
    let mut out = String::new();
    for row in 0..30 {
        for column in 0..40 {
            let point = Point::new(DevicePx(column as f32 * 10.0), DevicePx(row as f32 * 10.0));
            let hits = laid
                .tables
                .hit
                .hit(point, &laid.tables.clips, &laid.tables.spatial);
            let _ = write!(out, "{:?};", hits.first().map(|key| key.index()));
        }
        out.push('\n');
    }
    out
}

/// How much of a fragment a rendering writes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Detail {
    /// Edges and text only, so a test and its reference can be compared.
    Geometry,
    /// Everything a stage after layout reads.
    Full,
}

/// One box's fragments, then its children's.
fn write_box(store: &LayoutStore, key: BoxKey, depth: usize, detail: Detail, out: &mut String) {
    let Some(node) = store.get(key) else {
        return;
    };
    for fragment in store.fragments_of_box(key) {
        let Some(fragment) = store.fragment(*fragment) else {
            continue;
        };
        write_fragment(fragment, node.text.as_deref(), depth, detail, out);
    }
    for &child in &node.children {
        write_box(store, child, depth + 1, detail, out);
    }
}

/// One fragment: what it is, where its edges are, and what it says.
fn write_fragment(
    fragment: &Fragment,
    text: Option<&str>,
    depth: usize,
    detail: Detail,
    out: &mut String,
) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(label(fragment.kind));
    let _ = write!(out, " border={}", rect(fragment.border_box));
    if fragment.content_box != fragment.border_box {
        let _ = write!(out, " content={}", rect(fragment.content_box));
    }
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        let _ = write!(out, " text={text:?}");
    }
    if detail == Detail::Full {
        let _ = write!(
            out,
            " padding={} ink={} subtree-ink={} flags={} clip={} transform={:?} stacking={:?} \
             scroll={:?} disjoint={}",
            rect(fragment.padding_box),
            rect(fragment.ink),
            rect(fragment.subtree_ink),
            fragment.flags.bits(),
            fragment.clip.0,
            fragment.transform.map(zgui_scene::SpatialId::index),
            fragment.stacking.map(|id| id.0),
            fragment.scroll.map(|id| id.0),
            fragment.subtree_disjoint,
        );
    }
    out.push('\n');
}

/// What a fragment draws, without the identifiers that name where it came from.
///
/// A paragraph identifier and a line number are omitted on purpose: a test and its reference
/// number their paragraphs differently while laying the same text out in the same place.
fn label(kind: FragmentKind) -> &'static str {
    match kind {
        FragmentKind::Box => "box",
        FragmentKind::Line { .. } => "line",
        FragmentKind::TextRun { .. } => "run",
        FragmentKind::Replaced { .. } => "replaced",
        FragmentKind::Vector => "vector",
        FragmentKind::Scrollbar { .. } => "scrollbar",
    }
}

/// One rectangle, as `(x, y, w × h)`.
fn rect(rect: Rect<DevicePx, Device>) -> String {
    format!(
        "({}, {}, {} x {})",
        number::float(rect.origin.x.0),
        number::float(rect.origin.y.0),
        number::float(rect.size.width.0),
        number::float(rect.size.height.0),
    )
}
