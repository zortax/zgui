//! The one registered cache a window driven by this framework's own frame loop cannot fill.
//!
//! Nothing in the runtime attaches a decoded picture to a replaced node — the only callers of
//! [`ContentCache::set_image`](zgui_paint::ContentCache::set_image) in this workspace are
//! `zgui-paint`'s own tests, and an embedder reaches it through the content cache directly. So the
//! registry-wide assertions run against a window in which this cache is legitimately empty, and
//! what it promises is asserted here instead, against a cache filled the way an embedder fills one.

use zgui_geom::Size;
use zgui_paint::ContentCache;

use crate::budget::caches::DecodedImagesBudget;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::rebuild;

/// A four-by-four picture: sixty-four bytes of premultiplied texels.
const EXTENT: u32 = 4;
/// What those texels weigh.
const BYTES: u64 = (EXTENT * EXTENT * 4) as u64;

/// A replaced identifier for a node of the first domain.
fn id() -> zgui_dom::host::ReplacedId {
    use zgui_arena::{DomainId, Generation};
    zgui_dom::host::ReplacedId::new(zgui_dom::NodeKey::new(
        3,
        Generation::FIRST,
        DomainId::FIRST,
    ))
}

/// A cache holding one attached picture.
fn holding_a_picture() -> ContentCache {
    let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
    content
        .set_image(id(), Size::new(EXTENT, EXTENT), vec![0; BYTES as usize])
        .expect("the texels match the extent");
    content
}

/// Every byte is pinned, so eviction may never take one.
#[test]
fn nothing_attached_to_a_replaced_node_is_ever_evictable() {
    let mut content = holding_a_picture();
    let mut tracked = Tracked::default();
    let mut budget = DecodedImagesBudget::new(&mut content, &mut tracked);

    let report = budget.report();
    assert_eq!(report.resident, BYTES);
    assert_eq!(
        report.pinned, BYTES,
        "the texels arrived already decoded and nothing here can ask for them again"
    );
    assert_eq!(report.evictable(), 0);
    assert_eq!(report.rebuild_cost, rebuild::UNREPRODUCIBLE);

    assert_eq!(
        budget.evict(BYTES, crate::budget::SceneEpoch::FIRST),
        0,
        "eviction must not be able to lose a picture the application cannot be asked for again"
    );
    assert_eq!(budget.report().resident, BYTES);
}

/// It states no level, because a level it could never come back under would be a standing failure.
#[test]
fn it_states_no_level_at_all() {
    let mut content = holding_a_picture();
    let mut tracked = Tracked::default();
    let budget = DecodedImagesBudget::new(&mut content, &mut tracked);

    assert_eq!(budget.limit(), None);
}

/// Forget is the one path that does drop them, which is what puts a window into the cold state.
#[test]
fn forget_drops_what_eviction_may_not() {
    let mut content = holding_a_picture();
    let mut tracked = Tracked::default();
    let mut budget = DecodedImagesBudget::new(&mut content, &mut tracked);
    assert!(!budget.report().is_empty());

    budget.forget();

    assert!(
        budget.report().is_empty(),
        "the assertion over the whole registry says every cache is empty after this, and this is \
         the cache that a window fixture cannot put into the state where that claim bites"
    );
}
