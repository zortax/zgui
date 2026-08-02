//! Face metrics: what the cascade has to ask a font system before it can finish.

pub mod face;
pub mod fixed;
pub mod query;
pub mod source;

pub use crate::metrics::face::{FaceMetrics, X_HEIGHT_FALLBACK, ZERO_ADVANCE_FALLBACK};
pub use crate::metrics::fixed::FixedMetrics;
pub use crate::metrics::query::FaceQuery;
pub use crate::metrics::source::FontMetricsSource;
