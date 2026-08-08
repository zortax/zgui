//! One frame of animation: what is written, what is marked, and what the loop is told.
//!
//! This runs before anything else in a frame that has any obligation at all, because everything
//! after it reads what it decided: the restyle needs to know which elements are cascading again,
//! the fragment pass needs the repaint and hit obligations, and the park at the end of the frame
//! needs to know whether to come back.
//!
//! | Module | Contents |
//! |---|---|
//! | [`retire`] | undoing the previous frame's marks and writes |
//! | [`apply`] | this frame's writes and marks, per element |
//! | [`placer`] | who moves a box that can be moved without being composed again |

pub mod apply;
pub mod placer;
pub mod retire;

pub use crate::frame::apply::Placed;
pub use crate::frame::placer::{Placer, Recomposing};

use zgui_dom::side::AnimPlacement;
use zgui_dom::{Document, NodeIndex, NodeKey};
use zgui_style::AnimationReport;

use crate::event::Edge;

/// What one animation tick did.
#[derive(Clone, Debug, Default)]
pub struct Tick {
    /// The elements whose cascade has to be run again, in the order they were reported.
    ///
    /// Handed back rather than acted on, because asking for a cascade is the style engine's call to
    /// make and this crate names no engine.
    pub cascading: Vec<NodeIndex>,
    /// How many elements took the repaint-only path.
    pub cheap: usize,
    /// How many elements were re-placed rather than styled again.
    pub placed: usize,
    /// Every lifecycle edge crossed, ready to be dispatched.
    pub edges: Vec<Edge>,
}

impl Tick {
    /// Whether anything is animating after this tick.
    pub fn is_animating(&self) -> bool {
        self.cheap > 0 || self.placed > 0 || !self.cascading.is_empty()
    }
}

/// The animation stage's state between frames.
///
/// It holds one thing: which elements were given a private override on the last frame. Without it
/// an element whose animation has just ended keeps the last value the animation wrote for the rest
/// of the document's life — a button left half-faded after the pointer has gone, which no assertion
/// about the animation itself can see, because the animation is over and was correct throughout.
#[derive(Debug, Default)]
pub struct Animator {
    /// The elements carrying a cheap-path override, as of the last tick.
    ///
    /// Sorted, so that membership costs a search rather than a scan. A screen full of loading
    /// skeletons puts five hundred elements in here and asks about five hundred, and the scan is a
    /// quarter of a million comparisons per frame on a screen that is *waiting*.
    overridden: Vec<NodeIndex>,
    /// The elements the last tick reported as still *advancing*, whichever path they took.
    ///
    /// The two words differ for exactly the element this list exists to keep out of trouble. A
    /// paused animation, and one holding its last keyframe, are reported with nothing to advance,
    /// and the cascade that runs after the tick is what unpauses one. An element in this list is
    /// skipped when the animations the cascade started are marked, so recording a paused one here
    /// would leave the frame it resumed on unmarked and the loop parked for good over an animation
    /// that is running.
    ///
    /// Kept so that [`Animator::note_started`] can tell an animation the cascade has just created
    /// from one the tick already handled. Without the distinction it would mark the animating bit
    /// for *every* running element, which is a second writer for a bit that has one — and a second
    /// writer is what makes the first one's absence invisible.
    ///
    /// Sorted, for the reason [`Animator::overridden`] is.
    reported: Vec<NodeIndex>,
    /// Where each element on the placement path is putting its box, as of the last tick.
    ///
    /// This is the table the fragment pass composes against, and it lives here rather than in a
    /// column of the document because exactly one stage reads it, once, with the table already in
    /// hand. A second home for it would be a second thing to retire, and a placement left behind by
    /// an animation that ended is a box stuck where the last frame of it put it.
    ///
    /// Sorted by element, so the pass answers a lookup with a search rather than a scan.
    placements: Vec<(NodeKey, AnimPlacement)>,
    /// How many elements were animating on the last tick, all paths together.
    animating: usize,
}

impl Animator {
    /// An animator with nothing running.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many elements were animating on the last tick.
    pub fn animating(&self) -> usize {
        self.animating
    }

    /// Where every element on the placement path is putting its box, sorted by element.
    ///
    /// Handed to the pass that composes fragments, which is the only thing that reads it. It is
    /// rewritten by every tick and every frame runs one, so what it holds is always this frame's
    /// answer — including for an animation that has finished and whose fill mode is holding a
    /// transform in force.
    pub fn placements(&self) -> &[(NodeKey, AnimPlacement)] {
        &self.placements
    }

    /// Applies `report` to `document`: writes the cheap path's values, marks both paths, and
    /// collects the lifecycle edges.
    ///
    /// The previous tick's animation obligation is retired first and this tick's marked afterwards,
    /// so an element that is still animating is never left without one.
    ///
    /// `place` is offered every element whose placement moved, *before* anything is marked for it,
    /// and answers what it did with it. A caller that can move the box by writing the coordinate
    /// system it established answers [`Placed::Written`] and the element is left owing nothing;
    /// one that cannot answers [`Placed::Recomposed`] and the fragment pass is sent down to it.
    /// [`Recomposing`] is the answer for a caller with nowhere to write, which asks for the
    /// composition every time and is what this crate's own tests use.
    pub fn tick(
        &mut self,
        document: &mut Document,
        report: &AnimationReport,
        place: &mut dyn Placer,
    ) -> Tick {
        retire::animating(document);

        let mut tick = Tick {
            edges: report.edges.iter().map(Edge::from_engine).collect(),
            ..Tick::default()
        };
        let mut still_overridden = Vec::with_capacity(self.overridden.len());
        let mut placements: Vec<(NodeKey, AnimPlacement)> =
            Vec::with_capacity(self.placements.len());
        for element in &report.elements {
            // Answered against the previous tick's table, because that is what the standing
            // fragments were composed under: an interpolation that arrived back at the value it was
            // already at owes nothing, and a fragment recomposed for it would be an ink rectangle
            // damaged on every frame of a transform that is holding still.
            let placed = match &element.placement {
                Some(placement) => {
                    let moved = self.placement_of(element.node) != Some(placement);
                    placements.push((element.node, placement.clone()));
                    if moved {
                        place.place(element.node, placement)
                    } else {
                        Placed::Held
                    }
                }
                None => Placed::Held,
            };
            let tier = apply::element(document, element, placed);
            if tier.is_overriding() {
                // Counted only while it is still moving. An element reported because a finished
                // animation is holding its last keyframe is not one the loop owes a frame for, and
                // a count that included it would be the deadline's second opinion.
                if element.advancing {
                    if tier.is_cheap() {
                        tick.cheap += 1;
                    } else {
                        tick.placed += 1;
                    }
                }
                still_overridden.push(element.index);
            } else if element.advancing || element.crossed {
                tick.cascading.push(element.index);
            }
        }
        still_overridden.sort_unstable();
        placements.sort_unstable_by_key(|(node, _)| *node);

        // The elements this tick said anything at all about, whichever path they took. What is
        // *not* in here and still holds an override below had its animation dropped behind this
        // stage's back, and only for those is a cascade owed — an element the report still names
        // is being handled by whichever path it is on, and a second cascade forced from here would
        // disturb an exit that has only just begun.
        let mut in_report: Vec<NodeIndex> = report
            .elements
            .iter()
            .map(|element| element.index)
            .collect();
        in_report.sort_unstable();

        // Whatever carried an override last frame and does not now is painted from its shared style
        // again, and owes a repaint for the frame in which that happens.
        //
        // An element the report no longer names at all owes a cascade as well, not only a repaint.
        // The style it goes back to still carries the animation's declarations as the cascade last
        // captured them — for an animation that filled backwards, the *first* keyframe, held from
        // the frame the animation was created on and masked by the override ever since. The engine
        // drops a finished-but-filling animation outright when an unrelated restyle re-cascades
        // the element — a window resize re-resolving the scrim's viewport units was the report —
        // and without the cascade the element then paints from the first keyframe of an animation
        // that finished long ago: a scrim held at opacity one by its fill vanishes to the opacity
        // zero it entered from.
        for index in &self.overridden {
            if still_overridden.binary_search(index).is_err() {
                retire::override_on(document, *index);
                if in_report.binary_search(index).is_err()
                    && document.store().try_core(*index).is_some()
                {
                    tick.cascading.push(*index);
                }
            }
        }
        // And whatever was being *placed* last frame and is not now goes back to the transform its
        // own style asks for, which is a change of geometry rather than of colour: the fragment
        // pass has to compose it again, or the element keeps the position the animation's last
        // frame put it in for the rest of the document's life. The cascade is owed under the same
        // condition as above: only for an element whose animations the report has lost entirely,
        // whose style still holds the transform the animation's creation captured.
        for (node, _) in &self.placements {
            if placements
                .binary_search_by_key(node, |(held, _)| *held)
                .is_err()
            {
                retire::placement_on(document, *node);
                place.retired(*node);
                if let Some(index) = document.store().index_of(*node)
                    && in_report.binary_search(&index).is_err()
                    && !tick.cascading.contains(&index)
                {
                    tick.cascading.push(index);
                }
            }
        }
        self.overridden = still_overridden;
        self.placements = placements;
        self.reported = report
            .elements
            .iter()
            .filter(|element| element.advancing)
            .map(|element| element.index)
            .collect();
        self.reported.sort_unstable();
        self.animating = tick.cheap + tick.placed + tick.cascading.len();
        tick
    }

    /// Where the last tick left one element's box, if it was placing one at all.
    fn placement_of(&self, node: NodeKey) -> Option<&AnimPlacement> {
        self.placements
            .binary_search_by_key(&node, |(held, _)| *held)
            .ok()
            .map(|position| &self.placements[position].1)
    }

    /// Marks every element in `running` as animating, and reports how many that added.
    ///
    /// This closes a hole with no other symptom than an animation that never starts. A keyframe
    /// animation is created by the *cascade*, and the tick runs before the cascade — so on the
    /// frame that creates one, the element is in no report, carries no obligation, and the loop
    /// parks with nothing to wake for. Five hundred pulsing skeletons then sit at their first
    /// frame for ever, with a populated animation table and a correct tick that is never called
    /// again.
    ///
    /// Called after the restyle, with the elements the engine now holds animations for.
    pub fn note_started(&mut self, document: &mut Document, running: &[NodeIndex]) -> usize {
        let mut added = 0;
        for index in running {
            // Anything this frame's tick reported on has already been given whatever it is owed,
            // by the path it took. Marking it again here would make this the bit's second writer.
            if self.reported.binary_search(index).is_ok()
                || document.store().try_core(*index).is_none()
            {
                continue;
            }
            zgui_dom::dirty::propagate::mark(
                document.store_mut(),
                *index,
                zgui_bits::Dirty::ANIMATING,
            );
            added += 1;
        }
        self.animating += added;
        added
    }

    /// Drops everything, for a document being torn down or rebuilt from nothing.
    pub fn clear(&mut self) {
        self.overridden.clear();
        self.reported.clear();
        self.placements.clear();
        self.animating = 0;
    }
}
