//! What a sink says when it cannot do as it is asked.

/// A texture sink refused an operation.
///
/// The reason is a string rather than an enumeration because the interesting failures belong to
/// whatever is behind the sink — a device that ran out of memory, a device that was lost — and this
/// crate has no vocabulary for them and gains nothing from inventing one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the texture sink refused the operation: {reason}")]
pub struct SinkError {
    /// What went wrong, in the sink's own words.
    pub reason: String,
}

impl SinkError {
    /// A refusal with the given explanation.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}
