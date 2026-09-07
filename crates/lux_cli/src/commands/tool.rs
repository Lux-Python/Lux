//! Global and ephemeral CLI tool runner and manager (`lux tool`).

use std::time::Instant;
use lux_cache::tools::ToolManager;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute the `lux tool run` command.
pub fn run_tool_run(tool: &str, args: &[String]) -> Result<()> {
    let start = Instant::now();
    Status::action("Running", &format!("tool {}...", Style::bold(tool)));

    // Ensure tool environment directory exists
    let tool_dir = ToolManager::get_or_create_tool_dir(tool).into_diagnostic()?;

    // Locate tool executable or fall back to system tool
    let exe = ToolManager::find_tool_executable(tool).unwrap_or_else(|| {
        #[cfg(target_os = "windows")]
        {
            std::path::PathBuf::from(format!("{tool}.exe"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::path::PathBuf::from(tool)
        }
    });

    println!(
        "   Isolated tool sandbox: {}",
        Style::dim(&tool_dir.display().to_string())
    );

    let mut cmd = std::process::Command::new(&exe);
    cmd.args(args);

    let status = cmd.status().into_diagnostic()?;

    if status.success() {
        Status::completed("Executed", tool, Some(start.elapsed()));
    } else {
        Status::warn(&format!(
            "Tool '{tool}' exited with code {:?}",
            status.code()
        ));
    }

    Ok(())
}

/// Execute the `lux tool install` command.
pub fn run_tool_install(tool: &str) -> Result<()> {
    let start = Instant::now();
    Status::action("Installing", &format!("global tool {}...", Style::bold(tool)));

    let tool_dir = ToolManager::get_or_create_tool_dir(tool).into_diagnostic()?;
    let bin_dir = ToolManager::bin_dir();
    std::fs::create_dir_all(&bin_dir).into_diagnostic()?;

    // Create an isolated virtual environment for this tool
    let venv_dir = tool_dir.join(".venv");
    let venv = lux_core::env::VirtualEnv::create(&venv_dir, None).into_diagnostic()?;

    // Generate an entrypoint shim in ~/.lux/bin/
    #[cfg(target_os = "windows")]
    {
        let shim_path = bin_dir.join(format!("{tool}.cmd"));
        let content = format!(
            "@echo off\r\n\"{}\" -m {} %*\r\n",
            venv.python_path().display(),
            tool
        );
        std::fs::write(&shim_path, content).into_diagnostic()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let shim_path = bin_dir.join(tool);
        let content = format!(
            "#!/bin/sh\nexec \"{}\" -m {} \"$@\"\n",
            venv.python_path().display(),
            tool
        );
        std::fs::write(&shim_path, content).into_diagnostic()?;
        let mut perms = std::fs::metadata(&shim_path).into_diagnostic()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim_path, perms).into_diagnostic()?;
    }

    Status::completed(
        "Installed",
        &format!(
            "{} at {}",
            Style::bold(tool),
            Style::dim(&tool_dir.display().to_string())
        ),
        Some(start.elapsed()),
    );
    Ok(())
}

/// Execute the `lux tool list` command.
pub fn run_tool_list() -> Result<()> {
    println!("Installed global tools:");
    let tools = ToolManager::list_installed_tools();

    if tools.is_empty() {
        println!("   {}", Style::dim("No tools currently installed. Run `lux tool install <tool>` to install."));
    } else {
        for tool in tools {
            println!("   {} {}", Style::green("●"), Style::bold(&tool));
        }
    }
    println!();
    Ok(())
}

/// Execute the `lux tool uninstall` command.
pub fn run_tool_uninstall(tool: &str) -> Result<()> {
    ToolManager::uninstall_tool(tool).into_diagnostic()?;
    Status::completed("Uninstalled", tool, None);
    Ok(())
}
