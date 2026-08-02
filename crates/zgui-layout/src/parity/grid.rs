//! The grid vocabulary.
//!
//! Every row here is evidence-backed: setting the property on a fixture changes the fragment
//! tree or the answer hit testing gives, and a row whose probe stops showing that fails.

use zgui_css::parity::Support;

/// Where these properties are read.
const READER: &str = "zgui-layout::style::grid";

zgui_css::register_properties! {
    grid_auto_flow => Support::Implemented(READER),
    grid_auto_rows => Support::Implemented(READER),
    grid_column_end => Support::Implemented(READER),
    grid_column_start => Support::Implemented(READER),
    grid_row_end => Support::Implemented(READER),
    grid_row_start => Support::Implemented(READER),
    grid_template_columns => Support::Implemented(READER),
    grid_template_rows => Support::Implemented(READER),
}
