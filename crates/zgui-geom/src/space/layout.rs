//! The layout space.

crate::space::space!(
    Layout,
    "layout",
    "The space layout arithmetic is carried out in, measured in [`Au`](crate::Au).",
    "",
    "Layout adds, subtracts and distributes lengths thousands of times per box tree. Doing that",
    "in binary floating point makes the result depend on the order the additions happened in,",
    "which shows up as a column that is one pixel wider than its neighbour for no visible",
    "reason. [`Au`](crate::Au) is an exact integer, so this space has no rounding error to",
    "accumulate.",
    "",
    "Results leave this space once, at the end, by converting to [`Css`](crate::Css).",
    "",
    "This type is uninhabited, so a value of it cannot be created; it exists only as a type",
    "argument.",
);
