//! Global and ephemeral CLI tool runner and manager (`lux tool`).
//!
//! Manages isolated environments for CLI tools like `ruff`, `black`, `ipython`, and `pytest`.


use std::path::PathBuf;
use crate::cas::CasLayout;
use crate::error::CacheError;

/// Manager for isolated global and ephemeral tools.
pub struct ToolManager;

impl ToolManager {
    /// Directory housing isolated tool environments (`~/.lux/tools/`).
    #[must_use]
    pub fn tools_dir() -> PathBuf {
        CasLayout::default_root().parent().map_or_else(
            || PathBuf::from(".lux").join("tools"),
            |p| p.join("tools"),
        )
    }

    /// Directory where global tool entrypoint binaries/shims reside (`~/.lux/bin/`).
    #[must_use]
    pub fn bin_dir() -> PathBuf {
        CasLayout::default_root().parent().map_or_else(
            || PathBuf::from(".lux").join("bin"),
            |p| p.join("bin"),
        )
    }

    /// List all permanently installed tools in `~/.lux/tools/`.
    #[must_use]
    pub fn list_installed_tools() -> Vec<String> {
        let mut tools = Vec::new();
        let root = Self::tools_dir();
        if root.is_dir() {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            tools.push(name.to_string());
                        }
                    }
                }
            }
        }
        tools.sort();
        tools
    }

    /// Locate or prepare the isolated environment directory for a tool.
    pub fn get_or_create_tool_dir(tool_name: &str) -> Result<PathBuf, CacheError> {
        let tool_dir = Self::tools_dir().join(tool_name);
        if !tool_dir.exists() {
            std::fs::create_dir_all(&tool_dir).map_err(|source| CacheError::Io {
                path: tool_dir.display().to_string(),
                source,
            })?;
        }
        Ok(tool_dir)
    }

    /// Locate the tool executable inside its isolated environment.
    #[must_use]
    pub fn find_tool_executable(tool_name: &str) -> Option<PathBuf> {
        let tool_dir = Self::tools_dir().join(tool_name);
        let venv_dir = tool_dir.join(".venv");

        #[cfg(target_os = "windows")]
        {
            let exe = venv_dir.join("Scripts").join(format!("{tool_name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let bin = venv_dir.join("bin").join(tool_name);
            if bin.is_file() {
                return Some(bin);
            }
        }

        None
    }

    /// Uninstall a tool by removing its directory.
    pub fn uninstall_tool(tool_name: &str) -> Result<(), CacheError> {
        let tool_dir = Self::tools_dir().join(tool_name);
        if tool_dir.exists() {
            std::fs::remove_dir_all(&tool_dir).map_err(|source| CacheError::Io {
                path: tool_dir.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }
}
