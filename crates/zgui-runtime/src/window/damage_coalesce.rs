//! Avoid replaying a large scene separately for several densely packed dirty regions.
use zgui_bits::DamageSet;

pub(super) fn coalesce(damage: &mut DamageSet) -> bool {
    if damage.len() < 3 {
        return false;
    }
    let Some(bounds) = damage.bounds() else {
        return false;
    };
    let covered = damage.area().unwrap_or(0);
    let bounding = i64::from(bounds.size.width) * i64::from(bounds.size.height);
    // Keep sparse caret/cursor damage sparse. At most 50% extra pixels buys one
    // batch-stream replay instead of three or more, independent of plot count.
    if covered <= 0 || bounding > covered.saturating_add(covered / 2) {
        return false;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var("ZGUI_COALESCE_DAMAGE").as_deref() != Ok("0")) {
        return false;
    }
    damage.clear();
    damage.absorb(bounds);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgui_geom::{Device, Point, Rect, Size};
    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(w, h))
    }
    #[test]
    fn joins_dense_panels_and_keeps_sparse_cursors_and_small_updates_separate() {
        let mut damage = DamageSet::new();
        for y in [0, 110, 220] {
            damage.absorb(rect(0, y, 900, 100));
        }
        let bounds = damage.bounds();
        assert!(coalesce(&mut damage));
        assert_eq!(damage.len(), 1);
        assert_eq!(damage.bounds(), bounds);
        let mut sparse = DamageSet::new();
        for x in [0, 400, 800] {
            sparse.absorb(rect(x, 0, 1, 10));
        }
        assert!(!coalesce(&mut sparse));
        assert_eq!(sparse.len(), 3);
        let mut small = DamageSet::new();
        small.absorb(rect(0, 0, 10, 10));
        small.absorb(rect(11, 0, 10, 10));
        assert!(!coalesce(&mut small));
        assert_eq!(small.len(), 2);
        assert!(!coalesce(&mut DamageSet::full()));
        assert!(!coalesce(&mut DamageSet::new()));
    }
}
