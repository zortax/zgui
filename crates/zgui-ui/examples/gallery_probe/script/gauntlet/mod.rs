//! A second, harder run over the same window, written to disbelieve the first.
//!
//! Everything else in this script asks a component whether it worked and reads the answer out of
//! the document. That is the wrong instrument for two faults in particular. A surface that leaves
//! a focus trap behind when it closes leaves a document that is *correct* — the trap is a live
//! object doing exactly what it was told — and a window that answers nothing. And an icon that is
//! not drawn into the pixels a repaint cleared leaves a box of the right size in the right place
//! with nothing in it. Both look perfect from inside the process.
//!
//! So both claims here are made from pictures of the window, taken through the compositor, and
//! the rectangle each picture is judged over is written into the report from the laid-out document
//! rather than chosen afterwards by eye.
//!
//! The work is split into one cycle, or one step, per turn of the loop. A part that ran the whole
//! of it in one turn would stop answering the desktop's liveness ping for minutes at a time, and a
//! window the compositor has given up on is one whose captures can no longer be trusted to be of
//! the frame the application just drew — which would make every picture below evidence of nothing.

pub(crate) mod answer;
pub(crate) mod endurance;
pub(crate) mod ink;
pub(crate) mod modals;
pub(crate) mod nested;
