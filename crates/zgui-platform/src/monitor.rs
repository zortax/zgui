//! What is known about an output.

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Size};
use zgui_vocab::SharedString;

/// The name of one output, for as long as it is connected.
///
/// Two outputs connected at the same time never share an identifier. An output that is unplugged
/// and plugged in again can get a new one, so an application that must find the same screen again
/// matches [`MonitorInfo::name`] instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonitorId(u64);

impl MonitorId {
    /// The identifier a backend gives the output it knows as `raw`.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The number behind the identifier.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// How an output turns its picture before it shows it.
///
/// The rotations are counter-clockwise. A flip is a mirror around the vertical axis, applied
/// before the rotation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MonitorTransform {
    /// No change.
    #[default]
    Normal,
    /// A quarter turn.
    Rotated90,
    /// A half turn.
    Rotated180,
    /// Three quarter turns.
    Rotated270,
    /// A mirror image.
    Flipped,
    /// A mirror image, then a quarter turn.
    Flipped90,
    /// A mirror image, then a half turn.
    Flipped180,
    /// A mirror image, then three quarter turns.
    Flipped270,
}

impl MonitorTransform {
    /// Whether the transform exchanges the width and the height.
    ///
    /// ```
    /// use zgui_platform::MonitorTransform;
    ///
    /// assert!(MonitorTransform::Rotated90.swaps_axes());
    /// assert!(!MonitorTransform::Flipped180.swaps_axes());
    /// ```
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Rotated90 | Self::Rotated270 | Self::Flipped90 | Self::Flipped270
        )
    }
}

/// Which output a surface is asked to open on.
///
/// ```
/// use zgui_platform::{MonitorId, MonitorTarget};
///
/// assert_eq!(MonitorTarget::from("DP-1"), MonitorTarget::Name("DP-1".to_owned()));
/// assert_eq!(MonitorTarget::from(MonitorId::new(4)), MonitorTarget::Id(MonitorId::new(4)));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MonitorTarget {
    /// The output with this identifier, as [`MonitorInfo::id`] reports it.
    Id(MonitorId),
    /// The output with this name, as [`MonitorInfo::name`] reports it.
    Name(String),
}

impl MonitorTarget {
    /// Whether `monitor` is the output this names.
    pub fn matches(&self, monitor: &MonitorInfo) -> bool {
        match self {
            Self::Id(id) => monitor.id == Some(*id),
            Self::Name(name) => monitor.name.as_deref() == Some(name.as_str()),
        }
    }
}

impl From<MonitorId> for MonitorTarget {
    fn from(id: MonitorId) -> Self {
        Self::Id(id)
    }
}

impl From<String> for MonitorTarget {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

impl From<&str> for MonitorTarget {
    fn from(name: &str) -> Self {
        Self::Name(name.to_owned())
    }
}

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
    /// The identifier of this output while it is connected, when the platform gives one.
    pub id: Option<MonitorId>,
    /// What the platform calls this output, when it says.
    ///
    /// On most desktops this is the connector, such as `DP-1` or `HDMI-A-1`.
    pub name: Option<SharedString>,
    /// A description of this output for a person to read, when the platform gives one.
    pub description: Option<SharedString>,
    /// Who made the display, when the platform says.
    pub make: Option<SharedString>,
    /// The model of the display, when the platform says.
    pub model: Option<SharedString>,
    /// Where the output's top-left corner sits in the desktop's coordinate space.
    pub position: Point<DevicePx, Device>,
    /// How large the output is, in physical pixels.
    pub size: Size<DevicePx, Device>,
    /// The scale this output is presented at.
    ///
    /// A whole number on a desktop that gives only whole numbers. See
    /// [`MonitorInfo::fractional_scale`] for the scale a desktop that scales by fractions uses.
    pub scale_factor: f64,
    /// Where the output sits and how large it is, in the desktop's logical pixels, when the
    /// platform says.
    ///
    /// This is the space a shell surface is placed and sized in.
    pub logical: Option<Rect<CssPx, Css>>,
    /// The scale this output is presented at as a fraction, when it is known.
    ///
    /// A backend that knows both the physical and the logical extent gives their ratio, such as
    /// 1.25 or 1.5.
    pub fractional_scale: Option<f64>,
    /// How the output turns its picture.
    pub transform: MonitorTransform,
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
            id: None,
            name: None,
            description: None,
            make: None,
            model: None,
            position,
            size,
            scale_factor,
            logical: None,
            fractional_scale: None,
            transform: MonitorTransform::Normal,
            refresh_rate_millihertz: None,
        }
    }

    /// The same output with an identifier.
    pub const fn with_id(mut self, id: MonitorId) -> Self {
        self.id = Some(id);
        self
    }

    /// The same output with a name.
    pub fn with_name(mut self, name: impl Into<SharedString>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The same output with a description for a person to read.
    pub fn with_description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// The same output with the maker and the model of its display.
    pub fn with_make_and_model(
        mut self,
        make: impl Into<SharedString>,
        model: impl Into<SharedString>,
    ) -> Self {
        self.make = Some(make.into());
        self.model = Some(model.into());
        self
    }

    /// The same output with its place and extent in logical pixels.
    pub const fn with_logical(mut self, logical: Rect<CssPx, Css>) -> Self {
        self.logical = Some(logical);
        self
    }

    /// The same output with a known fractional scale.
    pub const fn with_fractional_scale(mut self, scale: f64) -> Self {
        self.fractional_scale = Some(scale);
        self
    }

    /// The same output with a transform.
    pub const fn with_transform(mut self, transform: MonitorTransform) -> Self {
        self.transform = transform;
        self
    }

    /// The scale to draw at on this output: the fractional scale when it is known, else the
    /// whole-number one.
    pub fn effective_scale(&self) -> f64 {
        self.fractional_scale
            .filter(|scale| *scale > 0.0)
            .unwrap_or(self.scale_factor)
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
    use super::{MonitorId, MonitorInfo, MonitorTarget, MonitorTransform, refresh_interval};
    use std::time::Duration;
    use zgui_geom::{CssPx, DevicePx, Point, Rect, Size};

    fn screen() -> MonitorInfo {
        MonitorInfo::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(2560.0), DevicePx(1440.0)),
            2.0,
        )
    }

    #[test]
    fn an_output_carries_only_the_facts_a_backend_gave_it() {
        let bare = screen();
        assert_eq!(bare.id, None);
        assert_eq!(bare.description, None);
        assert_eq!(bare.logical, None);
        assert_eq!(bare.transform, MonitorTransform::Normal);
        assert_eq!(bare.effective_scale(), 2.0);

        let logical = Rect::new(
            Point::new(CssPx(1920.0), CssPx(0.0)),
            Size::new(CssPx(2048.0), CssPx(1152.0)),
        );
        let full = bare
            .with_id(MonitorId::new(7))
            .with_description("Dell U2720Q (DP-2)")
            .with_make_and_model("Dell Inc.", "U2720Q")
            .with_logical(logical)
            .with_fractional_scale(1.25)
            .with_transform(MonitorTransform::Rotated90);
        assert_eq!(full.id, Some(MonitorId::new(7)));
        assert_eq!(full.make.as_deref(), Some("Dell Inc."));
        assert_eq!(full.model.as_deref(), Some("U2720Q"));
        assert_eq!(full.logical, Some(logical));
        assert_eq!(full.effective_scale(), 1.25);
        assert!(full.transform.swaps_axes());
    }

    #[test]
    fn a_target_matches_by_identifier_or_by_name() {
        let monitor = screen().with_id(MonitorId::new(3)).with_name("DP-1");
        assert!(MonitorTarget::from(MonitorId::new(3)).matches(&monitor));
        assert!(!MonitorTarget::from(MonitorId::new(4)).matches(&monitor));
        assert!(MonitorTarget::from("DP-1").matches(&monitor));
        assert!(!MonitorTarget::from("DP-2").matches(&monitor));
        // An output with no identifier matches no identifier.
        assert!(!MonitorTarget::from(MonitorId::new(3)).matches(&screen()));
    }

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
