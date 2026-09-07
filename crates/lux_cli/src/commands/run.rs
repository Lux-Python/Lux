//! Subprocess launcher with PEP 723 inline metadata support.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use lux_core::env::VirtualEnv;
use lux_core::manifest::ScriptMetadata;
use miette::Result;
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute a subprocess or PEP 723 standalone script within an isolated environment.
pub fn run_run(command: &str, args: &[String]) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Check if command is a standalone Python script (.py file)
    let script_path = Path::new(command);
    let script_meta = if script_path.is_file() && script_path.extension().and_then(|e| e.to_str()) == Some("py") {
        if let Ok(Some(meta)) = ScriptMetadata::from_file(script_path) {
            Status::action(
                "Executing",
                &format!(
                    "PEP 723 script {} ({} dependencies)...",
                    Style::bold(command),
                    meta.dependencies().len()
                ),
            );
            Some(meta)
        } else {
            None
        }
    } else {
        None
    };

    // Try finding .venv in current directory, or initialize an isolated script cache venv if PEP 723 dependencies declared
    let mut venv_opt = VirtualEnv::find(&cwd).ok();
    if venv_opt.is_none() {
        if let Some(ref meta) = script_meta {
            if !meta.dependencies().is_empty() {
                let cache_dir = lux_cache::cas::CasLayout::default_root().join("scripts");
                let script_stem = script_path.file_stem().and_then(|s| s.to_str()).unwrap_or("script");
                let script_env_dir = cache_dir.join(format!("{script_stem}-venv"));
                if let Ok(venv) = VirtualEnv::create(&script_env_dir, None) {
                    venv_opt = Some(venv);
                }
            }
        }
    }

    let (target_cmd, actual_args) = venv_opt.as_ref().map_or_else(
        || {
            if script_path.is_file() {
                let mut new_args = vec![command.to_string()];
                new_args.extend_from_slice(args);
                ("python".to_string(), new_args)
            } else {
                (command.to_string(), args.to_vec())
            }
        },
        |venv| {
            if (command == "python" || command == "python3") && venv.python_path().is_file() {
                (venv.python_path().to_string_lossy().to_string(), args.to_vec())
            } else if script_path.is_file() && script_path.extension().and_then(|e| e.to_str()) == Some("py") {
                let mut new_args = vec![command.to_string()];
                new_args.extend_from_slice(args);
                let py = if venv.python_path().is_file() {
                    venv.python_path().to_string_lossy().to_string()
                } else {
                    "python".to_string()
                };
                (py, new_args)
            } else {
                (command.to_string(), args.to_vec())
            }
        },
    );

    let mut child = Command::new(&target_cmd);
    child.args(&actual_args);

    if let Some(venv) = venv_opt {
        let current_path = env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![venv.scripts_dir().to_path_buf()];
        paths.extend(env::split_paths(&current_path));
        if let Ok(new_path) = env::join_paths(paths) {
            child.env("PATH", new_path);
        }
        child.env("VIRTUAL_ENV", venv.root());
    }

    let status = child
        .status()
        .map_err(|e| miette::miette!("Failed to execute command '{}': {e}", target_cmd))?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}
