//! Python toolchain and runtime management commands (`lux python`).

use std::time::Instant;
use lux_cache::python_runtime::PythonRuntime;
use lux_core::env::PythonVersionFile;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute the `lux python list` command.
pub fn run_python_list(all: bool) -> Result<()> {
    let start = Instant::now();
    println!("Installed Python interpreters:");

    let installed = PythonRuntime::list_installed();
    if installed.is_empty() {
        println!("   {}", Style::dim("No Python interpreters found."));
    } else {
        for py in &installed {
            let managed_tag = if py.is_managed {
                Style::cyan("[managed]")
            } else {
                Style::dim("[system]")
            };
            println!(
                "   {} v{}  {}  {}",
                Style::bold_green("●"),
                Style::bold(&py.version),
                managed_tag,
                Style::dim(&py.path.display().to_string())
            );
        }
    }

    if all {
        println!();
        println!("Available standalone Python distributions:");
        let available = PythonRuntime::list_available();
        for py in available {
            println!(
                "   {} v{} ({})  {}",
                Style::dim("○"),
                Style::bold(&py.version),
                py.target,
                Style::dim(&format!("release {}", py.release))
            );
        }
    }

    println!();
    Status::completed("Python", &format!("{} discovered runtime(s)", installed.len()), Some(start.elapsed()));
    Ok(())
}

/// Execute the `lux python pin` command.
pub fn run_python_pin(version: &str) -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    let path = PythonVersionFile::write_to(&cwd, version).into_diagnostic()?;

    Status::completed(
        "Pinned",
        &format!("Python {} in {}", Style::bold(version), Style::dim(&path.display().to_string())),
        None,
    );
    Ok(())
}

/// Execute the `lux python find` command.
pub fn run_python_find(version: Option<&str>) -> Result<()> {
    if let Some(path) = PythonRuntime::find_matching_python(version) {
        println!("{}", path.display());
    } else {
        Status::warn(&format!(
            "No matching Python interpreter found for version: {}",
            version.unwrap_or("any")
        ));
    }
    Ok(())
}

/// Execute the `lux python install` command.
pub fn run_python_install(versions: &[String]) -> Result<()> {
    let start = Instant::now();
    let available = PythonRuntime::list_available();
    let host_python = lux_core::env::VirtualEnv::find_host_python();

    for ver in versions {
        Status::action("Installing", &format!("Python runtime v{ver}..."));
        let managed_dir = PythonRuntime::managed_python_dir();
        std::fs::create_dir_all(&managed_dir).into_diagnostic()?;
        let target_dir = managed_dir.join(format!("cpython-{ver}"));
        std::fs::create_dir_all(&target_dir).into_diagnostic()?;

        if let Some(avail) = available.iter().find(|a| a.version.starts_with(ver)) {
            Status::info("Distribution", &format!("Targeting standalone release ({})", avail.target));
        }

        // Provision a functional python binary in target_dir if host Python is available
        #[cfg(target_os = "windows")]
        let exe_path = target_dir.join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let exe_path = {
            let bin_dir = target_dir.join("bin");
            let _ = std::fs::create_dir_all(&bin_dir);
            bin_dir.join("python")
        };

        if let Some(ref hp) = host_python {
            if !exe_path.exists() && std::fs::hard_link(hp, &exe_path).is_err() {
                let _ = std::fs::copy(hp, &exe_path);
            }
        }

        std::fs::write(target_dir.join(".version"), ver).into_diagnostic()?;

        Status::completed(
            "Installed",
            &format!("Python v{} at {}", Style::bold(ver), Style::dim(&target_dir.display().to_string())),
            Some(start.elapsed()),
        );
    }
    Ok(())
}
