//! Location fixes from Android's location providers.

use super::{Guard, jvm};
use std::time::Duration;

/// Providers to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// Satellite fixes only. Needs [`FINE_LOCATION`](super::permission::FINE_LOCATION).
    Gps,
    /// Network fixes only, quick and approximate.
    Network,
    /// Network and, with precise permission, satellite fixes.
    #[default]
    Any,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Self::Gps => "gps",
            Self::Network => "network",
            Self::Any => "any",
        }
    }
}

/// What to watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// Providers.
    pub source: Source,
    /// Shortest time between fixes.
    pub interval: Duration,
    /// Start with each provider's last known fix. Check [`Location::age`].
    pub last_known: bool,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            source: Source::Any,
            interval: Duration::from_secs(1),
            last_known: true,
        }
    }
}

/// One fix.
#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// Horizontal accuracy radius in meters.
    pub accuracy_m: Option<f32>,
    /// Altitude above the WGS 84 ellipsoid in meters.
    pub altitude_m: Option<f64>,
    /// Speed in meters per second.
    pub speed: Option<f32>,
    /// Bearing in degrees.
    pub bearing: Option<f32>,
    /// UTC time of the fix, in milliseconds since 1970.
    pub time_ms: u64,
    /// Time since the device measured the fix.
    pub age: Duration,
    /// Provider name, such as `gps`.
    pub provider: String,
}

/// One report of a watch.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    /// A new fix.
    Fix(Location),
    /// The user turned off the named provider.
    Disabled(String),
    /// The watch ended, for example because the user revoked permission.
    Failed(String),
}

fn parse(reply: jvm::Reply) -> Option<Update> {
    if reply.code != jvm::OK {
        return Some(Update::Failed(reply.get(0).to_owned()));
    }
    if reply.get(0) == "disabled" {
        return Some(Update::Disabled(reply.get(1).to_owned()));
    }
    let number = |index: usize| reply.get(index).parse::<f64>().ok();
    Some(Update::Fix(Location {
        lat: number(0)?,
        lon: number(1)?,
        accuracy_m: number(2).map(|v| v as f32),
        altitude_m: number(3),
        speed: number(4).map(|v| v as f32),
        bearing: number(5).map(|v| v as f32),
        time_ms: reply.get(6).parse().ok()?,
        age: Duration::from_millis(reply.get(7).parse::<i64>().ok()?.max(0) as u64),
        provider: reply.get(8).to_owned(),
    }))
}

/// Sends fixes to `callback` until the guard drops.
///
/// In a process with a visible activity, updates pause while the activity is hidden, as
/// Android allows background location only for foreground services. In the process of a
/// running `ZguiService` they continue.
///
/// # Errors
///
/// Location permission is missing, or the JNI call failed.
pub fn watch(
    request: Request,
    mut callback: impl FnMut(Update) + Send + 'static,
) -> Result<Guard, String> {
    jvm::many(
        "location",
        &[
            request.source.name(),
            &request.interval.as_millis().to_string(),
            if request.last_known { "1" } else { "0" },
        ],
        move |reply| {
            if let Some(update) = parse(reply) {
                callback(update);
            }
        },
    )
}

/// Whether the user turned on a provider of `source`.
pub fn enabled(source: Source) -> bool {
    jvm::call_one("location.enabled", &[source.name()]).is_ok_and(|value| value == "1")
}
