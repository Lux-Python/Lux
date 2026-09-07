//! Virtual environment creation command (`lux venv`).

use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;
use miette::Result;

use lux_core::env::{PythonVersionFile, VirtualEnv};

use crate::ui::theme::Style;
use crate::ui::Status;

/// Execute the `lux venv` command to create a virtual environment.
pub fn run_venv(path: Option<PathBuf>, python: Option<String>) -> Result<()> {
    let start_time = Instant::now();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Target directory defaults to `.venv` in current working directory
    let venv_dir = path.unwrap_or_else(|| cwd.join(".venv"));

    // Determine Python version preference: CLI flag > .python-version > host python fallback
    let py_ver = python.or_else(|| PythonVersionFile::read_from(&cwd));
    let py_ver_ref = py_ver.as_deref().unwrap_or("3.12.0");

    let venv = VirtualEnv::create(&venv_dir, Some(py_ver_ref))
        .map_err(|e| miette::miette!("Failed to create virtual environment: {e}"))?;

    let host_info = VirtualEnv::find_host_python().map_or_else(
        || format!("Python {py_ver_ref}"),
        |p| format!("host Python at {}", Style::dim(&p.display().to_string())),
    );
    Status::info("Using", &host_info);

    Status::completed(
        "Initialized",
        &format!("virtual environment at {}", Style::bold(&venv.root().display().to_string())),
        Some(start_time.elapsed()),
    );

    let activate_hint = activation_hint(venv.root());
    Status::info("Activate with", &activate_hint);

    Ok(())
}

/// Helper to render the platform-appropriate shell activation command.
fn activation_hint(root: &Path) -> String {
    if cfg!(windows) {
        let scripts = root.join("Scripts");
        format!(
            "{}\\activate or {}\\Activate.ps1",
            scripts.display(),
            scripts.display()
        )
    } else {
        format!("source {}/bin/activate", root.display())
    }
}
