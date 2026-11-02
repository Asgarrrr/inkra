//! The process's only `log::Log`: warnings and errors, one line each, to stderr (quit spec, Q6).

use std::io::{self, Write as _};

use log::{Level, LevelFilter, Metadata, Record};

struct Stderr;

static LOGGER: Stderr = Stderr;

impl log::Log for Stderr {
    /// gpui's own warnings (a duplicate font, every launch) are noise to the user; its errors are not.
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let floor = if metadata.target().starts_with("inkra") {
            Level::Warn
        } else {
            Level::Error
        };
        metadata.level() <= floor
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            // `eprintln!` panics on a closed stderr; a logger must never panic.
            let _ = writeln!(io::stderr().lock(), "{}", line(record));
        }
    }

    fn flush(&self) {}
}

/// Installed first in `run`; a logger already set keeps its place.
pub(crate) fn init() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Warn);
    }
}

fn line(record: &Record<'_>) -> String {
    format!("{} {}: {}", record.level(), record.target(), record.args())
}

#[cfg(test)]
mod tests;
