//! Why an application could not start, or could not keep going.

use thiserror::Error;

/// Why an application stopped.
///
/// Every variant is a condition that cannot be recovered from where it is found, and each names
/// what was tried. In particular there is **no silent fallback to drawing nowhere**: a machine
/// with no usable graphics device is told so, because a window that opens and never paints is
/// worse than a window that never opens.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AppError {
    /// The platform could not do something the application needs.
    #[error("the platform could not satisfy the application: {0}")]
    Platform(#[from] zgui_platform::PlatformError),

    /// No graphics adapter on this machine could be used.
    #[error("{0}")]
    GpuUnavailable(#[from] zgui_render::GpuUnavailable),

    /// Another asynchronous runtime already holds this process's executor slot.
    ///
    /// The reactive layer runs its tasks on the thread that owns the window, and it cannot do that
    /// if something else has claimed the slot those tasks are spawned through.
    #[error("another async executor is already installed in this process")]
    ForeignExecutor,

    /// The application's own stylesheet could not be understood.
    ///
    /// A dropped declaration is not this: a sheet with one unrecognised property still applies,
    /// and the diagnostic is reported rather than fatal. This is a sheet a caller *asked* to be
    /// treated as fatal.
    #[error("the application stylesheet was rejected: {0}")]
    Stylesheet(String),

    /// Every document identity has been handed out.
    ///
    /// One is minted per window opened and none is ever reused, because a node handle carries the
    /// document it belongs to and a reused identity would let a stale handle resolve inside an
    /// unrelated later window. Four thousand opens in one process is the ceiling that buys that,
    /// and reaching it is reported rather than allowed to alias.
    #[error("this process has opened every window it can name")]
    DocumentsExhausted,
}

impl From<zgui_reactive::InstallError> for AppError {
    fn from(error: zgui_reactive::InstallError) -> Self {
        match error {
            zgui_reactive::InstallError::ForeignExecutor => Self::ForeignExecutor,
            // Any later reason an executor cannot be installed is still the same fact from a
            // window's side: this thread is not ours to run reactivity on.
            _ => Self::ForeignExecutor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn a_foreign_executor_says_what_is_wrong_rather_than_what_failed() {
        let error = AppError::from(zgui_reactive::InstallError::ForeignExecutor);
        assert!(error.to_string().contains("executor"));
    }

    #[test]
    fn every_error_reads_as_a_sentence() {
        let error = AppError::Stylesheet("no `}` at line 4".to_owned());
        assert_eq!(
            error.to_string(),
            "the application stylesheet was rejected: no `}` at line 4"
        );
    }
}
