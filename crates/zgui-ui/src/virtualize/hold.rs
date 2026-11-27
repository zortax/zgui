//! How long a built window holds while the port moves.

use crate::virtualize::window::{MAX_EXTENT, VirtualWindow};

/// Returns the offsets, `[low, high)` in CSS pixels, at which [`window`](crate::virtualize::window)
/// builds no row outside `built`.
///
/// `viewport` and `overscan` are what `window` is given. A port that moves builds no row outside
/// `built` while its offset and its offset one step of the motion ahead both lie inside, since
/// [`window_ahead`](crate::virtualize::window_ahead) builds the rows of both. A window whose rows do
/// not depend on the offset, unmeasured or empty, holds at every offset.
#[must_use]
pub fn held_offsets(built: &VirtualWindow, viewport: f32, overscan: usize) -> [f32; 2] {
    if built.rows <= 1 || !built.row_size.is_finite() || built.row_size <= 0.0 {
        return [f32::NEG_INFINITY, f32::INFINITY];
    }
    // The offset that moves the window's anchor by one row.
    let pitch = if built.is_compressed() {
        MAX_EXTENT / (built.rows - 1) as f32
    } else {
        built.row_size
    };
    let visible = ((viewport.max(0.0) / built.row_size).ceil() as usize).max(1) + 1;
    let low = if built.first == 0 {
        f32::NEG_INFINITY
    } else {
        (built.first + overscan) as f32 * pitch
    };
    let high = if built.end() >= built.rows {
        f32::INFINITY
    } else {
        (built.end() as f32 - (visible + overscan) as f32 + 1.0) * pitch
    };
    [low, high]
}

/// Returns the rows to build in place of `previous`, the rows built so far, when `next` is worth
/// building: `previous` while it holds every row of `next`, and `next` once it does not.
///
/// A list therefore drops rows only in a frame that builds one, and a row built ahead of a moving
/// port stays built after the motion slows. A `previous` of another length or another row height
/// gives `next`.
#[must_use]
pub fn keep_built(previous: Option<&VirtualWindow>, next: VirtualWindow) -> VirtualWindow {
    match previous {
        Some(previous)
            if previous.rows == next.rows
                && previous.row_size == next.row_size
                && previous.first <= next.first
                && next.end() <= previous.end() =>
        {
            *previous
        }
        _ => next,
    }
}

#[cfg(test)]
mod tests {
    use super::{held_offsets, keep_built};
    use crate::virtualize::{window, window_ahead};

    #[test]
    fn a_window_holds_until_its_overscan_runs_out() {
        let built = window(1_000, 20.0, 400.0, 10_000.0, 2);
        let [low, high] = held_offsets(&built, 400.0, 2);
        assert_eq!([low, high], [10_000.0, 10_020.0]);
        assert_eq!(window(1_000, 20.0, 400.0, high - 0.5, 2), built);
        assert_ne!(window(1_000, 20.0, 400.0, high, 2), built);
        assert_ne!(window(1_000, 20.0, 400.0, low - 0.5, 2), built);
    }

    #[test]
    fn a_moving_window_holds_while_its_offset_a_step_ahead_does() {
        let built = window_ahead(1_000, 20.0, 400.0, 10_000.0, 2, 30.0);
        let [_, high] = held_offsets(&built, 400.0, 2);
        let inside = window_ahead(1_000, 20.0, 400.0, high - 30.5, 2, 30.0);
        assert!(inside.end() <= built.end());
        let past = window_ahead(1_000, 20.0, 400.0, high - 30.0, 2, 30.0);
        assert!(past.end() > built.end());
    }

    #[test]
    fn rows_built_ahead_stay_until_a_row_has_to_be_built() {
        let ahead = window_ahead(1_000, 20.0, 400.0, 10_000.0, 2, 60.0);
        let slower = window_ahead(1_000, 20.0, 400.0, 10_010.0, 2, 10.0);
        assert_eq!(keep_built(Some(&ahead), slower), ahead);
        let further = window(1_000, 20.0, 400.0, 10_200.0, 2);
        assert_eq!(keep_built(Some(&ahead), further), further);
    }
}
