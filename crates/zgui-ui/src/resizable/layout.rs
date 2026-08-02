//! How much of a group each panel gets, and what moving a divider does to that.

/// How small and how large one panel may be, as a percentage of its group.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PanelBound {
    /// The smallest share this panel may take.
    pub min: f64,
    /// The largest.
    pub max: f64,
}

impl PanelBound {
    /// A panel that may be anywhere between `min` and `max` percent of its group.
    #[must_use]
    pub const fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
}

impl Default for PanelBound {
    fn default() -> Self {
        Self::new(0.0, 100.0)
    }
}

/// Moves the divider after panel `before` by `delta` percentage points, and reports what moved.
///
/// The divider takes from one panel and gives to the other, so the total is unchanged whatever
/// happens — which is what keeps a group of panels exactly as wide as the group. A move that would
/// push either panel past one of its bounds is applied as far as it goes and no further, so a
/// divider dragged to the far side of the window stops where the panel beside it stops rather than
/// dragging the rest of the layout along with it.
///
/// The number returned is how much actually moved, which is what a caller needs to know to decide
/// whether the drag reached its own end.
///
/// ```
/// use zgui_ui::resizable::{PanelBound, drag};
///
/// let mut sizes = [50.0, 50.0];
/// let bounds = [PanelBound::new(20.0, 80.0), PanelBound::new(20.0, 80.0)];
///
/// assert_eq!(drag(&mut sizes, &bounds, 0, 10.0), 10.0);
/// assert_eq!(sizes, [60.0, 40.0]);
///
/// // Only as far as the second panel's minimum allows.
/// assert_eq!(drag(&mut sizes, &bounds, 0, 100.0), 20.0);
/// assert_eq!(sizes, [80.0, 20.0]);
/// ```
pub fn drag(sizes: &mut [f64], bounds: &[PanelBound], before: usize, delta: f64) -> f64 {
    let after = before + 1;
    if after >= sizes.len() || after >= bounds.len() {
        return 0.0;
    }
    let (first, second) = (sizes[before], sizes[after]);
    let (left, right) = (bounds[before], bounds[after]);

    // Every constraint written as a bound on the same number, so the answer is one clamp rather
    // than four branches that each get one case slightly wrong.
    let lower = (left.min - first).max(second - right.max);
    let upper = (left.max - first).min(second - right.min);
    let moved = delta.clamp(lower.min(0.0), upper.max(0.0));

    sizes[before] = first + moved;
    sizes[after] = second - moved;
    moved
}

/// Brings a set of declared sizes into a set that adds up to a hundred and respects every bound.
///
/// A caller writes what each panel should start at, and those numbers rarely add up: three panels
/// at 30% each is a group with a tenth of it unaccounted for. The shortfall is shared out in
/// proportion to what each panel already has, and then everything is clamped — so a group that was
/// declared sensibly is left alone, and one that was not is still laid out.
///
/// ```
/// use zgui_ui::resizable::{PanelBound, normalise};
///
/// // Three thirds of nothing in particular become three equal panels.
/// let sizes = normalise(&[30.0, 30.0, 30.0], &[PanelBound::default(); 3]);
/// assert!(sizes.iter().all(|size| (size - 100.0 / 3.0).abs() < 0.001));
///
/// // Nothing declared at all: an equal share each.
/// assert_eq!(normalise(&[0.0, 0.0], &[PanelBound::default(); 2]), [50.0, 50.0]);
/// ```
#[must_use]
pub fn normalise(sizes: &[f64], bounds: &[PanelBound]) -> Vec<f64> {
    if sizes.is_empty() {
        return Vec::new();
    }
    let count = sizes.len() as f64;
    let total: f64 = sizes.iter().sum();
    let mut out: Vec<f64> = if total <= f64::EPSILON {
        vec![100.0 / count; sizes.len()]
    } else {
        sizes.iter().map(|size| size * 100.0 / total).collect()
    };
    for (size, bound) in out.iter_mut().zip(bounds) {
        *size = size.clamp(bound.min, bound.max);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{PanelBound, drag, normalise};

    /// How much of the group is accounted for.
    fn total(sizes: &[f64]) -> f64 {
        sizes.iter().sum()
    }

    #[test]
    fn a_drag_never_changes_how_much_of_the_group_is_accounted_for() {
        // The defect this catches is a divider that grows one panel without shrinking the other,
        // which looks right until the last panel is pushed out of the window.
        let bounds = [PanelBound::new(10.0, 90.0); 3];
        let mut sizes = [40.0, 30.0, 30.0];
        for delta in [5.0_f64, -12.0, 400.0, -400.0, 0.5] {
            drag(&mut sizes, &bounds, 0, delta);
            assert!(
                (total(&sizes) - 100.0).abs() < 0.000_1,
                "{sizes:?} adds up to {}",
                total(&sizes)
            );
        }
    }

    #[test]
    fn a_drag_stops_at_whichever_bound_it_reaches_first() {
        let bounds = [PanelBound::new(0.0, 60.0), PanelBound::new(25.0, 100.0)];
        let mut sizes = [50.0, 50.0];
        assert_eq!(
            drag(&mut sizes, &bounds, 0, 50.0),
            10.0,
            "the first panel's maximum"
        );
        assert_eq!(sizes, [60.0, 40.0]);

        // The other way, the first panel's own minimum is what stops it.
        let mut sizes = [50.0, 50.0];
        assert_eq!(
            drag(&mut sizes, &bounds, 0, -80.0),
            -50.0,
            "the first panel's minimum"
        );
        assert_eq!(sizes, [0.0, 100.0]);

        // A squeeze that stays inside every bound is applied in full.
        let mut sizes = [50.0, 50.0];
        assert_eq!(drag(&mut sizes, &bounds, 0, -40.0), -40.0);
        assert_eq!(sizes, [10.0, 90.0]);
    }

    #[test]
    fn a_divider_past_the_last_panel_moves_nothing() {
        let bounds = [PanelBound::default(); 2];
        let mut sizes = [50.0, 50.0];
        assert_eq!(drag(&mut sizes, &bounds, 1, 10.0), 0.0);
        assert_eq!(sizes, [50.0, 50.0]);
    }

    #[test]
    fn normalising_leaves_a_group_that_already_adds_up_alone() {
        let bounds = [PanelBound::default(); 2];
        assert_eq!(normalise(&[70.0, 30.0], &bounds), [70.0, 30.0]);
    }

    #[test]
    fn normalising_respects_a_minimum() {
        let bounds = [PanelBound::new(40.0, 100.0), PanelBound::default()];
        let sizes = normalise(&[10.0, 90.0], &bounds);
        assert!(sizes[0] >= 40.0);
    }

    #[test]
    fn a_group_with_no_panels_normalises_to_nothing() {
        assert!(normalise(&[], &[]).is_empty());
    }
}
