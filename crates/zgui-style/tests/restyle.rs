//! The restyle itself: what it touches, what it skips, and what it makes the rest of the frame owe.
//!
//! | Module | Contents |
//! |---|---|
//! | `animation` | the second descent an animation that cannot repaint is serviced by |
//! | `custom_reader` | a custom property change reaching a custom element that names no `var()` |
//! | `negation` | a complex selector inside `:not()` as its ancestor and attribute move |
//! | `moved` | an element that moved between its animation mark and the restyle |
//! | `scope` | the marks a restyle never turns into engine work |
//! | `settling` | where a transition leaves the element's own style once it has finished |
//! | `sharing` | equal cascade results under different parents taking one style's groups |
//! | `fill` | what a finished animation that fills forwards holds through a later restyle |
//! | `keyframe_vars` | keyframe values that read a custom property, as the property moves |
//! | `damage` | what each damage level the engine produces makes the frame owe |
//! | `pass` | the shape of one pass: what it collects, how wide it runs, what it retires |
//! | `throughput` | how fast a restyle runs, recorded rather than asserted |
//! | `zero` | keyframe animations with a duration of `0s` |

#[path = "support/mod.rs"]
mod support;

#[path = "restyle/animation.rs"]
mod animation;
#[path = "restyle/custom_reader.rs"]
mod custom_reader;
#[path = "restyle/damage.rs"]
mod damage;
#[path = "restyle/fill.rs"]
mod fill;
#[path = "restyle/keyframe_vars.rs"]
mod keyframe_vars;
#[path = "restyle/moved.rs"]
mod moved;
#[path = "restyle/negation.rs"]
mod negation;
#[path = "restyle/pass.rs"]
mod pass;
#[path = "restyle/scope.rs"]
mod scope;
#[path = "restyle/settling.rs"]
mod settling;
#[path = "restyle/sharing.rs"]
mod sharing;
#[path = "restyle/throughput.rs"]
mod throughput;
#[path = "restyle/zero.rs"]
mod zero;
