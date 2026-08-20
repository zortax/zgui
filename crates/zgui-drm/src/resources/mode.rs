//! One way a display can be driven, and how to state one from a timing table.
//!
//! A [`Mode`] usually arrives from a connector, which lists what the display says it accepts. This
//! module is the other route: a mode a caller states itself, from the numbers a timing table
//! carries. The kernel allows a display to be driven at a mode it never listed, and a test needs
//! one; both go through [`Mode::builder`].

use std::fmt;
use std::ops::BitOr;

use crate::sys;

/// The timings of one axis: what is visible, where the sync pulse sits, and the whole scan.
///
/// Every number counts from the start of the visible part, which is how a timing table states
/// them. `1920 2008 2052 2200` is the horizontal axis of 1920x1080 at 60 Hz, so `visible` is 1920
/// and `total` is 2200.
///
/// The kernel requires the four to rise: `visible <= sync_start <= sync_end <= total`. A mode that
/// breaks the order is refused when it reaches the hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axis {
    /// How much of the scan reaches the screen: pixels across, or lines down.
    pub visible: u16,
    /// Where the sync pulse starts.
    pub sync_start: u16,
    /// Where the sync pulse ends.
    pub sync_end: u16,
    /// The whole scan, the visible part and the blanking together.
    pub total: u16,
}

/// What a mode says about itself beyond its timings, as a set of `DRM_MODE_FLAG_*` bits.
///
/// Combined with `|`. The two polarities are the ones real hardware reads: a display handed the
/// wrong one shows a shifted picture, or none at all. The value is public so that a caller with a
/// flag this crate does not name — a stereo layout, a clock divider — can still state it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModeFlags(pub u32);

impl ModeFlags {
    /// Nothing stated, so the hardware keeps its own defaults.
    pub const NONE: Self = Self(0);
    /// The horizontal sync pulse is active high.
    pub const HSYNC_POSITIVE: Self = Self(sys::DRM_MODE_FLAG_PHSYNC);
    /// The horizontal sync pulse is active low.
    pub const HSYNC_NEGATIVE: Self = Self(sys::DRM_MODE_FLAG_NHSYNC);
    /// The vertical sync pulse is active high.
    pub const VSYNC_POSITIVE: Self = Self(sys::DRM_MODE_FLAG_PVSYNC);
    /// The vertical sync pulse is active low.
    pub const VSYNC_NEGATIVE: Self = Self(sys::DRM_MODE_FLAG_NVSYNC);
    /// The lines arrive as two interlaced fields.
    pub const INTERLACE: Self = Self(sys::DRM_MODE_FLAG_INTERLACE);
    /// Every line is scanned twice.
    pub const DOUBLE_SCAN: Self = Self(sys::DRM_MODE_FLAG_DBLSCAN);
}

impl BitOr for ModeFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One way a display can be driven: an extent, a rate, and the timings that produce them.
///
/// The kernel's own structure is kept whole, because `MODE_SETCRTC` and the atomic mode blob both
/// take it back as it stands. Unpacking it and building it again would be a chance to get a timing
/// wrong for no gain.
#[derive(Clone, Copy)]
pub struct Mode {
    /// The kernel's structure, passed back untouched when a mode is set.
    pub(crate) raw: sys::drm_mode_modeinfo,
}

impl Mode {
    /// Starts a mode from the pixel clock in kilohertz and the two axes.
    ///
    /// Nothing else can supply these three. The clock says how fast pixels leave the hardware, and
    /// each axis says how many of them there are and where the sync pulse falls; together they
    /// give the extent and the refresh rate, and no display can be driven without all of them.
    ///
    /// Everything a timing table leaves out has a default: no flags, the mode type of a
    /// user-defined mode, a name read off the extent, and the whole-hertz refresh rate worked out
    /// from the timings. [`ModeBuilder`] is where each of those is stated instead.
    ///
    /// A caller that wants a mode the display listed already has one, from
    /// [`Connector::preferred_mode`](crate::resources::Connector::preferred_mode) or from
    /// [`Connector::modes`](crate::resources::Connector::modes). This is for the other case.
    ///
    /// A timing table states each axis as four rising numbers, and they go across as they stand:
    ///
    /// ```
    /// use zgui_drm::resources::{Axis, Mode};
    ///
    /// // 1920x1080 at 60 Hz: 148.5 MHz, "1920 2008 2052 2200" across, "1080 1084 1089 1125" down.
    /// let mode = Mode::builder(
    ///     148_500,
    ///     Axis { visible: 1920, sync_start: 2008, sync_end: 2052, total: 2200 },
    ///     Axis { visible: 1080, sync_start: 1084, sync_end: 1089, total: 1125 },
    /// )
    /// .build();
    ///
    /// assert_eq!(mode.width(), 1920);
    /// assert_eq!(mode.height(), 1080);
    /// assert_eq!(
    ///     mode.refresh_rate_millihertz(),
    ///     60_000,
    ///     "the rate follows from the clock and the two totals",
    /// );
    /// ```
    pub fn builder(clock_khz: u32, horizontal: Axis, vertical: Axis) -> ModeBuilder {
        ModeBuilder {
            raw: sys::drm_mode_modeinfo {
                clock: clock_khz,
                hdisplay: horizontal.visible,
                hsync_start: horizontal.sync_start,
                hsync_end: horizontal.sync_end,
                htotal: horizontal.total,
                vdisplay: vertical.visible,
                vsync_start: vertical.sync_start,
                vsync_end: vertical.sync_end,
                vtotal: vertical.total,
                // A mode a caller states is a user-defined mode, which is the kernel's own name
                // for this type. `ModeBuilder::preferred` adds the preference bit to it.
                type_: sys::DRM_MODE_TYPE_USERDEF,
                ..Default::default()
            },
        }
    }

    /// Returns how wide this mode is, in pixels.
    pub fn width(&self) -> u32 {
        u32::from(self.raw.hdisplay)
    }

    /// Returns how tall this mode is, in pixels.
    pub fn height(&self) -> u32 {
        u32::from(self.raw.vdisplay)
    }

    /// Returns how many times a second this mode refreshes, in millihertz.
    ///
    /// Computed from the pixel clock and the two totals. The kernel's own `vrefresh` field is
    /// documented as approximate and holds whole hertz, and a frame loop pacing against 60 where
    /// the display runs at 59.94 drifts by a frame every seventeen seconds.
    pub fn refresh_rate_millihertz(&self) -> u32 {
        let total = u64::from(self.raw.htotal) * u64::from(self.raw.vtotal);
        if total == 0 {
            return 0;
        }
        // `clock` is in kilohertz. A thousand for that, and a thousand for millihertz.
        u32::try_from(u64::from(self.raw.clock) * 1_000_000 / total).unwrap_or(0)
    }

    /// Returns `true` when this is the mode the display prefers.
    ///
    /// The `DRM_MODE_TYPE_PREFERRED` bit of the mode's type. A mode a caller built carries it only
    /// because [`ModeBuilder::preferred`] was called.
    pub fn is_preferred(&self) -> bool {
        self.raw.type_ & sys::DRM_MODE_TYPE_PREFERRED != 0
    }
}

/// Written by hand, because the derived form prints the whole kernel structure: the skew, the
/// scan, an undecoded flag word and the name as bytes. A mode is named to a reader by its extent,
/// its rate, and whether the display asked for it.
impl fmt::Debug for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mode")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("refresh_rate_millihertz", &self.refresh_rate_millihertz())
            .field("preferred", &self.is_preferred())
            .finish()
    }
}

/// A mode being built from a timing table.
///
/// [`Mode::builder`] starts one with the clock and the two axes. Every method here states one of
/// the things a timing table leaves out, and [`ModeBuilder::build`] answers the mode.
///
/// Two fields of the kernel's structure have no method: the horizontal skew and the vertical scan.
/// Both go across as zero, the value every mode a display lists carries, which the hardware reads
/// as "no skew, one scan per line". Either one gains a method later without touching a call that
/// is already written, which is why this is a builder.
#[derive(Clone, Copy)]
pub struct ModeBuilder {
    /// The structure being filled, already carrying the clock and both axes.
    raw: sys::drm_mode_modeinfo,
}

impl ModeBuilder {
    /// States the flags, replacing whatever was stated before.
    pub fn flags(mut self, flags: ModeFlags) -> Self {
        self.raw.flags = flags.0;
        self
    }

    /// Marks this as the mode the display prefers.
    ///
    /// [`Mode::is_preferred`] reads it back. A mode a caller built is preferred only
    /// because the caller said so; what a display asks for arrives on the modes the connector
    /// lists.
    pub fn preferred(mut self) -> Self {
        self.raw.type_ |= sys::DRM_MODE_TYPE_PREFERRED;
        self
    }

    /// Returns the mode.
    ///
    /// The refresh rate the kernel carries in whole hertz and the name are both filled in here,
    /// from the timings and the extent, the same way the kernel fills them for a mode it read off
    /// a display.
    ///
    /// The timings are handed over as stated. The kernel checks that they rise —
    /// `visible <= sync_start <= sync_end <= total` on both axes — and refuses the commit that
    /// carries a mode that does not, so a mistake here is reported when the mode reaches the
    /// hardware.
    pub fn build(mut self) -> Mode {
        let mode = Mode { raw: self.raw };
        // The kernel's own field, which it documents as approximate and rounds to whole hertz.
        // It is filled so that a driver or a debugging tool reading it sees the rate the timings
        // give, and `Mode::refresh_rate_millihertz` keeps reading the timings.
        self.raw.vrefresh = (mode.refresh_rate_millihertz() + 500) / 1_000;
        write_name(&mut self.raw);
        Mode { raw: self.raw }
    }
}

/// Written by hand for the reason [`Mode`]'s own is: the derived form prints the kernel structure.
impl fmt::Debug for ModeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ModeBuilder")
            .field(&Mode { raw: self.raw })
            .finish()
    }
}

/// Names a mode after its extent, the way the kernel names one it read off a display.
///
/// An interlaced mode carries a trailing `i`, which is the kernel's own spelling. The field holds
/// 32 bytes and a name that long is cut, leaving the last byte zero so the string stays
/// terminated.
fn write_name(raw: &mut sys::drm_mode_modeinfo) {
    let interlaced = if raw.flags & sys::DRM_MODE_FLAG_INTERLACE == 0 {
        ""
    } else {
        "i"
    };
    let name = format!("{}x{}{interlaced}", raw.hdisplay, raw.vdisplay);
    let room = raw.name.len() - 1;
    for (slot, byte) in raw.name.iter_mut().zip(name.bytes().take(room)) {
        *slot = byte as _;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The horizontal axis of 1920x1080 at 60 Hz, as the timing table states it.
    const HORIZONTAL: Axis = Axis {
        visible: 1920,
        sync_start: 2008,
        sync_end: 2052,
        total: 2200,
    };

    /// The vertical axis of the same mode.
    const VERTICAL: Axis = Axis {
        visible: 1080,
        sync_start: 1084,
        sync_end: 1089,
        total: 1125,
    };

    /// Returns a mode with these timings and nothing else filled in.
    fn timed(clock: u32, htotal: u16, vtotal: u16) -> Mode {
        Mode::builder(
            clock,
            Axis {
                total: htotal,
                ..HORIZONTAL
            },
            Axis {
                total: vtotal,
                ..VERTICAL
            },
        )
        .build()
    }

    #[test]
    fn a_built_mode_answers_the_extent_and_the_rate_it_was_given() {
        let mode = Mode::builder(148_500, HORIZONTAL, VERTICAL).build();

        assert_eq!(
            mode.width(),
            1920,
            "the width is the horizontal visible part"
        );
        assert_eq!(mode.height(), 1080, "the height is the vertical one");
        assert_eq!(
            mode.refresh_rate_millihertz(),
            60_000,
            "148.5 MHz over 2200 x 1125 pixels is 60 Hz"
        );
        assert!(
            !mode.is_preferred(),
            "a mode is preferred only because the caller said so"
        );
    }

    #[test]
    fn a_built_mode_keeps_the_fraction_in_its_rate() {
        // The same mode at 59.94, which the kernel's whole-hertz field calls 60. Pacing a frame
        // loop against 60 on a display running this drifts a frame every seventeen seconds, and
        // that is why the rate is computed from the timings.
        let mode = Mode::builder(148_352, HORIZONTAL, VERTICAL).build();

        assert_eq!(mode.refresh_rate_millihertz(), 59_940);
        assert_eq!(
            mode.raw.vrefresh, 60,
            "the kernel's own field is the same rate rounded to whole hertz"
        );
    }

    #[test]
    fn a_mode_built_as_preferred_says_so() {
        let mode = Mode::builder(148_500, HORIZONTAL, VERTICAL)
            .preferred()
            .build();

        assert!(mode.is_preferred());
        assert_eq!(mode.width(), 1920, "and carries the extent it was built at");
    }

    #[test]
    fn the_timings_reach_the_kernels_structure_where_the_kernel_reads_them() {
        // Every one of these is a field a commit hands over, and swapping any two of them is
        // accepted by the compiler and produces a picture that is shifted or absent.
        let mode = Mode::builder(148_500, HORIZONTAL, VERTICAL).build();

        assert_eq!(mode.raw.clock, 148_500);
        assert_eq!(mode.raw.hdisplay, 1920);
        assert_eq!(mode.raw.hsync_start, 2008);
        assert_eq!(mode.raw.hsync_end, 2052);
        assert_eq!(mode.raw.htotal, 2200);
        assert_eq!(mode.raw.vdisplay, 1080);
        assert_eq!(mode.raw.vsync_start, 1084);
        assert_eq!(mode.raw.vsync_end, 1089);
        assert_eq!(mode.raw.vtotal, 1125);
        assert_eq!(mode.raw.hskew, 0, "no timing table states a skew");
        assert_eq!(mode.raw.vscan, 0, "or a scan other than one per line");
        assert_eq!(
            mode.raw.type_,
            sys::DRM_MODE_TYPE_USERDEF,
            "a mode a caller states is a user-defined mode"
        );
    }

    #[test]
    fn the_flags_a_mode_is_built_with_are_the_bits_the_kernel_names() {
        let mode = Mode::builder(148_500, HORIZONTAL, VERTICAL)
            .flags(ModeFlags::HSYNC_POSITIVE | ModeFlags::VSYNC_POSITIVE)
            .build();

        assert_eq!(
            mode.raw.flags,
            sys::DRM_MODE_FLAG_PHSYNC | sys::DRM_MODE_FLAG_PVSYNC
        );
        assert_eq!(ModeFlags::NONE.0, 0, "and no flag at all is no bit at all");
    }

    #[test]
    fn a_mode_is_named_after_its_extent() {
        let mode = Mode::builder(148_500, HORIZONTAL, VERTICAL).build();
        assert_eq!(name_of(&mode), "1920x1080");

        let interlaced = Mode::builder(148_500, HORIZONTAL, VERTICAL)
            .flags(ModeFlags::INTERLACE)
            .build();
        assert_eq!(
            name_of(&interlaced),
            "1920x1080i",
            "an interlaced mode carries the kernel's own trailing letter"
        );
    }

    #[test]
    fn a_mode_with_no_timings_reports_no_rate_rather_than_dividing_by_zero() {
        assert_eq!(timed(148_500, 0, 0).refresh_rate_millihertz(), 0);
    }

    #[test]
    fn a_modes_rate_comes_from_its_timings() {
        assert_eq!(timed(148_500, 2200, 1125).refresh_rate_millihertz(), 60_000);
        assert_eq!(timed(148_352, 2200, 1125).refresh_rate_millihertz(), 59_940);
    }

    /// Returns the name field as a string, up to the byte that terminates it.
    fn name_of(mode: &Mode) -> String {
        mode.raw
            .name
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| *byte as u8 as char)
            .collect()
    }
}
