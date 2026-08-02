//! The damage bits this framework adds to the style engine's own four.
//!
//! The engine reserves the top twelve bits of its damage word for whoever is doing the layout, and
//! fills them by calling an associated function with no receiver and no context — so the only
//! implementation that can exist lives beside the element type the engine calls it on. These are
//! that one definition, named from here so that the stage which reads them and the stage which
//! sets them cannot drift into two different sets of bits.
//!
//! Every one of them describes work that is a *consequence* of a layout-affecting change, because
//! the hook only fires when the engine's own relayout bit is already set. That is why there is no
//! restacking bit among them: a `z-index` change that resizes nothing never reaches the hook at
//! all, and its damage comes from the engine's own stacking bit instead.
//!
//! The engine's relayout bit is much wider than this pipeline's idea of a layout — it is set for a
//! border colour and for a corner radius — so these bits are also what says that a change costs no
//! layout at all. A relayout carrying none of them is a change the engine could not classify more
//! finely and this framework can.

pub use zgui_dom::stylo::element::damage::{
    ALL, CONSTRUCT_BOX, CONSTRUCT_DESCENDANTS, CONSTRUCT_FC, REBREAK_TEXT, RECALCULATE_INK,
    RELAYOUT_BOX, RESHAPE_TEXT,
};
