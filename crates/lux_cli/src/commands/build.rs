//! Isolated PEP 517 build command.

use std::path::PathBuf;
use std::time::Instant;
use lux_core::env::VirtualEnv;
use lux_sysroot::builder::Pep517Builder;
use lux_sysroot::overlay::SysrootOverlay;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute the `lux build` command.
pub async fn execute(out_dir: Option<PathBuf>, sdist: bool, wheel: bool) -> Result<()> {
    let start_time = Instant::now();
    let cwd = std::env::current_dir().into_diagnostic()?;

    let target_out_dir = out_dir.unwrap_or_else(|| cwd.join("dist"));

    println!("Building project at {}", Style::dim(&cwd.display().to_string()));

    let mut builder = Pep517Builder::from_source_dir(&cwd).into_diagnostic()?;

    // If a virtual environment contains a hermetic sysroot, wire it into the builder
    let sysroot_dir = cwd.join(".venv").join("sysroot");
    if sysroot_dir.exists() {
        println!("   Using sysroot: {}", Style::green(&sysroot_dir.display().to_string()));
        let sysroot = SysrootOverlay::new(sysroot_dir);
        builder = builder.with_sysroot(sysroot);
    }

    println!("   Build backend: {}", Style::bold(&builder.config().build_backend));
    println!("   Output directory: {}", Style::dim(&target_out_dir.display().to_string()));
    println!();

    let python_bin = VirtualEnv::find(&cwd).ok().and_then(|venv| {
        #[cfg(target_os = "windows")]
        let bin = venv.scripts_dir().join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let bin = venv.bin_dir().join("python");

        if bin.exists() {
            Some(bin)
        } else {
            None
        }
    });

    let build_wheel = wheel || !sdist;
    let build_sdist = sdist || !wheel;

    if build_sdist {
        Status::action("Building", "source distribution via PEP 517 backend");
        match builder.build_sdist(&target_out_dir, python_bin.as_deref()).await {
            Ok(sdist_path) => {
                Status::completed(
                    "Built",
                    &format!("sdist artifact at {}", Style::bold(&sdist_path.display().to_string())),
                    Some(start_time.elapsed()),
                );
            }
            Err(e) => {
                Status::warn(&format!("Failed to build sdist: {e}"));
            }
        }
    }

    if build_wheel {
        Status::action("Building", "wheel distribution via PEP 517 backend");
        let wheel_path = builder
            .build_wheel(&target_out_dir, python_bin.as_deref())
            .await
            .into_diagnostic()?;

        let duration = start_time.elapsed();
        Status::completed(
            "Built",
            &format!("wheel artifact at {}", Style::bold(&wheel_path.display().to_string())),
            Some(duration),
        );
    }

    Ok(())
}
