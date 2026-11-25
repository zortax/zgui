//! How precisely the default renderer holds a group of content.

/// How precisely a plain group holds its content.
///
/// A plain group has no filter and composites by source-over. Every other group is held in
/// sixteen-bit float at each precision.
///
/// A renderer starts with the precision that [`GroupPrecision::VARIABLE`] names, or with the one
/// the application set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GroupPrecision {
    /// A plain group is held in eight bits per channel.
    ///
    /// Its target takes half the memory of a [`GroupPrecision::High`] one. Each blend into the
    /// target rounds to eight bits, so a channel can differ from [`GroupPrecision::High`] by up to
    /// two steps in 255.
    #[default]
    Compact,
    /// Every group is held in sixteen-bit float.
    ///
    /// Use it when stacked translucent content inside a group must not show banding.
    High,
}

impl GroupPrecision {
    /// The variable that names the precision a new renderer starts with.
    ///
    /// Set `ZGUI_GROUP_PRECISION=high` for [`GroupPrecision::High`] or `compact` for
    /// [`GroupPrecision::Compact`]. Case and surrounding spaces are ignored. Other values are
    /// ignored with a warning.
    pub const VARIABLE: &str = "ZGUI_GROUP_PRECISION";

    /// The precision that [`GroupPrecision::VARIABLE`] names, if it names one.
    ///
    /// Read once in a process.
    pub fn from_environment() -> Option<Self> {
        static NAMED: std::sync::OnceLock<Option<GroupPrecision>> = std::sync::OnceLock::new();
        *NAMED.get_or_init(|| {
            let value = std::env::var(Self::VARIABLE).ok()?;
            let named = Self::parse(&value);
            if named.is_none() {
                tracing::warn!(
                    variable = Self::VARIABLE,
                    value = %value,
                    "unknown group precision, expected `high` or `compact`"
                );
            }
            named
        })
    }

    /// The precision `value` names: `high` or `compact`, in any case.
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("high") {
            Some(Self::High)
        } else if value.eq_ignore_ascii_case("compact") {
            Some(Self::Compact)
        } else {
            None
        }
    }

    /// Returns the precision the compositor's wgpu renderer holds groups at.
    pub(crate) fn wgpu(self) -> iceblink_wgpu::GroupPrecision {
        match self {
            Self::High => iceblink_wgpu::GroupPrecision::High,
            Self::Compact => iceblink_wgpu::GroupPrecision::Compact,
        }
    }
}
