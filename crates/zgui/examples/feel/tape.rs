//! The recording every other module writes into.
//!
//! One line per moment, in the order they happened, with a monotonic timestamp taken as early as
//! the caller can take it. Durations are never recorded — only instants — so that the gap between
//! two stages is visible whether or not anyone thought to name what filled it.
//!
//! The zero is carried as a wall-clock time as well as a monotonic one, so that a driver in
//! another process — the thing that injects the input being measured — can put its own timestamps
//! on the same axis.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// A shared handle on the recording.
pub(crate) type Shared = Rc<RefCell<Tape>>;

/// One recorded moment.
struct Moment {
    /// Nanoseconds since the recording's zero.
    at_ns: u128,
    /// What happened.
    stage: &'static str,
    /// Which thing it happened to.
    detail: String,
}

/// The recording.
pub(crate) struct Tape {
    /// The zero every moment is relative to.
    base: Instant,
    /// What that zero was on the wall clock.
    base_unix_ns: u128,
    /// Where the file goes.
    path: PathBuf,
    /// How many moments the file already holds.
    written: usize,
    /// What has happened.
    moments: Vec<Moment>,
}

impl Tape {
    /// A recording whose zero is now, to be written to `path`.
    pub(crate) fn new(path: PathBuf) -> Shared {
        Rc::new(RefCell::new(Self {
            base: Instant::now(),
            base_unix_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos()),
            path,
            written: 0,
            moments: Vec::with_capacity(1 << 16),
        }))
    }

    /// Records that `stage` happened at `at`, described by `detail`.
    pub(crate) fn at(&mut self, at: Instant, stage: &'static str, detail: impl Into<String>) {
        let at_ns = at.saturating_duration_since(self.base).as_nanos();
        self.moments.push(Moment {
            at_ns,
            stage,
            detail: detail.into(),
        });
    }

    /// Records that `stage` happened now, described by `detail`.
    pub(crate) fn now(&mut self, stage: &'static str, detail: impl Into<String>) {
        self.at(Instant::now(), stage, detail);
    }

    /// Writes the recording if anything has been added since it was last written.
    ///
    /// Called every time the loop is about to park, which is the last thing that happens in a turn
    /// and therefore the last moment before a run that is killed from outside stops existing. A
    /// loop that is parked properly takes no turns at all, so this is also the only place the file
    /// can be brought up to date without a thread of its own.
    pub(crate) fn flush_if_due(&mut self) {
        if self.moments.len() != self.written {
            self.write();
        }
    }

    /// Writes the recording as one JSON object per line, replacing whatever was there.
    pub(crate) fn write(&mut self) {
        let Ok(file) = File::create(&self.path) else {
            return;
        };
        self.written = self.moments.len();
        let mut out = BufWriter::new(file);
        for moment in &self.moments {
            let _ = writeln!(
                out,
                r#"{{"t_ns":{},"base_unix_ns":{},"stage":"{}","detail":"{}"}}"#,
                moment.at_ns,
                self.base_unix_ns,
                moment.stage,
                moment.detail.replace('"', "'")
            );
        }
        let _ = out.flush();
    }
}
