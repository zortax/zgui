//! A connector: where a display is plugged in, and what modes it can be driven at.

use std::fmt;

use crate::device::Device;
use crate::error::{Error, Result};
use crate::ioctl;
use crate::resources::ATTEMPTS;
use crate::sys;

/// What kind of socket a connector is.
///
/// The kernel numbers these, and the numbering is part of the interface. Anything this crate has
/// not been taught keeps its number, so a device with a connector newer than this code still
/// reports something a person can act on.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    /// A VGA socket.
    Vga,
    /// A DVI socket, of any of its three kinds.
    Dvi,
    /// A composite socket.
    Composite,
    /// An internal panel.
    Panel,
    /// A DisplayPort socket.
    DisplayPort,
    /// An HDMI socket.
    Hdmi,
    /// A virtual connector that writes what it is sent back to memory.
    Writeback,
    /// Something this crate does not name, with the number the kernel gave it.
    Other(u32),
}

impl ConnectorKind {
    /// Returns the kind `raw` names.
    fn from_raw(raw: u32) -> Self {
        match raw {
            sys::DRM_MODE_CONNECTOR_VGA => Self::Vga,
            sys::DRM_MODE_CONNECTOR_DVII
            | sys::DRM_MODE_CONNECTOR_DVID
            | sys::DRM_MODE_CONNECTOR_DVIA => Self::Dvi,
            sys::DRM_MODE_CONNECTOR_Composite => Self::Composite,
            sys::DRM_MODE_CONNECTOR_LVDS | sys::DRM_MODE_CONNECTOR_eDP => Self::Panel,
            sys::DRM_MODE_CONNECTOR_DisplayPort => Self::DisplayPort,
            sys::DRM_MODE_CONNECTOR_HDMIA | sys::DRM_MODE_CONNECTOR_HDMIB => Self::Hdmi,
            sys::DRM_MODE_CONNECTOR_WRITEBACK => Self::Writeback,
            other => Self::Other(other),
        }
    }
}

/// One way a display can be driven: an extent, a rate, and the timings that produce them.
///
/// The kernel's own structure is kept whole rather than unpacked, because it is what
/// `MODE_SETCRTC` and the atomic mode blob both take back. Unpacking it and building it again
/// would be a chance to get a timing wrong for no gain.
#[derive(Clone, Copy)]
pub struct Mode {
    /// The kernel's structure, passed back untouched when a mode is set.
    pub(crate) raw: sys::drm_mode_modeinfo,
}

impl Mode {
    /// How wide, in pixels.
    pub fn width(&self) -> u32 {
        u32::from(self.raw.hdisplay)
    }

    /// How tall, in pixels.
    pub fn height(&self) -> u32 {
        u32::from(self.raw.vdisplay)
    }

    /// How many times a second this mode refreshes, in millihertz.
    ///
    /// Computed from the timings rather than read from `vrefresh`, because that field is rounded
    /// to whole hertz and a frame loop pacing against 60 where the display runs at 59.94 drifts by
    /// a frame every seventeen seconds.
    pub fn refresh_rate_millihertz(&self) -> u32 {
        let total = u64::from(self.raw.htotal) * u64::from(self.raw.vtotal);
        if total == 0 {
            return 0;
        }
        // `clock` is in kilohertz. A thousand for that, and a thousand for millihertz.
        u32::try_from(u64::from(self.raw.clock) * 1_000_000 / total).unwrap_or(0)
    }

    /// Whether this is the mode the display prefers.
    pub fn is_preferred(&self) -> bool {
        self.raw.type_ & sys::DRM_MODE_TYPE_PREFERRED != 0
    }
}

/// Written by hand, because the derived form prints the whole kernel structure: the skew, the
/// scan, an undecoded flag word and the name as bytes. What names a mode to a reader is its
/// extent, its rate, and whether the display asked for it.
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

/// One connector.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Connector {
    /// The object id, for naming it in a commit.
    pub id: u32,
    /// What kind of socket it is.
    pub kind: ConnectorKind,
    /// The encoders that can drive it.
    pub encoders: Vec<u32>,
    /// The modes it offers, empty when nothing is plugged in.
    pub modes: Vec<Mode>,
    /// The encoder currently attached, when there is one.
    pub encoder: Option<u32>,
    /// The kernel's connection status, as `enum drm_connector_status`.
    connection: u32,
}

impl Connector {
    /// Returns `true` when the kernel is certain a display is plugged in.
    ///
    /// The status has three values and this answers `true` for one of them, `connected`, which the
    /// kernel describes as "definitely connected to a sink device, and can be enabled". The
    /// numbers are `enum drm_connector_status` in `drm_connector.h`, which states that the uapi
    /// has no separate defines for them, so no vendored header declares the 1 this compares to.
    ///
    /// The third value is `unknown`: a status the kernel could not detect reliably. It documents
    /// that such a connector can usually still be lit, and that a default configuration should try
    /// one only where no connector answers `connected`. This answers `false` for it, so a caller
    /// that drives what this reports gets that default. Nothing here tells `unknown` from
    /// `disconnected`.
    pub fn is_connected(&self) -> bool {
        self.connection == 1
    }

    /// Returns the mode the display prefers, or the first it offers.
    ///
    /// A connector with nothing plugged in lists no mode and is answered `None`.
    ///
    /// ```no_run
    /// use zgui_drm::Device;
    ///
    /// let device = Device::open_first()?;
    ///
    /// for id in device.resources()?.connectors {
    ///     let connector = device.connector(id)?;
    ///     assert_eq!(
    ///         connector.preferred_mode().is_some(),
    ///         !connector.modes.is_empty(),
    ///         "a connector that offers any mode is answered one",
    ///     );
    /// }
    /// # Ok::<(), zgui_drm::Error>(())
    /// ```
    pub fn preferred_mode(&self) -> Option<&Mode> {
        self.modes
            .iter()
            .find(|mode| mode.is_preferred())
            .or_else(|| self.modes.first())
    }
}

impl Device {
    /// Reads the connector with this id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses, and [`Error::Unusable`] when the counts
    /// kept moving.
    pub fn connector(&self, id: u32) -> Result<Connector> {
        for _ in 0..ATTEMPTS {
            // The header's own instruction for a size query: one mode of capacity, pointed at a
            // throwaway. A zero here means something else entirely — the kernel force-probes the
            // connector, which is slow, blocks, and can make the display flicker. It is what a
            // client does once at startup and after a hot-plug event, and never per read.
            let mut probe = sys::drm_mode_modeinfo::default();
            let mut counts = sys::drm_mode_get_connector {
                connector_id: id,
                modes_ptr: std::ptr::from_mut(&mut probe) as u64,
                count_modes: 1,
                ..Default::default()
            };
            ioctl::issue(self.fd(), ioctl::MODE_GETCONNECTOR, &mut counts)?;

            let mut encoders = vec![0_u32; counts.count_encoders as usize];
            let mut modes = vec![sys::drm_mode_modeinfo::default(); counts.count_modes as usize];
            // The properties are read here and dropped: a connector's properties are asked for
            // through `property`, which reads them for any object. Passing null for these two
            // would make the kernel report the counts again instead of filling the rest.
            let mut property_ids = vec![0_u32; counts.count_props as usize];
            let mut property_values = vec![0_u64; counts.count_props as usize];

            // A connector with nothing plugged in reports no modes, and a zero count force-probes
            // on this pass exactly as it does on the one above. So a connector with none keeps the
            // throwaway and its one element of capacity. The kernel declines to fill an array whose
            // length is not the count it reports, and reports the true count either way, so a
            // monitor plugged in since the first pass still shows up as a count that moved.
            let (modes_ptr, count_modes) = if modes.is_empty() {
                (std::ptr::from_mut(&mut probe) as u64, 1)
            } else {
                (modes.as_mut_ptr() as u64, counts.count_modes)
            };

            let mut filled = sys::drm_mode_get_connector {
                connector_id: id,
                encoders_ptr: encoders.as_mut_ptr() as u64,
                modes_ptr,
                props_ptr: property_ids.as_mut_ptr() as u64,
                prop_values_ptr: property_values.as_mut_ptr() as u64,
                count_encoders: counts.count_encoders,
                count_modes,
                count_props: counts.count_props,
                ..Default::default()
            };
            ioctl::issue(self.fd(), ioctl::MODE_GETCONNECTOR, &mut filled)?;

            if filled.count_encoders != counts.count_encoders
                || filled.count_modes != counts.count_modes
                || filled.count_props != counts.count_props
            {
                continue;
            }

            return Ok(Connector {
                id,
                kind: ConnectorKind::from_raw(filled.connector_type),
                encoders,
                modes: modes.into_iter().map(|raw| Mode { raw }).collect(),
                encoder: (filled.encoder_id != 0).then_some(filled.encoder_id),
                connection: filled.connection,
            });
        }

        Err(Error::Unusable(format!(
            "connector {id} changed under every attempt to read it"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mode with these timings, and nothing else filled in.
    fn mode(clock: u32, htotal: u16, vtotal: u16) -> Mode {
        Mode {
            raw: sys::drm_mode_modeinfo {
                clock,
                htotal,
                vtotal,
                ..Default::default()
            },
        }
    }

    #[test]
    fn a_modes_rate_comes_from_its_timings_and_keeps_the_fraction() {
        // 1920x1080 at 60 Hz: 148.5 MHz over 2200 x 1125 pixels.
        assert_eq!(mode(148_500, 2200, 1125).refresh_rate_millihertz(), 60_000);
        // The same mode at 59.94, which is what the field rounded to whole hertz would call 60.
        // Pacing a frame loop against 60 on a display running this drifts a frame every
        // seventeen seconds, which is the reason this is computed rather than read.
        assert_eq!(mode(148_352, 2200, 1125).refresh_rate_millihertz(), 59_940);
    }

    #[test]
    fn a_mode_with_no_timings_reports_no_rate_rather_than_dividing_by_zero() {
        assert_eq!(mode(148_500, 0, 0).refresh_rate_millihertz(), 0);
    }
}
