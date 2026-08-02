//! The CSS coordinate space.

crate::space::space!(
    Css,
    "css",
    "The author's coordinate space: the viewport origin at the top left, x growing right and y",
    "growing down, measured in [`CssPx`](crate::CssPx).",
    "",
    "Style, layout results reported back to application code and hit testing all speak this",
    "space. It is independent of the output device: the same document produces the same CSS",
    "coordinates on a 1x display and a 3x one.",
    "",
    "Reaching [`Device`](crate::Device) means multiplying by a",
    "[`Scale<Css, Device>`](crate::Scale), which is also where the device-pixel snapping policy",
    "in [`snap`](crate::snap) applies.",
    "",
    "This type is uninhabited, so a value of it cannot be created; it exists only as a type",
    "argument.",
);
