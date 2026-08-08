//! The flexbox vocabulary, and the alignment and spacing both formatting contexts share.
//!
//! Every row here is evidence-backed: setting the property on a fixture changes the fragment
//! tree or the answer hit testing gives, and a row whose probe stops showing that fails.

use zgui_css::parity::Support;

/// Where these properties are read.
const READER: &str = "zgui-layout::style::flex";

zgui_css::register_properties! {
    align_content => Support::Implemented(READER),
    align_items => Support::Implemented(READER),
    align_self => Support::Implemented(READER),
    column_gap => Support::Implemented(READER),
    flex_basis => Support::Implemented(READER),
    flex_direction => Support::Implemented(READER),
    flex_grow => Support::Implemented(READER),
    flex_shrink => Support::Implemented(READER),
    flex_wrap => Support::Implemented(READER),
    justify_content => Support::Implemented(READER),
    justify_items => Support::Implemented(READER),
    justify_self => Support::Implemented(READER),
    order => Support::Implemented(READER),
    row_gap => Support::Implemented(READER),
}
