use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use miette::Result;

use lux_core::env::VirtualEnv;
use lux_core::manifest::lockfile::LuxLockfile;
use lux_core::manifest::PyProjectToml;

use super::super::ui::{PackageDiff, Status};

/// Execute the `lux remove` command to prune dependencies from manifest, lockfile, and virtual environment.
pub fn run_remove(packages: &[String]) -> Result<()> {
    if packages.is_empty() {
        return Err(miette::miette!("No packages specified. Usage: `lux remove <package...>`"));
    }

    let total_start = Instant::now();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Update pyproject.toml
    let manifest_path = cwd.join("pyproject.toml");
    if !manifest_path.is_file() {
        return Err(miette::miette!("No pyproject.toml found in {}.", cwd.display()));
    }

    let mut manifest = PyProjectToml::from_file(&manifest_path)
        .map_err(|e| miette::miette!("Failed to read pyproject.toml: {e}"))?;

    for pkg in packages {
        manifest.remove_dependency(pkg);
    }

    manifest
        .save_to_file(&manifest_path)
        .map_err(|e| miette::miette!("Failed to save pyproject.toml: {e}"))?;

    // Update lux.lock
    let lock_path = cwd.join("lux.lock");
    let mut diff = PackageDiff::new();

    if lock_path.is_file() {
        if let Ok(mut lockfile) = LuxLockfile::from_file(&lock_path) {
            for pkg in packages {
                if let Some(existing) = lockfile.find_package(pkg) {
                    diff.remove(&existing.name, &existing.version);
                }
                lockfile.remove_package(pkg);
            }
            let _ = lockfile.save_to_file(&lock_path);
        }
    }

    // Remove from .venv site-packages
    if let Ok(venv) = VirtualEnv::find(&cwd) {
        let site_packages = venv.site_packages_dir();
        for pkg in packages {
            let norm = lux_core::types::normalize_name(pkg);
            let pkg_dir = site_packages.join(&norm);
            if pkg_dir.is_dir() {
                let _ = fs::remove_dir_all(&pkg_dir);
            }

            if let Ok(entries) = fs::read_dir(site_packages) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let prefix = format!("{norm}-");
                    if file_name.starts_with(&prefix) && file_name.ends_with(".dist-info") {
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }
    }

    diff.render();
    Status::completed("Removed", &format!("{} package(s)", diff.removed.len()), Some(total_start.elapsed()));

    Ok(())
}
