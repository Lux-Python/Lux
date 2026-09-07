//! Drop-in `pip` compatibility commands (`lux pip`).

use std::path::{Path, PathBuf};
use std::time::Instant;
use lux_cache::cas::CasStore;
use lux_core::env::VirtualEnv;
use lux_core::types::Requirement;
use lux_resolver::PubGrubSolver;
use crate::provider::RegistryDependencyProvider;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::{PackageDiff, Status};
use crate::ui::theme::Style;

/// Parse a requirements file recursively, extracting standard requirements and editable paths.
pub fn parse_requirements_file(
    file_path: &Path,
    collected_reqs: &mut Vec<Requirement>,
    collected_editables: &mut Vec<PathBuf>,
) -> Result<()> {
    let content = std::fs::read_to_string(file_path).into_diagnostic()?;
    let base_dir = file_path.parent().unwrap_or_else(|| Path::new("."));

    for line in content.lines() {
        let mut trimmed = line.trim();
        if let Some(idx) = trimmed.find('#') {
            trimmed = trimmed[..idx].trim();
        }
        if trimmed.is_empty() {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("-e ").or_else(|| trimmed.strip_prefix("--editable ")) {
            let edit_path = base_dir.join(rest.trim());
            collected_editables.push(edit_path);
        } else if let Some(rest) = trimmed.strip_prefix("-r ").or_else(|| trimmed.strip_prefix("--requirement ")) {
            let nested_req = base_dir.join(rest.trim());
            parse_requirements_file(&nested_req, collected_reqs, collected_editables)?;
        } else if let Ok(req) = Requirement::parse(trimmed) {
            collected_reqs.push(req);
        }
    }
    Ok(())
}

/// Install a local directory in editable mode into the virtual environment's site-packages.
pub fn install_editable(path: &Path, site_packages: &Path) -> Result<(String, String)> {
    let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (name, version) = {
        let pyproject_path = abs_path.join("pyproject.toml");
        if pyproject_path.is_file() {
            let manifest = lux_core::manifest::PyProjectToml::from_file(&pyproject_path)
                .map_err(|e| miette::miette!("Failed to read {}: {e}", pyproject_path.display()))?;
            let pkg_name = manifest.name().to_string();
            let pkg_ver = manifest.version().unwrap_or("0.1.0").to_string();
            (pkg_name, pkg_ver)
        } else {
            let dir_name = abs_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("local_package")
                .to_string();
            (dir_name, "0.1.0".to_string())
        }
    };

    let norm_name = lux_core::types::normalize_name(&name);

    // 1. Write <name>.pth referencing the absolute source directory
    let pth_content = format!("{}\n", abs_path.display());
    let pth_file = site_packages.join(format!("{norm_name}.pth"));
    std::fs::write(&pth_file, &pth_content).into_diagnostic()?;

    let underscored = norm_name.replace('-', "_");
    if underscored != norm_name {
        let pth_underscored = site_packages.join(format!("{underscored}.pth"));
        let _ = std::fs::write(pth_underscored, &pth_content);
    }

    // 2. Write <name>-<version>.dist-info directory for pip discovery
    let dist_info = site_packages.join(format!("{norm_name}-{version}.dist-info"));
    std::fs::create_dir_all(&dist_info).into_diagnostic()?;

    // METADATA
    let metadata_content = format!("Metadata-Version: 2.1\nName: {norm_name}\nVersion: {version}\n");
    std::fs::write(dist_info.join("METADATA"), metadata_content).into_diagnostic()?;

    // direct_url.json (PEP 660 / PEP 610)
    let direct_url = serde_json::json!({
        "url": format!("file:///{}", abs_path.display().to_string().replace('\\', "/")),
        "dir_info": { "editable": true }
    });
    std::fs::write(dist_info.join("direct_url.json"), direct_url.to_string()).into_diagnostic()?;

    Ok((norm_name, version))
}

/// Execute `lux pip install <packages...> [-r requirements.txt] [-e <path>]`.
pub async fn run_pip_install(
    packages: &[String],
    requirements: &[PathBuf],
    editables: &[PathBuf],
) -> Result<()> {
    let start = Instant::now();
    let cwd = std::env::current_dir().into_diagnostic()?;

    if packages.is_empty() && requirements.is_empty() && editables.is_empty() {
        return Err(miette::miette!(
            "No packages or requirements specified. Usage: `lux pip install <package...> [-r requirements.txt] [-e <path>]`"
        ));
    }

    let venv = VirtualEnv::find(&cwd).unwrap_or_else(|_| {
        let venv_dir = cwd.join(".venv");
        VirtualEnv::create(&venv_dir, None).expect("create venv")
    });

    let site_packages = venv.site_packages_dir();
    let mut diff = PackageDiff::default();

    // 1. Collect all requirements and editable targets
    let mut root_reqs = Vec::new();
    let mut all_editables = editables.to_vec();

    for req_file in requirements {
        parse_requirements_file(req_file, &mut root_reqs, &mut all_editables)?;
    }

    for pkg in packages {
        if let Ok(req) = Requirement::parse(pkg) {
            root_reqs.push(req);
        }
    }

    // 2. Install editable packages
    for edit_path in &all_editables {
        let (name, ver) = install_editable(edit_path, site_packages)?;
        diff.add(name, format!("{ver} (editable)"));
    }

    // 3. Resolve and install distribution requirements
    if !root_reqs.is_empty() {
        Status::action("Resolving", "dependencies via PubGrub SAT solver...");

        let provider = RegistryDependencyProvider::new(None)
            .map_err(|e| miette::miette!("Failed to initialize dependency provider: {e}"))?;
        provider.prefetch_or_fallback(&root_reqs).await;

        let mut solver = PubGrubSolver::new(&provider);
        let solution = solver.resolve(&root_reqs).map_err(|e| miette::miette!("{e}"))?;

        let store = CasStore::open(None).map_err(|e| miette::miette!("Failed to open CAS store: {e}"))?;

        for (pkg_name, version) in &solution {
            let _ = provider
                .install_package(pkg_name.as_str(), version, &store, site_packages)
                .await
                .map_err(|e| miette::miette!("Installation error for {pkg_name}: {e}"))?;

            diff.add(pkg_name.as_str(), version.to_string());
        }
    }

    diff.render();
    Status::completed(
        "Installed",
        &format!("{} package(s)", diff.added.len()),
        Some(start.elapsed()),
    );
    Ok(())
}

/// Execute `lux pip compile <file.in> [-o <file.txt>]`.
pub fn run_pip_compile(in_file: &Path, out_file: Option<PathBuf>) -> Result<()> {
    let start = Instant::now();
    let content = std::fs::read_to_string(in_file).into_diagnostic()?;

    let mut reqs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Ok(req) = Requirement::parse(trimmed) {
            reqs.push(req);
        }
    }

    let provider = RegistryDependencyProvider::new(None)
        .map_err(|e| miette::miette!("Failed to initialize dependency provider: {e}"))?;
    let p_clone = provider.clone();
    let r_clone = reqs.clone();
    crate::provider::run_async_safe(async move {
        p_clone.prefetch_or_fallback(&r_clone).await;
    });

    let mut solver = PubGrubSolver::new(&provider);
    let solution = solver.resolve(&reqs).map_err(|e| miette::miette!("{e}"))?;

    let mut output_lines = Vec::new();
    output_lines.push(format!("# Compiled by Lux on {}", chrono_stamp()));
    for (name, ver) in &solution {
        output_lines.push(format!("{}=={}", name.as_str(), ver));
    }
    output_lines.push(String::new());

    let target_out = out_file.unwrap_or_else(|| in_file.with_extension("txt"));
    std::fs::write(&target_out, output_lines.join("\n")).into_diagnostic()?;

    Status::completed(
        "Compiled",
        &format!(
            "{} requirement(s) -> {}",
            solution.len(),
            Style::bold(&target_out.display().to_string())
        ),
        Some(start.elapsed()),
    );
    Ok(())
}

/// Execute `lux pip list`.
pub fn run_pip_list() -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    let Ok(venv) = VirtualEnv::find(&cwd) else {
        Status::warn("No active virtual environment found. Run `lux init` or `lux pip install`.");
        return Ok(());
    };

    let installed = scan_installed_packages(venv.site_packages_dir());

    println!("{:<30} {:<15}", Style::bold("Package"), Style::bold("Version"));
    println!("{:-<30} {:-<15}", "", "");
    for (pkg, ver) in installed {
        println!("{:<30} {:<15}", pkg, Style::cyan(&ver));
    }
    Ok(())
}

/// Execute `lux pip freeze`.
pub fn run_pip_freeze() -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    if let Ok(venv) = VirtualEnv::find(&cwd) {
        let installed = scan_installed_packages(venv.site_packages_dir());
        for (pkg, ver) in installed {
            println!("{pkg}=={ver}");
        }
    }
    Ok(())
}

/// Execute `lux pip uninstall <packages...>`.
pub fn run_pip_uninstall(packages: &[String]) -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    let venv = VirtualEnv::find(&cwd).into_diagnostic()?;
    let site_packages = venv.site_packages_dir();

    let mut diff = PackageDiff::default();
    for pkg in packages {
        let pkg_dir = site_packages.join(pkg);
        if pkg_dir.exists() {
            let _ = std::fs::remove_dir_all(pkg_dir);
            
            let normalized_name = lux_core::types::normalize_name(pkg);
            let prefix = format!("{normalized_name}-");
            for entry in std::fs::read_dir(site_packages).into_diagnostic()? {
                let entry = entry.into_diagnostic()?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".dist-info") && name_str.starts_with(&prefix) {
                    std::fs::remove_dir_all(entry.path()).into_diagnostic()?;
                }
            }

            diff.remove(pkg.clone(), "installed");
        }
    }

    diff.render();
    Status::completed("Uninstalled", &format!("{} package(s)", packages.len()), None);
    Ok(())
}

/// Scans site-packages directory for `.dist-info` packages.
fn scan_installed_packages(site_packages: &Path) -> Vec<(String, String)> {
    let mut packages = Vec::new();
    if let Ok(entries) = std::fs::read_dir(site_packages) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if dir_name.ends_with(".dist-info") {
                        let stem = dir_name.trim_end_matches(".dist-info");
                        if let Some((name, ver)) = stem.split_once('-') {
                            packages.push((name.to_string(), ver.to_string()));
                        }
                    }
                }
            }
        }
    }
    packages.sort_by(|a, b| a.0.cmp(&b.0));
    packages
}

fn chrono_stamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple ISO 8601 without pulling in chrono crate
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Days since 1970-01-01
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

const fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Civil calendar algorithm from Howard Hinnant
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
