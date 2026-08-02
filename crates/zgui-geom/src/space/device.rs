//! The device pixel space.

crate::space::space!(
    Device,
    "device",
    "The output surface's pixel grid, measured in [`DevicePx`](crate::DevicePx).",
    "",
    "Everything handed to the renderer is in this space, because this is the space the pixel",
    "grid exists in: a horizontal edge lands exactly on a pixel boundary or it does not, and no",
    "amount of care in [`Css`](crate::Css) can decide that question.",
    "",
    "Reaching it means multiplying by a [`Scale<Css, Device>`](crate::Scale), usually through",
    "[`snap_bounds`](crate::snap_bounds) or [`cover_bounds`](crate::cover_bounds) so the result",
    "sits on the grid.",
    "",
    "This type is uninhabited, so a value of it cannot be created; it exists only as a type",
    "argument.",
);
