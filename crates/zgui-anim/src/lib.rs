//! Transitions, keyframe animations, and the two paths that move a frame without restyling it.
//!
//! # The three tiers
//!
//! The question every animation is sorted by is *which stage of the frame has to run again for the
//! screen to be right*. There are three answers, and the whole design is that the third is the last
//! resort rather than the default.
//!
//! **The repaint path** is for an animation nothing computes from: an `opacity` that fades, a
//! background that lightens under a pointer. Nothing about where the box is, what it contains or
//! what it covers changes, so the frame owes a repaint of a rectangle that already exists and
//! nothing else. The interpolated value is written into the node's **own** override column and
//! composed over the shared lowered style at the moment of emission — never into the shared style
//! itself, which a thousand identical buttons are all drawn from.
//!
//! **The placement path** is for an animation that moves only *where* the box is drawn: a
//! `transform`, a `translate`, a `rotate`, a `scale`. That is more than a repaint — the box's ink
//! rectangle moves, the rectangle a click is answered over moves, and so does the device position
//! of everything drawn inside it — and it is far less than a cascade, because all three of those
//! are composed by the fragment pass out of one matrix. The interpolated matrix is put where that
//! pass reads it and the element is asked to be composed again; no style is computed, no size is
//! measured and no box is rebuilt.
//!
//! **The general path** is for everything else, and it is a cascade: the element is asked to
//! compute its style again with its animation and transition declarations replaced. It costs more
//! and it is correct for everything, including the cases the other two structurally cannot express
//! — a filter, which changes how far the box's pixels reach; a length, which moves every box around
//! it; an inherited colour, which every descendant computes from. It is also where a transform that
//! comes into existence or goes out of it goes, because *whether* a box is transformed decides
//! whether it establishes a stacking context and whether it is a containing block, and both of
//! those are read from the shared style rather than from the matrix.
//!
//! # One bit, marked on every path
//!
//! Whichever path an element takes, it is marked as animating, and the reason is that this is the
//! only thing anywhere that tells the frame loop to come back. A loop with nothing to wake for
//! sleeps until the user does something. Marking the bit on the general path alone would leave the
//! commonest animation in any component library — a hover transition, which is the repaint path by
//! construction — ticking exactly once and then stopping, on a screen that looks frozen mid-fade
//! while every counter and every assertion about the tick itself still reads correct.
//!
//! ```
//! use zgui_anim::{Animator, Tier};
//! use zgui_style::AnimatedProperties;
//!
//! // A fading button: nothing but its own alpha moves, so it repaints and never restyles.
//! assert_eq!(Tier::of(AnimatedProperties::OPACITY), Tier::Cheap);
//! // A sliding panel carries its descendants with it, so its fragments are composed again.
//! assert_eq!(Tier::of(AnimatedProperties::TRANSFORM), Tier::Place);
//! // A growing panel moves everything around it, so it goes back through the cascade.
//! assert_eq!(Tier::of(AnimatedProperties::CASCADED), Tier::Cascade);
//!
//! let animator = Animator::new();
//! assert_eq!(animator.animating(), 0);
//! ```
//!
//! | Module | Contents |
//! |---|---|
//! | [`tier`] | which path an element's animations take |
//! | [`frame`] | one tick: the writes, the marks and the counts it produces |
//! | [`event`] | lifecycle edges lowered into the payloads a listener receives |
//! | [`motion`] | the non-CSS driver: springs and tweens over plain numbers |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod event;
pub mod frame;
pub mod motion;
pub mod tier;

pub use crate::event::{Edge, lower};
pub use crate::frame::{Animator, Placed, Placer, Recomposing, Tick};
pub use crate::motion::{Spring, Tween};
pub use crate::tier::Tier;
