//! What the route cache declines and keeps as shapes change between frames.

use std::sync::Arc;

use crate::VectorId;

use super::VectorRouteCache;

/// The owner most cases ask as.
const OWNER: VectorId = VectorId(7);

/// Runs one frame of `cache`, in which `during` runs.
fn frame(cache: &mut VectorRouteCache, during: impl FnOnce(&mut VectorRouteCache)) {
    cache.begin_frame();
    during(cache);
    cache.end_frame();
}

#[test]
fn an_analytic_decline_is_remembered_for_eight_frames() {
    let mut cache = VectorRouteCache::default();
    frame(&mut cache, |cache| {
        assert!(cache.analytic_allowed(OWNER), "nothing declined yet");
        cache.note_analytic_declined(OWNER);
        assert!(!cache.analytic_allowed(OWNER));
        assert!(
            cache.analytic_allowed(VectorId(8)),
            "a decline names one owner"
        );
        assert!(
            cache.marks_allowed(OWNER),
            "an analytic decline leaves the marks route alone"
        );
    });
    for index in 1..8 {
        frame(&mut cache, |cache| {
            assert!(!cache.analytic_allowed(OWNER), "frame {index}");
        });
        assert!(
            cache.histories.contains_key(&OWNER),
            "the decline counts in frame {index}, so the sweep keeps it"
        );
    }
    frame(&mut cache, |cache| {
        assert!(cache.analytic_allowed(OWNER), "frame 8 tries again");
    });
    assert!(
        !cache.histories.contains_key(&OWNER),
        "swept once it stops counting"
    );
}

#[test]
fn a_marks_decline_is_remembered_for_eight_frames() {
    let mut cache = VectorRouteCache::default();
    frame(&mut cache, |cache| cache.note_marks_declined(OWNER));
    for index in 1..8 {
        frame(&mut cache, |cache| {
            assert!(!cache.marks_allowed(OWNER), "frame {index}");
            assert!(cache.path_glyphs_allowed(OWNER));
        });
    }
    frame(&mut cache, |cache| assert!(cache.marks_allowed(OWNER)));
}

/// A source that keeps recognitions.
#[derive(Default)]
struct Remembering(core::cell::RefCell<crate::content::vectors::Recognitions>);

impl crate::content::vectors::VectorMaskSource for Remembering {
    fn recognitions(
        &self,
    ) -> Option<core::cell::RefMut<'_, crate::content::vectors::Recognitions>> {
        Some(self.0.borrow_mut())
    }
}

/// A filled circle as a shape in its own space.
fn circle_shape() -> zgui_svg::Shape {
    use kurbo::Shape as _;

    zgui_svg::Shape {
        path: Arc::new(kurbo::Circle::new((20.0, 20.0), 8.0).to_path(0.01)),
        fill: Some(zgui_svg::Fill {
            paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// What the circle's fill is found to be, at `tau`.
fn recognised_at(
    source: &Remembering,
    shape: &zgui_svg::Shape,
    tau: f64,
) -> Option<Arc<crate::emit::vector::recognise::Decomposition>> {
    use crate::emit::vector::ShapeSource;
    use crate::emit::vector::recognised::{Outline, PartOf, recognised};

    recognised(
        &ShapeSource::placed_shape(shape),
        Outline::Path,
        PartOf::Fill(peniko::Fill::NonZero),
        tau,
        1 << 22,
        source,
    )
}

#[test]
fn a_recognition_is_reused_by_path_identity_and_swept_after_eight_frames() {
    let source = Remembering::default();
    let shape = circle_shape();
    source.0.borrow_mut().begin_frame();
    let first = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    let again = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    assert!(
        Arc::ptr_eq(&first, &again),
        "the second lookup is the first result"
    );
    // Another allocation of the same geometry is another path.
    let copy = zgui_svg::Shape {
        path: Arc::new(shape.path.as_ref().clone()),
        ..shape.clone()
    };
    let other = recognised_at(&source, &copy, 1.0 / 16.0).expect("a circle");
    assert!(!Arc::ptr_eq(&first, &other));
    let other_again = recognised_at(&source, &copy, 1.0 / 16.0).expect("a circle");
    assert!(Arc::ptr_eq(&other, &other_again));
    // A result something else holds is kept; these are let go.
    drop((first, again, other, other_again));
    source.0.borrow_mut().end_frame();
    for frame in 1..8 {
        source.0.borrow_mut().begin_frame();
        source.0.borrow_mut().end_frame();
        assert_eq!(source.0.borrow().len(), 2, "frame {frame} keeps both");
    }
    source.0.borrow_mut().begin_frame();
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        0,
        "eight untouched frames sweep them"
    );
}

#[test]
fn a_finer_tau_class_misses_the_cache() {
    let source = Remembering::default();
    let shape = circle_shape();
    let coarse = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    let same_class = recognised_at(&source, &shape, 1.0 / 12.0).expect("a circle");
    assert!(
        Arc::ptr_eq(&coarse, &same_class),
        "one class serves every tolerance within a factor of two"
    );
    let finer = recognised_at(&source, &shape, 1.0 / 64.0).expect("a circle");
    assert!(
        !Arc::ptr_eq(&coarse, &finer),
        "a finer class is measured again"
    );
}

#[test]
fn a_recognition_nothing_finds_again_lives_one_frame() {
    let source = Remembering::default();
    let shape = circle_shape();
    source.0.borrow_mut().begin_frame();
    recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        1,
        "kept for the next frame to find"
    );
    source.0.borrow_mut().begin_frame();
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        0,
        "nothing found it, so it is gone"
    );
}

#[test]
fn a_frame_that_finds_nothing_it_held_stops_keeping_until_a_probe() {
    let source = Remembering::default();
    let fresh = || zgui_svg::Shape {
        path: Arc::new(circle_shape().path.as_ref().clone()),
        ..circle_shape()
    };
    // Every frame draws new allocations, so no lookup finds anything.
    let mut kept = Vec::new();
    for _ in 0..16 {
        source.0.borrow_mut().begin_frame();
        let shapes: Vec<_> = (0..20).map(|_| fresh()).collect();
        let before = source.0.borrow().len();
        for shape in &shapes {
            recognised_at(&source, shape, 1.0 / 16.0);
        }
        kept.push(source.0.borrow().len() - before);
        source.0.borrow_mut().end_frame();
    }
    assert!(
        kept.iter().filter(|&&held| held == 0).count() >= 12,
        "an idle cache keeps entries only on probe frames: {kept:?}"
    );
    assert!(
        kept.iter().any(|&held| held > 0),
        "a probe keeps them: {kept:?}"
    );
}
