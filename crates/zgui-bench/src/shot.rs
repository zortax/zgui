//! Reading a witness swatch back off the screen, through the compositor.
//!
//! A counter kept in a signal says a handler ran. It does not say the window is showing what the
//! handler did, and a window whose surface never went away shows the surface whatever its handlers
//! did. So every claim that a press or a key still reaches the page is settled here: the compositor
//! is asked for the window's own pixels, and the swatch the driver expects to have changed colour
//! is read out of them.
//!
//! The region is the window's, taken from a file the launcher writes once the window manager has
//! placed it, because where a window ends up is the window manager's decision and a region written
//! into a script captures whatever ended up there instead.
//!
//! Where in that region to look is decided by the window rather than by arithmetic on a size: the
//! view publishes each swatch's centre in device pixels and the driver knows the surface's size, so
//! a fraction of the capture is what is sampled. That survives a scale factor, a window the manager
//! resized, and a page that scrolled.

use std::io::Write;
use std::process::{Command, Stdio};

/// One swatch to read, as a fraction of the captured region.
#[derive(Clone, Copy, Debug)]
pub(crate) struct At {
    /// How far across the capture the swatch's centre is.
    pub(crate) x: f32,
    /// How far down it is.
    pub(crate) y: f32,
}

/// A colour a witness is expected to be showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Witness(pub [u8; 3]);

impl Witness {
    /// Which witness is which, for a trace line.
    pub(crate) const CLICK: &'static str = "click";
    /// The other one.
    pub(crate) const KEY: &'static str = "key";

    /// What the click witness shows when the probe has been pressed a multiple of four times, and
    /// so on for each residue.
    pub(crate) const fn click(residue: u64) -> Self {
        Self(match residue % 4 {
            0 => [0xff, 0x00, 0x00],
            1 => [0x00, 0xff, 0x00],
            2 => [0x00, 0x00, 0xff],
            _ => [0xff, 0xff, 0x00],
        })
    }

    /// What the key witness shows for each residue of the field's length.
    pub(crate) const fn key(residue: u64) -> Self {
        Self(match residue % 4 {
            0 => [0x00, 0xff, 0xff],
            1 => [0xff, 0x00, 0xff],
            2 => [0xff, 0xff, 0xff],
            _ => [0x00, 0x00, 0x00],
        })
    }

    /// Whether a sampled pixel is this colour, allowing for a compositor's own conversion.
    fn matches(self, pixel: [u8; 3]) -> bool {
        (0..3).all(|channel| {
            let want = i32::from(self.0[channel]);
            let got = i32::from(pixel[channel]);
            (want - got).abs() <= 24
        })
    }
}

/// The screenshots one run takes.
pub(crate) struct Shot {
    /// Where the file naming the window's region is.
    geometry: Option<String>,
    /// How many captures have been taken.
    taken: u64,
    /// How many could not be taken or read.
    errors: u64,
}

impl Shot {
    /// A screenshotter reading its region from `ZMV_GEOM_FILE`.
    pub(crate) fn from_environment() -> Self {
        Self {
            geometry: std::env::var("ZMV_GEOM_FILE").ok(),
            taken: 0,
            errors: 0,
        }
    }

    /// How many captures have been taken.
    pub(crate) const fn taken(&self) -> u64 {
        self.taken
    }

    /// How many could not be taken or read, which is what makes a run's failures trustworthy.
    pub(crate) const fn errors(&self) -> u64 {
        self.errors
    }

    /// Whether the window's region is known yet.
    ///
    /// A window manager places a window some time after the process opens it, so the region is
    /// written from outside once that has happened. Cycles do not start before it is: a check that
    /// cannot photograph anything is a check that reports every window deaf.
    pub(crate) fn ready(&self) -> bool {
        self.region().is_some()
    }

    /// The window's region, when the launcher has written it.
    fn region(&self) -> Option<(i32, i32, i32, i32)> {
        let region = std::fs::read_to_string(self.geometry.as_ref()?).ok()?;
        let mut parts = region.split_whitespace();
        Some((
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ))
    }

    /// Whether the swatch at `at` is showing `want`, according to the screen.
    ///
    /// Answers `false` when the capture could not be made at all, and counts that separately: a run
    /// whose error count is zero is a run in which every `false` was a pixel that was really the
    /// wrong colour.
    pub(crate) fn reads(&mut self, which: &str, at: At, want: Witness) -> bool {
        let Some(image) = self.capture() else {
            self.errors += 1;
            return false;
        };
        let Some(pixel) = image.sample(at) else {
            self.errors += 1;
            eprintln!("ZMT shot.outside which={which} x={:.3} y={:.3}", at.x, at.y);
            return false;
        };
        if want.matches(pixel) {
            return true;
        }
        eprintln!(
            "ZMT shot.mismatch which={which} want={:02x}{:02x}{:02x} got={:02x}{:02x}{:02x}",
            want.0[0], want.0[1], want.0[2], pixel[0], pixel[1], pixel[2]
        );
        false
    }

    /// Asks the compositor for the window's pixels.
    fn capture(&mut self) -> Option<Image> {
        let (x, y, width, height) = self.region()?;
        eprintln!("ZMT 0 shot.begin");
        let child = Command::new("grim")
            .arg("-g")
            .arg(format!("{x},{y} {width}x{height}"))
            .arg("-t")
            .arg("ppm")
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let output = child.wait_with_output().ok()?;
        eprintln!("ZMT 0 shot.end bytes={}", output.stdout.len());
        self.taken += 1;
        if !output.status.success() {
            return None;
        }
        std::io::stderr().flush().ok();
        Image::parse(&output.stdout)
    }
}

/// A captured region.
struct Image {
    /// How wide it is.
    width: usize,
    /// How tall it is.
    height: usize,
    /// Three bytes per pixel, row by row.
    pixels: Vec<u8>,
}

impl Image {
    /// Reads a binary portable pixmap.
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut cursor = 0;
        let mut fields = Vec::new();
        while fields.len() < 4 && cursor < bytes.len() {
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'#') {
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
                continue;
            }
            let start = cursor;
            while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            fields.push(core::str::from_utf8(&bytes[start..cursor]).ok()?.to_owned());
        }
        if fields.first().map(String::as_str) != Some("P6") {
            return None;
        }
        let width: usize = fields.get(1)?.parse().ok()?;
        let height: usize = fields.get(2)?.parse().ok()?;
        if fields.get(3).map(String::as_str) != Some("255") {
            return None;
        }
        cursor += 1;
        let pixels = bytes.get(cursor..)?.to_vec();
        (pixels.len() >= width * height * 3).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// The colour at `at`, as the average of a small block so that one stray pixel decides nothing.
    fn sample(&self, at: At) -> Option<[u8; 3]> {
        if !(0.0..1.0).contains(&at.x) || !(0.0..1.0).contains(&at.y) {
            return None;
        }
        let centre_x = (at.x * self.width as f32) as usize;
        let centre_y = (at.y * self.height as f32) as usize;
        let mut totals = [0_u32; 3];
        let mut counted = 0_u32;
        for row in centre_y.saturating_sub(3)..(centre_y + 4).min(self.height) {
            for column in centre_x.saturating_sub(3)..(centre_x + 4).min(self.width) {
                let base = (row * self.width + column) * 3;
                let pixel = self.pixels.get(base..base + 3)?;
                for (total, channel) in totals.iter_mut().zip(pixel) {
                    *total += u32::from(*channel);
                }
                counted += 1;
            }
        }
        (counted > 0).then(|| {
            [
                (totals[0] / counted) as u8,
                (totals[1] / counted) as u8,
                (totals[2] / counted) as u8,
            ]
        })
    }
}
