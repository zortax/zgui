//! Where the compositor runs.

/// The thread a compositor builds and draws its frames on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CompositorDriver {
    /// A thread of its own, which keeps animations and compositor scrolling at the display's rate
    /// while the application's thread is busy.
    #[default]
    Threaded,
    /// The thread that runs the application's windows.
    InProcess,
}

impl CompositorDriver {
    /// The variable that names the driver, overriding the one the application chose.
    ///
    /// Set `ZGUI_DRIVER=threaded` for [`CompositorDriver::Threaded`] or `local` for
    /// [`CompositorDriver::InProcess`]. Case and surrounding spaces are ignored. Other values are
    /// ignored with a warning.
    pub const VARIABLE: &str = "ZGUI_DRIVER";

    /// The driver that [`CompositorDriver::VARIABLE`] names, if it names one.
    ///
    /// Read once in a process.
    pub fn from_environment() -> Option<Self> {
        static NAMED: std::sync::OnceLock<Option<CompositorDriver>> = std::sync::OnceLock::new();
        *NAMED.get_or_init(|| {
            let value = std::env::var(Self::VARIABLE).ok()?;
            let named = Self::parse(&value);
            if named.is_none() {
                tracing::warn!(
                    variable = Self::VARIABLE,
                    value = %value,
                    "unknown compositor driver, expected `threaded` or `local`"
                );
            }
            named
        })
    }

    /// The driver `value` names: `threaded` or `local`, in any case.
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("threaded") {
            Some(Self::Threaded)
        } else if value.eq_ignore_ascii_case("local") {
            Some(Self::InProcess)
        } else {
            None
        }
    }
}
