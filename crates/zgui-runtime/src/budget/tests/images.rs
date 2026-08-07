//! What the decoded-images budget promises now that the loader can decode again.
//!
//! The premise this adapter ran on for its first life — that texels were unreproducible and
//! therefore all pinned — is gone: a picture arrives by name, and the loader keeps the name. What
//! is asserted here is the new split: shown pictures are pinned, off-screen history is evictable
//! whole-entries-at-a-time, and forget drops even the shown ones in the knowledge that a settle
//! brings them back.

use std::sync::Arc;

use zgui_geom::Size;
use zgui_paint::ContentCache;

use crate::budget::caches::DecodedImagesBudget;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::rebuild;
use crate::images::ImageLoader;
use crate::replaced::IntrinsicTable;

/// A four-by-four picture: sixty-four bytes of premultiplied texels.
const EXTENT: u32 = 4;
/// What those texels weigh.
const BYTES: u64 = (EXTENT * EXTENT * 4) as u64;

/// A node key of the first domain.
fn node(n: u32) -> zgui_dom::NodeKey {
    use zgui_arena::{DomainId, Generation};
    zgui_dom::NodeKey::new(n, Generation::FIRST, DomainId::FIRST)
}

/// One decoded picture's worth of texels.
fn decoded() -> zgui_image::Decoded {
    zgui_image::Decoded {
        size: Size::new(EXTENT, EXTENT),
        texels: Arc::new(vec![0; BYTES as usize]),
    }
}

/// A loader holding one shown picture and one orphaned one.
fn loader() -> ImageLoader {
    let mut loader = ImageLoader::new(IntrinsicTable::new(), 2048);
    loader.insert_ready_for_tests("shown.png", &[node(3)], decoded());
    loader.insert_ready_for_tests("history.png", &[], decoded());
    loader
}

/// The report splits shown from history, and prices a rebuild as a decode.
#[test]
fn shown_pictures_are_pinned_and_history_is_evictable() {
    let mut loader = loader();
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    let mut tracked = Tracked::default();
    let budget = DecodedImagesBudget::new(&mut loader, &mut content, BYTES, &mut tracked);

    let report = budget.report();
    assert_eq!(report.resident, 2 * BYTES);
    assert_eq!(
        report.pinned, BYTES,
        "what is on the screen may not be trimmed"
    );
    assert_eq!(report.evictable(), BYTES, "what scrolled away may");
    assert_eq!(report.rebuild_cost, rebuild::DECODED);
}

/// One source is one allocation even when many elements show it.
#[test]
fn a_shared_source_is_counted_once() {
    let mut loader = ImageLoader::new(IntrinsicTable::new(), 2048);
    loader.insert_ready_for_tests("shared.png", &[node(3), node(4), node(5)], decoded());
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    let mut tracked = Tracked::default();
    let budget = DecodedImagesBudget::new(&mut loader, &mut content, BYTES, &mut tracked);

    let report = budget.report();
    assert_eq!(report.resident, BYTES);
    assert_eq!(report.pinned, BYTES);
}

/// Eviction frees history and never touches what is shown, however much is asked for.
#[test]
fn eviction_takes_history_only() {
    let mut loader = loader();
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    let mut tracked = Tracked::default();
    let mut budget = DecodedImagesBudget::new(&mut loader, &mut content, BYTES, &mut tracked);

    let freed = budget.evict(10 * BYTES, crate::budget::SceneEpoch::FIRST);
    assert_eq!(freed, BYTES, "the orphaned entry, whole, and nothing else");
    let report = budget.report();
    assert_eq!(report.resident, BYTES);
    assert_eq!(report.pinned, BYTES);

    assert!(loader.holds_texels_for_tests("shown.png"));
    assert!(!loader.holds_texels_for_tests("history.png"));
}

/// The level is real now: it is stated, and it is the number the window's limits carry.
#[test]
fn it_states_the_configured_level() {
    let mut loader = loader();
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    let mut tracked = Tracked::default();
    let budget = DecodedImagesBudget::new(&mut loader, &mut content, 12_345, &mut tracked);

    assert_eq!(budget.limit(), Some(12_345));
}

/// Forget drops the shown pictures too — and leaves their entries behind to be decoded again,
/// which is the difference between losing a picture and re-paying for it.
#[test]
fn forget_drops_everything_but_keeps_the_names() {
    let mut loader = loader();
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    let mut tracked = Tracked::default();
    let mut budget = DecodedImagesBudget::new(&mut loader, &mut content, BYTES, &mut tracked);

    budget.forget();

    assert!(
        budget.report().is_empty(),
        "the registry-wide assertion that a forgotten window holds nothing includes this cache"
    );
    assert!(!loader.holds_texels_for_tests("shown.png"));
    assert!(
        !loader.holds_texels_for_tests("history.png"),
        "an orphan is not even worth re-decoding: it is gone entirely"
    );
}
