//! Face metrics with no font files behind them.

use zgui_geom::CssPx;
use zgui_text_style::GenericFamily;

use crate::metrics::face::FaceMetrics;
use crate::metrics::query::FaceQuery;
use crate::metrics::source::FontMetricsSource;

/// A [`FontMetricsSource`] whose answers are fixed fractions of the size asked for.
///
/// Two things need this. A style engine cannot be built or tested at all without *some* answer to
/// "how tall is an `ex`", and a test that wants to assert what a cascade produced needs that answer
/// to be the same on every machine — which no real font system can promise, because it depends on
/// which faces are installed.
///
/// So the answers here are ratios of the font size, chosen to sit within the range real text faces
/// occupy, and every field is present: a document styled against this source exercises the
/// *present* branch of every font-relative unit rather than the fallback branch.
///
/// It holds no state, so it is trivially shared between threads and returns the same value however
/// many times it is asked, from however many threads.
///
/// ```
/// use zgui_geom::CssPx;
/// use zgui_text::{FaceQuery, FixedMetrics, FontMetricsSource};
/// use zgui_text_style::TextStyle;
///
/// let source = FixedMetrics::new();
/// let style = TextStyle::initial();
/// let metrics = source.face_metrics(&FaceQuery::of(&style), CssPx(20.0), false);
///
/// assert_eq!(metrics.x_height, Some(CssPx(10.0)));
/// assert_eq!(metrics.ascent, CssPx(16.0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FixedMetrics;

/// The ratios [`FixedMetrics`] reports, as fractions of the font size.
pub mod ratio {
    /// Height of a lowercase `x`.
    pub const X_HEIGHT: f32 = 0.5;
    /// Advance width of the digit zero.
    pub const ZERO_ADVANCE: f32 = 0.5;
    /// Height of a capital letter.
    pub const CAP_HEIGHT: f32 = 0.7;
    /// Advance width of an ideograph, which is square.
    pub const IC_WIDTH: f32 = 1.0;
    /// Distance from the baseline to the top of the content area.
    pub const ASCENT: f32 = 0.8;
    /// Distance from the baseline to the bottom of the content area.
    pub const DESCENT: f32 = 0.2;
    /// Extra leading between one line and the next.
    pub const LINE_GAP: f32 = 0.0;
    /// How far the underline sits below the baseline, measured upwards.
    pub const UNDERLINE_OFFSET: f32 = -0.1;
    /// Thickness of the underline and of the strikeout.
    pub const DECORATION_THICKNESS: f32 = 0.07;
    /// How far the strikeout sits above the baseline.
    pub const STRIKEOUT_OFFSET: f32 = 0.25;
}

/// How far a first-level script is scaled down.
pub const SCRIPT_PERCENT: f32 = 0.71;

/// How far a second-level script is scaled down, which is the first level applied twice.
pub const SCRIPT_SCRIPT_PERCENT: f32 = SCRIPT_PERCENT * SCRIPT_PERCENT;

/// The default size for a proportional family.
pub const BASE_SIZE: CssPx = CssPx(16.0);

/// The default size for a monospace family, which environments configure smaller because a
/// monospace face at the same nominal size reads larger.
pub const MONOSPACE_BASE_SIZE: CssPx = CssPx(13.0);

impl FixedMetrics {
    /// The source.
    pub const fn new() -> Self {
        Self
    }

    /// The metrics reported at one size, whatever face was asked for.
    pub fn at(size: CssPx) -> FaceMetrics {
        FaceMetrics {
            x_height: Some(CssPx(size.0 * ratio::X_HEIGHT)),
            zero_advance: Some(CssPx(size.0 * ratio::ZERO_ADVANCE)),
            cap_height: Some(CssPx(size.0 * ratio::CAP_HEIGHT)),
            ic_width: Some(CssPx(size.0 * ratio::IC_WIDTH)),
            ascent: CssPx(size.0 * ratio::ASCENT),
            descent: CssPx(size.0 * ratio::DESCENT),
            line_gap: CssPx(size.0 * ratio::LINE_GAP),
            underline_offset: Some(CssPx(size.0 * ratio::UNDERLINE_OFFSET)),
            underline_thickness: Some(CssPx(size.0 * ratio::DECORATION_THICKNESS)),
            strikeout_offset: Some(CssPx(size.0 * ratio::STRIKEOUT_OFFSET)),
            strikeout_thickness: Some(CssPx(size.0 * ratio::DECORATION_THICKNESS)),
            script_percent: Some(SCRIPT_PERCENT),
            script_script_percent: Some(SCRIPT_SCRIPT_PERCENT),
        }
    }

    /// The descent reported at one size.
    ///
    /// Reads the same field [`at`](Self::at) fills, so the two can never disagree.
    pub fn descent_at(size: CssPx) -> CssPx {
        Self::at(size).descent
    }
}

impl FontMetricsSource for FixedMetrics {
    fn face_metrics(&self, _query: &FaceQuery<'_>, size: CssPx, _vertical: bool) -> FaceMetrics {
        Self::at(size)
    }

    fn base_size(&self, generic: GenericFamily) -> CssPx {
        match generic {
            GenericFamily::Monospace => MONOSPACE_BASE_SIZE,
            _ => BASE_SIZE,
        }
    }
}
