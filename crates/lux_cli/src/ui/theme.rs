use std::env;
use std::time::Duration;
use std::io::IsTerminal;

/// Determine whether color formatting should be disabled via `NO_COLOR` or terminal state.
#[must_use]
pub fn colors_enabled() -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Ok(term) = env::var("TERM") {
        if term == "dumb" {
            return false;
        }
    }
    if !std::io::stderr().is_terminal() {
        return false;
    }
    true
}

/// Minimal ANSI styling utilities with automatic suppression when colors are disabled.
pub struct Style;

impl Style {
    #[must_use]
    pub fn bold(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn dim(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[2m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn cyan(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[36m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn bold_cyan(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[1;36m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn green(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[32m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn bold_green(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[1;32m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn yellow(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[33m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn bold_yellow(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[1;33m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn red(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    #[must_use]
    pub fn bold_red(s: &str) -> String {
        if colors_enabled() {
            format!("\x1b[1;31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

/// Standard box-drawing characters for dependency trees and diffs (no emojis).
pub struct Glyphs;

impl Glyphs {
    pub const ADD: &'static str = "+";
    pub const REMOVE: &'static str = "-";
    pub const BRANCH: &'static str = "├── ";
    pub const LAST_BRANCH: &'static str = "└── ";
    pub const VERTICAL: &'static str = "│   ";
    pub const EMPTY: &'static str = "    ";
}

/// High-precision duration formatter (µs, ms, s).
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{nanos}ns")
    } else if nanos < 1_000_000 {
        let micros = (nanos as f64) / 1_000.0;
        format!("{micros:.1}µs")
    } else if nanos < 1_000_000_000 {
        let millis = (nanos as f64) / 1_000_000.0;
        format!("{millis:.2}ms")
    } else {
        let secs = d.as_secs_f64();
        format!("{secs:.2}s")
    }
}

/// Byte size formatter (B, KiB, MiB, GiB).
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;

    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", (bytes as f64) / (KIB as f64))
    } else if bytes < GIB {
        format!("{:.2} MiB", (bytes as f64) / (MIB as f64))
    } else {
        format!("{:.2} GiB", (bytes as f64) / (GIB as f64))
    }
}
