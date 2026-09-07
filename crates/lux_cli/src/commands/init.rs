use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use miette::Result;

use lux_core::env::VirtualEnv;
use lux_core::manifest::PyProjectToml;

use super::super::ui::Status;

/// Execute the `lux init` command.
pub fn run_init(path: Option<PathBuf>, name: Option<String>) -> Result<()> {
    let start_time = Instant::now();

    let target_dir = path.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let project_name = name.unwrap_or_else(|| {
        target_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed-project")
            .to_string()
    });

    fs::create_dir_all(&target_dir).map_err(|e| miette::miette!("Failed to create directory '{}': {e}", target_dir.display()))?;

    let manifest_path = target_dir.join("pyproject.toml");
    if manifest_path.is_file() {
        Status::info("Existing", &format!("`pyproject.toml` at {}", manifest_path.display()));
    } else {
        let manifest = PyProjectToml::init_default(&project_name);
        manifest
            .save_to_file(&manifest_path)
            .map_err(|e| miette::miette!("Failed to write pyproject.toml: {e}"))?;
        Status::completed("Created", &format!("`pyproject.toml` ({project_name})"), None);
    }

    let venv_dir = target_dir.join(".venv");
    let pinned_ver = lux_core::env::PythonVersionFile::read_from(&target_dir);
    let py_ver = pinned_ver.as_deref().unwrap_or("3.12.0");
    let venv = VirtualEnv::create(&venv_dir, Some(py_ver))
        .map_err(|e| miette::miette!("Failed to create virtual environment: {e}"))?;

    Status::completed(
        "Initialized",
        &format!("virtual environment at {}", venv.root().display()),
        Some(start_time.elapsed()),
    );

    Ok(())
}
