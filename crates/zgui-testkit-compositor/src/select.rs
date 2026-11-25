//! Reading which implementations the environment selects.

use crate::Unavailable;

/// A compositor implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositorKind {
    /// The reference compositor, `iceblink-core`.
    Core,
    /// The second implementation, written from the spec alone.
    Naive,
}

/// A driver of the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverKind {
    /// The driver of `iceblink-driver` on the test's own thread.
    Local,
    /// The driver of `iceblink_driver::thread` on a thread of its own.
    Threaded,
    /// The driver of `iceblink_driver::thread` on a thread of its own, building frames at the
    /// display's rate.
    Paced,
}

/// A renderer implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererKind {
    /// The wgpu reference renderer, drawing offscreen.
    Wgpu,
    /// The CPU reference renderer.
    Cpu,
    /// The recording renderer, which keeps frames and draws no pixels.
    Capture,
    /// A renderer that accepts every frame and keeps nothing.
    Null,
}

impl CompositorKind {
    /// Returns the kind `ZGUI_COMPOSITOR` selects.
    ///
    /// # Errors
    ///
    /// Fails when the variable holds a value other than `core` or `naive`.
    pub fn from_environment() -> Result<Self, Unavailable> {
        Self::parse(read("ZGUI_COMPOSITOR").as_deref())
    }

    /// Returns the kind `value` names, `Core` when there is no value.
    ///
    /// # Errors
    ///
    /// Fails when `value` is other than `core` or `naive`.
    pub fn parse(value: Option<&str>) -> Result<Self, Unavailable> {
        match value {
            None | Some("core") => Ok(Self::Core),
            Some("naive") => Ok(Self::Naive),
            Some(other) => Err(Unavailable::unknown(
                "ZGUI_COMPOSITOR",
                other,
                "core, naive",
            )),
        }
    }
}

impl DriverKind {
    /// Returns the kind `ZGUI_DRIVER` selects.
    ///
    /// # Errors
    ///
    /// Fails when the variable holds a value other than `local`, `threaded` or `paced`.
    pub fn from_environment() -> Result<Self, Unavailable> {
        Self::parse(read("ZGUI_DRIVER").as_deref())
    }

    /// Returns the kind `value` names, `Local` when there is no value.
    ///
    /// # Errors
    ///
    /// Fails when `value` is other than `local`, `threaded` or `paced`.
    pub fn parse(value: Option<&str>) -> Result<Self, Unavailable> {
        match value {
            None | Some("local") => Ok(Self::Local),
            Some("threaded") => Ok(Self::Threaded),
            Some("paced") => Ok(Self::Paced),
            Some(other) => Err(Unavailable::unknown(
                "ZGUI_DRIVER",
                other,
                "local, threaded, paced",
            )),
        }
    }
}

impl RendererKind {
    /// Returns the kind `ZGUI_RENDERER` selects.
    ///
    /// # Errors
    ///
    /// Fails when the variable holds a value other than `wgpu`, `cpu`, `capture` or `null`.
    pub fn from_environment() -> Result<Self, Unavailable> {
        Self::parse(read("ZGUI_RENDERER").as_deref())
    }

    /// Returns the kind `value` names, `Capture` when there is no value.
    ///
    /// # Errors
    ///
    /// Fails when `value` is other than `wgpu`, `cpu`, `capture` or `null`.
    pub fn parse(value: Option<&str>) -> Result<Self, Unavailable> {
        match value {
            None | Some("capture") => Ok(Self::Capture),
            Some("wgpu") => Ok(Self::Wgpu),
            Some("cpu") => Ok(Self::Cpu),
            Some("null") => Ok(Self::Null),
            Some(other) => Err(Unavailable::unknown(
                "ZGUI_RENDERER",
                other,
                "wgpu, cpu, capture, null",
            )),
        }
    }
}

/// Returns the trimmed value of `variable`, or `None` when it is unset or empty.
fn read(variable: &str) -> Option<String> {
    let value = std::env::var(variable).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{CompositorKind, RendererKind};

    #[test]
    fn no_value_selects_the_defaults() {
        assert_eq!(CompositorKind::parse(None).unwrap(), CompositorKind::Core);
        assert_eq!(RendererKind::parse(None).unwrap(), RendererKind::Capture);
    }

    #[test]
    fn an_unknown_value_is_refused_with_the_known_ones() {
        let error = RendererKind::parse(Some("vulkan")).unwrap_err().to_string();
        assert!(
            error.contains("vulkan") && error.contains("capture"),
            "{error}"
        );
        assert!(CompositorKind::parse(Some("Core")).is_err());
    }
}
