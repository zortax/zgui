//! What a glide frame's per-box cost is made of: the arithmetic, or the two structures told.
//!
//! **This is a diagnostic probe and not a reference workload.** It has no criterion, it gates
//! nothing, and `cargo xtask` does not run it. It answers one question, on purpose, once.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin glide-split
//! ```
//!
//! # The question
//!
//! A subtree that only moved is offset rather than composed again, and the offsetting walk reaches
//! every box in it on every frame of a glide. That walk does two unrelated things to each box. It
//! moves five rectangles and re-interns the clip chain a clipping box imposes; and it moves the
//! box's hit entry and marks a moved control's accessibility node. The first is arithmetic over
//! geometry this crate owns, and a representation that wrote one offset in one place instead of
//! visiting every box would subsume it. The second maintains two structures whose readers are
//! elsewhere, and whatever wrote that one offset would still owe it.
//!
//! So: how does the walk's per-box cost divide between the two? The answer decides whether there
//! is anything left for a representation change to win.
//!
//! # The technique, and what it cannot see
//!
//! [`zgui_layout::fragment::diff::split`] asks the walk to make its descents separately — the bare
//! traversal, the same traversal again, then the geometry, then the index — inside a frame that
//! still discharges both duties in the same order. Each descent is timed. A duty's own cost is its
//! descent minus the traversal it shares with the others, and the fused walk over the same
//! document, measured in the same run on the alternate pass, is what those parts have to add back
//! up to.
//!
//! The index duty is not all inside the walk. Moving an entry writes it where it now is and leaves
//! the structure above the entries for one pass at the end of the frame, so that pass is timed too
//! and counted with the descent that made it necessary.
//!
//! Four sizes, because a slope is a gradient and not one number from one size. Three shapes of the
//! walk interleaved inside one turn, because a comparison between them survives a machine that
//! slows down and a comparison between blocks of turns does not. Two documents, because the
//! arithmetic half includes interning a clip chain and an ordinary list has exactly one box that
//! clips: the second document makes every row clip, which is the most that half can be.
//!
//! What it cannot see: inside a duty — a descent is timed whole, so interning a clip chain and
//! translating five rectangles arrive as one number; how the memory the first descent faults in
//! would divide between the duties, which is why it is reported on its own and charged to neither;
//! and anything about a machine other than this one, which is why what is concluded from it is a
//! ratio between the halves and not either half's microseconds.

#![forbid(unsafe_code)]

mod clock;
mod document;
mod measure;
mod report;
mod sweep;

use crate::document::Clipping;

fn main() {
    println!(
        "This is a diagnostic probe and not a reference workload: it answers one question about \
         one walk, and nothing in `cargo xtask` runs it. See the module documentation."
    );
    let read = clock::read_ns();
    println!(
        "CLOCK one read costs {read:.1} ns; a fused descent is bracketed by two of them and a \
         divided one by eight"
    );
    report::document(Clipping::Scroller);
    report::document(Clipping::EveryRow);
}
