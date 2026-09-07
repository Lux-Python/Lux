use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;
use miette::Result;

use lux_cache::cas::CasStore;
use lux_core::env::VirtualEnv;
use lux_core::manifest::lockfile::{LockedPackage, LuxLockfile, PackageHash};
use lux_core::manifest::PyProjectToml;
use lux_resolver::PubGrubSolver;
use crate::provider::RegistryDependencyProvider;

use super::super::ui::{PackageDiff, Status};

/// Execute the `lux add` command to append dependencies, resolve, and install them into `.venv`.
#[allow(clippy::too_many_lines)]
pub async fn run_add(packages: &[String], editable: Option<&Path>, no_sync: bool) -> Result<()> {
    if packages.is_empty() && editable.is_none() {
        return Err(miette::miette!("No packages specified. Usage: `lux add <package...> [-e <path>]`"));
    }

    let total_start = Instant::now();
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Locate and update pyproject.toml
    let manifest_path = cwd.join("pyproject.toml");
    if !manifest_path.is_file() {
        return Err(miette::miette!(
            "No pyproject.toml found in {}. Run `lux init` first.",
            cwd.display()
        ));
    }

    let mut manifest = PyProjectToml::from_file(&manifest_path)
        .map_err(|e| miette::miette!("Failed to read pyproject.toml: {e}"))?;

    for pkg in packages {
        manifest
            .add_dependency(pkg)
            .map_err(|e| miette::miette!("Invalid requirement '{pkg}': {e}"))?;
    }

    let added_editable_info = if let Some(edit_path) = editable {
        let abs_path = std::fs::canonicalize(edit_path).unwrap_or_else(|_| edit_path.to_path_buf());
        let pyproject_path = abs_path.join("pyproject.toml");
        let pkg_name = if pyproject_path.is_file() {
            let member_toml = PyProjectToml::from_file(&pyproject_path)
                .map_err(|e| miette::miette!("Failed to read {}: {e}", pyproject_path.display()))?;
            member_toml.name().to_string()
        } else {
            abs_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("local_package")
                .to_string()
        };
        manifest
            .add_dependency(&pkg_name)
            .map_err(|e| miette::miette!("Failed to add editable dependency '{pkg_name}': {e}"))?;
        Some((pkg_name, abs_path))
    } else {
        None
    };

    manifest
        .save_to_file(&manifest_path)
        .map_err(|e| miette::miette!("Failed to save pyproject.toml: {e}"))?;

    // PubGrub-CDCL SAT Resolution
    Status::action("Resolving", "dependencies...");

    let all_reqs = manifest
        .parsed_requirements()
        .map_err(|e| miette::miette!("Failed to parse requirements: {e}"))?;

    let provider = RegistryDependencyProvider::new(None)
        .map_err(|e| miette::miette!("Failed to initialize dependency provider: {e}"))?;
    provider.prefetch_or_fallback(&all_reqs).await;

    let mut solver = PubGrubSolver::new(&provider);
    let solution = solver
        .resolve(&all_reqs)
        .map_err(|e| miette::miette!("Resolution error: {e}"))?;

    // Update lux.lock
    let lock_path = cwd.join("lux.lock");
    let mut lockfile = LuxLockfile::from_file(&lock_path).unwrap_or_else(|_| LuxLockfile::new());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp = format!("{}Z", now.as_secs());
    lockfile.created_at = Some(timestamp);

    let mut diff = PackageDiff::new();

    for (pkg_name, version) in &solution {
        let name_str = pkg_name.as_str();
        let ver_str = version.to_string();

        let dummy_hash = CasStore::compute_sha256(format!("{name_str}=={ver_str}").as_bytes());

        lockfile.add_or_update_package(LockedPackage {
            name: name_str.to_string(),
            version: ver_str.clone(),
            dependencies: Vec::new(),
            hashes: vec![PackageHash {
                algorithm: "sha256".to_string(),
                digest: dummy_hash,
            }],
        });

        diff.add(name_str, &ver_str);
    }

    lockfile
        .save_to_file(&lock_path)
        .map_err(|e| miette::miette!("Failed to write lux.lock: {e}"))?;

    // Environment Synchronization
    if !no_sync {
        let venv = VirtualEnv::find(&cwd).unwrap_or_else(|_| {
            VirtualEnv::create(&cwd.join(".venv"), Some("3.12.0")).expect("created fallback venv")
        });

        let site_packages = venv.site_packages_dir();
        let store = CasStore::open(None).map_err(|e| miette::miette!("Failed to open CAS store: {e}"))?;

        for (pkg_name, version) in &solution {
            let digest_opt = provider
                .install_package(pkg_name.as_str(), version, &store, site_packages)
                .await
                .map_err(|e| miette::miette!("Installation error for {pkg_name}: {e}"))?;

            if let Some(hash) = digest_opt {
                lockfile.add_or_update_package(LockedPackage {
                    name: pkg_name.to_string(),
                    version: version.to_string(),
                    dependencies: Vec::new(),
                    hashes: vec![PackageHash {
                        algorithm: "sha256".to_string(),
                        digest: hash,
                    }],
                });
            }
        }

        let _ = lockfile.save_to_file(&lock_path);

        if let Some((_, ref edit_path)) = added_editable_info {
            let (name, ver) = crate::commands::pip::install_editable(edit_path, site_packages)?;
            diff.add(name, format!("{ver} (editable)"));
        }
    }

    diff.render();
    Status::completed("Installed", &format!("{} package(s)", diff.added.len()), Some(total_start.elapsed()));

    Ok(())
}
