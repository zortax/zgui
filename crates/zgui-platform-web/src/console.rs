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

/// Installs the console logger, unless the application installed a logger of its own, and
/// prints a panic to the console.
///
/// A panic in a page is a trap with no message: the default hook writes to a standard error
/// stream the page does not have. The message is printed first, and the hook that was installed
/// before still runs.
pub(crate) fn install() {
    if log::set_logger(&Console).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
    static HOOKED: std::sync::Once = std::sync::Once::new();
    HOOKED.call_once(|| {
        let earlier = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            web_sys::console::error_1(&format!("zgui: {panic}").into());
            earlier(panic);
        }));
    });
}
