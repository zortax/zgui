//! The restyle itself: what it touches, what it skips, and what it makes the rest of the frame owe.
//!
//! | Module | Contents |
//! |---|---|
//! | `animation` | the second descent an animation that cannot repaint is serviced by |
//! | `scope` | the marks a restyle never turns into engine work |
//! | `settling` | where a transition leaves the element's own style once it has finished |
//! | `damage` | what each damage level the engine produces makes the frame owe |
//! | `pass` | the shape of one pass: what it collects, how wide it runs, what it retires |
//! | `throughput` | how fast a restyle runs, recorded rather than asserted |

#[path = "support/mod.rs"]
mod support;

#[path = "restyle/animation.rs"]
mod animation;
#[path = "restyle/damage.rs"]
mod damage;
#[path = "restyle/pass.rs"]
mod pass;
#[path = "restyle/scope.rs"]
mod scope;
#[path = "restyle/settling.rs"]
mod settling;
#[path = "restyle/throughput.rs"]
mod throughput;
