//! The feature flags a stylesheet has to be parsed with.
//!
//! Every one of these is read at *parse* time, so a sheet parsed while a flag is off silently loses
//! those declarations rather than reporting them. They are flipped once per process, before anything
//! is parsed.
//!
//! The flips themselves are not written here. There is one bootstrap, in the crate that owns the
//! style facade, and these tests run against that one — a second copy of the list would drift, and
//! the drift would be invisible: these tests would go on passing while exercising a differently
//! configured engine from the one the framework ships.

/// Turns on every CSS feature this framework targets, once per process.
pub(crate) fn enable_css_features() {
    zgui_css::prefs::enable_css_features();
}
