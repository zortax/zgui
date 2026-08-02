//! Why an application could not start, or could not keep going.

/// What stopped an application.
///
/// Every variant names what was tried, and none of them is a quiet fallback: a machine with no
/// usable graphics device is reported as such rather than answered with a window that opens and
/// never paints.
pub type Error = zgui_runtime::AppError;
