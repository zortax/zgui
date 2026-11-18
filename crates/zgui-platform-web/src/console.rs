//! Warnings and errors, in the page's console.
//!
//! A page has no standard error stream. A warning from the framework, or from the graphics
//! library under it, is printed nowhere unless something forwards it, and a renderer that draws
//! nothing with no message is the result. This forwards every log record at warning level or
//! above. An application that installs its own logger first keeps it.

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The logger that writes to the console.
struct Console;

impl Log for Console {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{}: {}", record.target(), record.args()).into();
        if record.level() == Level::Error {
            web_sys::console::error_1(&line);
        } else {
            web_sys::console::warn_1(&line);
        }
    }

    fn flush(&self) {}
}

/// Installs the console logger, unless the application installed a logger of its own.
pub(crate) fn install() {
    if log::set_logger(&Console).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
}
