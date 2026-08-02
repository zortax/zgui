//! What is known about an output.

use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_vocab::SharedString;

/// What is known about one output.
///
/// This is for placing and sizing a window before it exists and for reporting to the user which
/// screen is which. It is deliberately *not* where a surface's scale factor comes from: a surface
/// can be shown at a scale that is not its monitor's, and reading the monitor's instead produces
/// a window that is subtly the wrong size on exactly the configurations that are hardest to test.
/// Ask the surface.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MonitorInfo {
    /// What the platform calls this output, when it says.
    pub name: Option<SharedString>,
    /// Where the output's top-left corner sits in the desktop's coordinate space.
    pub position: Point<DevicePx, Device>,
    /// How large the output is, in physical pixels.
    pub size: Size<DevicePx, Device>,
    /// The scale this output is presented at.
    pub scale_factor: f64,
    /// How many frames per second the output refreshes, in thousandths.
    ///
    /// Thousandths rather than a whole number because the rates that matter are not whole: a
    /// display advertised as sixty hertz usually runs at 59.94, and a deadline computed against
    /// sixty either misses a frame or presents twice.
    pub refresh_rate_millihertz: Option<u32>,
}

impl MonitorInfo {
    /// An output at `position`, `size` pixels large, presented at `scale_factor`.
    ///
    /// The name and the refresh rate are absent until a backend fills them in, because a backend
    /// that does not know them must not invent them.
    pub const fn new(
        position: Point<DevicePx, Device>,
        size: Size<DevicePx, Device>,
        scale_factor: f64,
    ) -> Self {
        Self {
            name: None,
            position,
            size,
            scale_factor,
            refresh_rate_millihertz: None,
        }
    }

    /// The same output with a name.
    pub fn with_name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The same output with a known refresh rate, in thousandths of a hertz.
    pub const fn with_refresh_rate_millihertz(mut self, rate: u32) -> Self {
        self.refresh_rate_millihertz = Some(rate);
        self
    }

    /// The refresh interval, or the sixty-hertz fallback when the output does not say.
    ///
    /// A fallback is needed because a deadline has to be computed either way, and it is stated
    /// here, once, rather than invented separately wherever a rate is missing.
    ///
    /// ```
    /// use std::time::Duration;
    /// use zgui_geom::{DevicePx, Point, Size};
    /// use zgui_platform::MonitorInfo;
    ///
    /// let monitor = MonitorInfo::new(
    ///     Point::new(DevicePx(0.0), DevicePx(0.0)),
    ///     Size::new(DevicePx(1920.0), DevicePx(1080.0)),
    ///     1.0,
    /// )
    /// .with_refresh_rate_millihertz(165_000);
    ///
    /// assert!(monitor.refresh_interval() < Duration::from_millis(7));
    /// ```
    pub fn refresh_interval(&self) -> std::time::Duration {
        refresh_interval(self.refresh_rate_millihertz)
    }
}

/// The refresh interval for a rate in thousandths of a hertz, falling back to sixty hertz.
///
/// A rate of zero is treated as no rate at all rather than as an infinitely fast display.
pub fn refresh_interval(millihertz: Option<u32>) -> std::time::Duration {
    const FALLBACK_MILLIHERTZ: u32 = 60_000;
    let rate = millihertz
        .filter(|rate| *rate > 0)
        .unwrap_or(FALLBACK_MILLIHERTZ);
    std::time::Duration::from_secs_f64(1_000.0 / f64::from(rate))
}

#[cfg(test)]
mod tests {
    use super::{MonitorInfo, refresh_interval};
    use std::time::Duration;
    use zgui_geom::{DevicePx, Point, Size};

    #[test]
    fn an_output_starts_with_only_what_a_backend_actually_knows() {
        let monitor = MonitorInfo::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
            1.5,
        );
        assert_eq!(monitor.name, None);
        assert_eq!(monitor.refresh_rate_millihertz, None);
        assert_eq!(monitor.refresh_interval(), refresh_interval(None));

        let named = monitor
            .with_name("DP-1")
            .with_refresh_rate_millihertz(144_000);
        assert_eq!(named.name.as_deref(), Some("DP-1"));
        assert!(named.refresh_interval() < refresh_interval(None));
    }

    #[test]
    fn a_missing_rate_falls_back_to_sixty_hertz() {
        assert_eq!(refresh_interval(None), refresh_interval(Some(60_000)));
        assert_eq!(refresh_interval(Some(0)), refresh_interval(Some(60_000)));
    }

    #[test]
    fn a_faster_display_gets_a_shorter_interval() {
        assert!(refresh_interval(Some(165_000)) < refresh_interval(Some(60_000)));
        assert!(refresh_interval(Some(59_940)) > refresh_interval(Some(60_000)));
    }

    #[test]
    fn sixty_hertz_is_about_sixteen_and_a_half_milliseconds() {
        let interval = refresh_interval(Some(60_000));
        assert!(interval > Duration::from_micros(16_600));
        assert!(interval < Duration::from_micros(16_700));
    }
}
