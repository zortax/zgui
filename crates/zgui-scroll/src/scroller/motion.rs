//! Advancing everything that is moving on its own.

use core::time::Duration;

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_layout::LayoutStore;

use crate::scroller::Scroller;

impl Scroller {
    /// Stops whatever one container is doing on its own, leaving it where it is.
    ///
    /// A smooth scroll, a glide and a fling are all a container travelling towards somewhere it is
    /// not yet, so a container that is asked to stop moving has to be taken off the motion as well
    /// as refused new ones — otherwise a page frozen half a glide keeps arriving.
    pub fn halt(&mut self, container: NodeKey) {
        self.motions.remove(&container);
    }

    /// Advances every running motion and relaxes every elastic displacement by `elapsed`.
    ///
    /// Returns the containers whose **composed** position changed, which is what the caller marks
    /// — and which is deliberately not the containers that *moved*. A displacement relaxing counts
    /// even though the clamped offset did not move, because the content is drawn somewhere else;
    /// and a clamped offset that moved by less than the device grid can express does **not**,
    /// because the content is drawn in exactly the same place.
    ///
    /// That second half is worth its own paragraph, because reporting it costs far more than the
    /// frame it belongs to. Marking a container scrolled is what makes the fragment pass descend
    /// through it, and a subtree whose offset is unchanged fails the test that lets the pass
    /// *translate* it rather than compose it again — so a frame that moved a list by three
    /// hundredths of a pixel re-composes and re-encodes the slice of the document on the screen,
    /// and then presents a picture identical to the last one. Every decelerating motion ends in a
    /// run of such frames, and on a fast output that run is most of the motion.
    pub fn advance(&mut self, store: &LayoutStore, elapsed: Duration) -> SmallVec<[NodeKey; 4]> {
        let mut touched: SmallVec<[NodeKey; 4]> = SmallVec::new();
        if elapsed.is_zero() {
            return touched;
        }

        let running: SmallVec<[NodeKey; 4]> = self.motions.keys().copied().collect();
        for container in running {
            let at = self.offset_of(container);
            let drawn_at = self.composed.of(container);
            let Some(motion) = self.motions.get_mut(&container) else {
                continue;
            };
            let step = motion.advance(at, elapsed);
            if step.done {
                self.motions.remove(&container);
            }
            let landed = self.offsets.scroll_to(store, container, step.to);
            if landed == at {
                // A fling that has run into the end of its content stops there rather than
                // spending the rest of its speed asking for frames that move nothing.
                if !step.done {
                    self.motions.remove(&container);
                }
                continue;
            }
            self.compose(container);
            // The listener hears about the whole movement, on the frame it happened, whether or
            // not it moved a pixel: `scrollTop` is exact, and a virtualiser reading it must not
            // see a scroll arrive in steps of a pixel.
            self.record(container, at, landed);
            if self.composed.of(container) != drawn_at {
                touched.push(container);
            }
        }

        let held: SmallVec<[(NodeKey, _); 2]> = self
            .elastic
            .iter()
            .map(|(container, edge)| (*container, *edge))
            .collect();
        for (container, edge) in held {
            let sprung = edge.advanced(elapsed);
            if sprung == edge {
                continue;
            }
            let drawn_at = self.composed.of(container);
            self.displace(container, sprung);
            if self.composed.of(container) != drawn_at && !touched.contains(&container) {
                touched.push(container);
            }
        }

        touched
    }
}
