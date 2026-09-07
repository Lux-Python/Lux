use std::time::Duration;
use super::theme::{format_duration, Style};

/// Professional, minimal status logger inspired by Cargo and modern systems tooling.
pub struct Status;

impl Status {
    /// Print an active action in progress (e.g. `Resolving dependencies...`).
    pub fn action(verb: &str, message: &str) {
        println!("{:>12} {message}", Style::bold_cyan(verb));
    }

    /// Print a completed milestone (e.g. `Installed 2 packages in 13ms`).
    pub fn completed(verb: &str, message: &str, duration: Option<Duration>) {
        let timing = duration.map_or_else(String::new, |d| {
            format!(" {}", Style::dim(&format!("in {}", format_duration(d))))
        });
        println!("{:>12} {message}{timing}", Style::bold_green(verb));
    }

    /// Print an informative detail line.
    pub fn info(verb: &str, message: &str) {
        println!("{:>12} {message}", Style::dim(verb));
    }

    /// Print a warning.
    pub fn warn(message: &str) {
        println!("{:>12} {message}", Style::bold_yellow("warning:"));
    }

    /// Print an error.
    pub fn error(message: &str) {
        println!("{:>12} {message}", Style::bold_red("error:"));
    }
}

/// Package change delta representation for clean addition/removal listings.
#[derive(Debug, Default, Clone)]
pub struct PackageDiff {
    pub added: Vec<(String, String)>,
    pub removed: Vec<(String, String)>,
}

impl PackageDiff {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.added.push((name.into(), version.into()));
    }

    pub fn remove(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.removed.push((name.into(), version.into()));
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    pub fn render(&self) {
        for (name, version) in &self.added {
            println!("   {} {} {}", Style::green("+"), Style::bold(name), Style::dim(&format!("v{version}")));
        }
        for (name, version) in &self.removed {
            println!("   {} {} {}", Style::red("-"), Style::bold(name), Style::dim(&format!("v{version}")));
        }
    }
}
