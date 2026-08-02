//! Writing a readback out as a picture, so that a claim and an image are the same pixels.
//!
//! A screenshot taken from a second run proves that *a* window looked like that. What is written
//! here is the very buffer the assertions beside it read, cropped to the rectangle the document
//! reported — so a picture and the measurement under it cannot disagree.
//!
//! The format is a binary [PPM]. It is eleven lines of code and no dependency, and every image tool
//! reads it; the pictures that ship are converted from these once.
//!
//! [PPM]: https://netpbm.sourceforge.net/doc/ppm.html

use std::io;
use std::path::{Path, PathBuf};

use zgui::geom::{Device, DevicePx, Rect};
use zgui_render_wgpu::Pixels;

/// The environment variable naming the directory captures go to.
///
/// Unset, nothing is written: a test run is not a photo session, and a fixture that wrote files
/// every time would put a megabyte into the build directory on every `cargo test`.
pub const DIRECTORY: &str = "ZGUI_SHOT_DIR";

/// Where captures go this run, or nothing when none were asked for.
pub fn directory() -> Option<PathBuf> {
    std::env::var_os(DIRECTORY).map(PathBuf::from)
}

/// Writes `rect` of `pixels` into `name.ppm` under the capture directory, if there is one.
///
/// Silently does nothing when no directory was asked for. Reports what stopped it when there was
/// one and the write failed, because a missing picture and a picture of the wrong thing are told
/// apart only by saying so.
pub fn crop(pixels: &Pixels, rect: Rect<DevicePx, Device>, name: &str) -> io::Result<()> {
    let Some(directory) = directory() else {
        return Ok(());
    };
    std::fs::create_dir_all(&directory)?;
    write(
        pixels,
        bounds(pixels, rect),
        &directory.join(format!("{name}.ppm")),
    )
}

/// Writes the whole readback, if a directory was asked for.
///
/// # Errors
///
/// Returns whatever stopped the write.
pub fn whole(pixels: &Pixels, name: &str) -> io::Result<()> {
    let size = pixels.size();
    let Some(directory) = directory() else {
        return Ok(());
    };
    std::fs::create_dir_all(&directory)?;
    write(
        pixels,
        (0, 0, size.width, size.height),
        &directory.join(format!("{name}.ppm")),
    )
}

/// `rect` in whole pixels, cut down to what was read back, never empty.
fn bounds(pixels: &Pixels, rect: Rect<DevicePx, Device>) -> (i32, i32, i32, i32) {
    let size = pixels.size();
    let left = (rect.origin.x.0.floor() as i32).clamp(0, size.width.saturating_sub(1));
    let top = (rect.origin.y.0.floor() as i32).clamp(0, size.height.saturating_sub(1));
    let right = ((rect.origin.x.0 + rect.size.width.0).ceil() as i32).clamp(left + 1, size.width);
    let bottom = ((rect.origin.y.0 + rect.size.height.0).ceil() as i32).clamp(top + 1, size.height);
    (left, top, right, bottom)
}

/// Writes one rectangle out.
fn write(pixels: &Pixels, bounds: (i32, i32, i32, i32), path: &Path) -> io::Result<()> {
    let (left, top, right, bottom) = bounds;
    let width = right - left;
    let height = bottom - top;
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    out.reserve((width * height * 3) as usize);
    for y in top..bottom {
        for x in left..right {
            let [r, g, b, _] = pixels.rgba(x, y);
            out.extend_from_slice(&[r, g, b]);
        }
    }
    std::fs::write(path, out)
}
