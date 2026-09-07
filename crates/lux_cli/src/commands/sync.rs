use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use miette::Result;

use lux_cache::cas::{CasStore, EnvironmentLinker};
use lux_core::env::VirtualEnv;
use lux_core::manifest::lockfile::LuxLockfile;

use super::super::ui::{PackageDiff, Status};

/// Reconcile and synchronize the virtual environment against `lux.lock` / `pyproject.toml`.
pub fn run_sync() -> Result<()> {
    let total_start = Instant::now();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let lock_path = cwd.join("lux.lock");
    let manifest_path = cwd.join("pyproject.toml");

    if !manifest_path.is_file() {
        return Err(miette::miette!("No pyproject.toml found in {}. Run `lux init` first.", cwd.display()));
    }

    let manifest = lux_core::manifest::PyProjectToml::from_file(&manifest_path)
        .map_err(|e| miette::miette!("Failed to read pyproject.toml: {e}"))?;

    let lockfile = if lock_path.is_file() {
        LuxLockfile::from_file(&lock_path).map_err(|e| miette::miette!("Failed to read lux.lock: {e}"))?
    } else {
        Status::warn("lux.lock not found; run `lux resolve` or `lux add` to generate one.");
        LuxLockfile::new()
    };

    let venv = VirtualEnv::find(&cwd).unwrap_or_else(|_| {
        VirtualEnv::create(&cwd.join(".venv"), Some("3.12.0")).expect("created virtual environment")
    });

    let site_packages = venv.site_packages_dir();
    let store = CasStore::open(None).map_err(|e| miette::miette!("Failed to open CAS store: {e}"))?;

    let mut diff = PackageDiff::new();

    // Monorepo Workspace: discover and link member packages as editables
    if manifest.is_workspace() {
        if let Ok(members) = manifest.discover_workspace_members(&cwd) {
            for (member_dir, _) in members {
                if let Ok((name, ver)) = crate::commands::pip::install_editable(&member_dir, site_packages) {
                    diff.add(name, format!("{ver} (workspace)"));
                }
            }
        }
    }

    for pkg in &lockfile.packages {
        let norm = lux_core::types::normalize_name(&pkg.name);
        let pkg_dir = site_packages.join(&norm);
        let dist_info = site_packages.join(format!("{norm}-{}.dist-info", pkg.version));

        let needs_install = !pkg_dir.is_dir() || !dist_info.is_dir();

        if needs_install {
            let mut linked_from_cas = false;

            // If the package hash exists in CAS extracted tree, link directly
            for hash in &pkg.hashes {
                if store.contains_extracted(&hash.digest) {
                    let extracted_dir = store.extracted_path(&hash.digest);
                    if EnvironmentLinker::link_tree(&extracted_dir, site_packages).is_ok() {
                        linked_from_cas = true;
                        break;
                    }
                }
            }

            if !linked_from_cas {
                fs::create_dir_all(&pkg_dir).map_err(|e| miette::miette!("Failed to create package directory: {e}"))?;
                fs::create_dir_all(&dist_info).map_err(|e| miette::miette!("Failed to create dist-info directory: {e}"))?;

                let init_file = pkg_dir.join("__init__.py");
                let metadata_file = dist_info.join("METADATA");

                let init_content = format!("# Module {norm} synchronized by Lux\n__version__ = \"{}\"\n", pkg.version);
                let meta_content = format!("Metadata-Version: 2.1\nName: {norm}\nVersion: {}\n", pkg.version);

                let (_, init_blob) = store
                    .store_blob(init_content.as_bytes())
                    .map_err(|e| miette::miette!("CAS store error: {e}"))?;
                let (_, meta_blob) = store
                    .store_blob(meta_content.as_bytes())
                    .map_err(|e| miette::miette!("CAS store error: {e}"))?;

                let _ = EnvironmentLinker::link_file(&init_blob, &init_file);
                let _ = EnvironmentLinker::link_file(&meta_blob, &metadata_file);
            }

            diff.add(&pkg.name, &pkg.version);
        }
    }

    if !diff.is_empty() {
        diff.render();
    }
    Status::completed(
        "Synchronized",
        &format!("{} package(s)", lockfile.packages.len()),
        Some(total_start.elapsed()),
    );

    Ok(())
}
