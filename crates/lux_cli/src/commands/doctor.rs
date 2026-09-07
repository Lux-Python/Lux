//! Environment and binary ABI diagnostic command.

use std::time::Instant;
use lux_core::env::VirtualEnv;
use lux_sysroot::abi::AbiInspector;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute the `lux doctor` diagnostic command.
pub fn execute() -> Result<()> {
    let start_time = Instant::now();
    let cwd = std::env::current_dir().into_diagnostic()?;
    let venv_dir = cwd.join(".venv");

    println!("Scanning environment at {}", Style::dim(&cwd.display().to_string()));

    if !venv_dir.exists() {
        Status::warn("No virtual environment found at .venv. Run `lux init` first.");
        return Ok(());
    }

    let venv = VirtualEnv::find(&cwd).into_diagnostic()?;
    let site_packages = venv.site_packages_dir();
    let sysroot_dir = venv_dir.join("sysroot");

    println!("   Virtual environment: {}", Style::bold(&venv.root().display().to_string()));
    if sysroot_dir.exists() {
        println!("   Hermetic sysroot:    {}", Style::green(&sysroot_dir.display().to_string()));
    } else {
        println!("   Hermetic sysroot:    {}", Style::dim("none"));
    }

    let mut inspector = AbiInspector::new();
    if site_packages.exists() {
        inspector.add_search_path(site_packages);
    }
    if sysroot_dir.exists() {
        inspector.add_search_path(sysroot_dir.join("lib"));
        inspector.add_search_path(sysroot_dir.join("bin"));
    }

    println!();
    println!("Inspecting native binary extensions (.so, .pyd, .dylib)...");

    let reports = inspector.scan_directory(site_packages).into_diagnostic()?;
    let duration = start_time.elapsed();

    if reports.is_empty() {
        println!("   {}", Style::dim("No compiled native extensions found in site-packages."));
        println!();
        Status::completed("Diagnostics", "Environment is clean", Some(duration));
        return Ok(());
    }

    let mut total_missing = 0;
    for report in &reports {
        let rel_path = report
            .path
            .strip_prefix(site_packages)
            .unwrap_or(&report.path);

        if report.is_satisfied() {
            println!(
                "   {} {} ({}, {} dependencies)",
                Style::green("ok"),
                Style::bold(&rel_path.display().to_string()),
                report.architecture,
                report.dependencies.len()
            );
        } else {
            let missing = report.missing_dependencies();
            total_missing += missing.len();
            println!(
                "   {} {} ({})",
                Style::red("missing"),
                Style::bold(&rel_path.display().to_string()),
                report.architecture
            );
            for dep in missing {
                println!("      {}", Style::red(&format!("- {}", dep.name)));
            }
        }
    }

    println!();
    if total_missing == 0 {
        Status::completed(
            "Diagnostics",
            &format!("All {} binary extension(s) satisfied", reports.len()),
            Some(duration),
        );
    } else {
        Status::warn(
            &format!("{total_missing} missing dynamic library dependency(ies) detected"),
        );
    }

    Ok(())
}
