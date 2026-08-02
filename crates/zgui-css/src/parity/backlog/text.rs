//! Text properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is
//! measured: each one has a probe that sets it on a fixture, and none of those probes moves
//! anything the fragment tree or hit testing shows. A row that starts moving something fails.

use crate::parity::support::Support;

/// Why none of these has an effect yet.
const NOTE: &str = "no probe has shown it changing a shaped line, and no module reads it";

crate::register_properties! {
    _webkit_text_security => Support::Ignored(NOTE),
    caret_color => Support::Ignored(NOTE),
    color_scheme => Support::Ignored(NOTE),
    cursor => Support::Ignored(NOTE),
    image_rendering => Support::Ignored(NOTE),
    tab_size => Support::Ignored(NOTE),
    text_decoration_color => Support::Ignored(NOTE),
    text_decoration_line => Support::Ignored(NOTE),
    text_decoration_style => Support::Ignored(NOTE),
    text_overflow => Support::Ignored(NOTE),
    text_rendering => Support::Ignored(NOTE),
    text_shadow => Support::Ignored(NOTE),
    text_transform => Support::Ignored(NOTE),
    unicode_bidi => Support::Ignored(NOTE),
}
