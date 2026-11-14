//! Camera capture and audio recording and playback.

use super::{Guard, jvm};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Opens the camera application to take one photo. `callback` receives the JPEG in
/// [`document::shared_dir`](super::document::shared_dir), or `None` when the user cancels.
/// Needs `ZguiFiles` in the manifest. Do not declare the camera permission: Android then
/// refuses this request until the user grants it.
///
/// # Errors
///
/// No activity is visible in this process, or no camera application exists.
pub fn capture_photo(
    callback: impl FnOnce(Result<Option<PathBuf>, String>) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.camera", &[], move |reply| {
        callback(
            reply
                .result()
                .map(|text| text.and_then(|t| t.into_iter().next()).map(PathBuf::from)),
        );
    })
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| "Path must be UTF-8".into())
}

/// Records the microphone to `path` as AAC in MPEG-4, for at most `limit`. `done` runs when the
/// limit ends the recording. Dropping the guard ends it earlier; `done` then does not run.
/// Needs [`RECORD_AUDIO`](super::permission::RECORD_AUDIO).
///
/// # Errors
///
/// Permission is missing, the microphone is busy, or the JNI call failed.
pub fn record_audio(
    path: &Path,
    limit: Duration,
    done: impl FnOnce(Result<(), String>) + Send + 'static,
) -> Result<Guard, String> {
    let mut done = Some(done);
    jvm::many(
        "audio.record",
        &[path_str(path)?, &limit.as_millis().to_string()],
        move |reply| {
            if let Some(done) = done.take() {
                done(reply.result().map(drop));
            }
        },
    )
}

/// How playback ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Playback {
    /// The file played to its end.
    Finished,
    /// Another application took audio focus, or the user paused from the media controls.
    Interrupted,
    /// The file could not play.
    Failed(String),
}

/// Plays the audio file at `path` with audio focus and a media session, so system media
/// controls can pause it. `done` runs when playback ends; dropping the guard stops it, and
/// `done` then does not run.
///
/// # Errors
///
/// The JNI call failed.
pub fn play_audio(
    path: &Path,
    done: impl FnOnce(Playback) + Send + 'static,
) -> Result<Guard, String> {
    let mut done = Some(done);
    jvm::many("audio.play", &[path_str(path)?], move |reply| {
        if let Some(done) = done.take() {
            done(match reply.result() {
                Ok(Some(_)) => Playback::Finished,
                Ok(None) => Playback::Interrupted,
                Err(error) => Playback::Failed(error),
            });
        }
    })
}
