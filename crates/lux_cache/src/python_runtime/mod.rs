//! Standalone Python runtime manager.
//!
//! Discovers and manages Python runtimes in `~/.lux/python/`.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// An installed Python interpreter discovered on the system or in the managed cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPython {
    /// Discovered version string (e.g. `"3.12.3"`).
    pub version: String,
    /// Absolute path to the python executable.
    pub path: PathBuf,
    /// Whether this runtime is managed by Lux (`~/.lux/python/`).
    pub is_managed: bool,
}

/// An available standalone Python distribution that Lux can automatically download and install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailablePython {
    /// Python release version (e.g. `"3.12.7"`).
    pub version: String,
    /// Release tag or vendor identifier.
    pub release: String,
    /// Platform target architecture string.
    pub target: String,
    /// Remote download URL.
    pub url: String,
}

/// Manager for host and portable Python interpreters.
pub struct PythonRuntime;

impl PythonRuntime {
    /// Returns the default directory for Lux-managed Python installations (`~/.lux/python`).
    #[must_use]
    pub fn managed_python_dir() -> PathBuf {
        if let Ok(val) = std::env::var("LUX_HOME") {
            if !val.trim().is_empty() {
                return PathBuf::from(val).join("python");
            }
        }

        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map_or_else(
                |_| PathBuf::from(".lux").join("python"),
                |home| PathBuf::from(home).join(".lux").join("python"),
            )
    }

    /// List all discovered installed Python runtimes on the host machine and in the Lux cache.
    #[must_use]
    pub fn list_installed() -> Vec<InstalledPython> {
        let mut installed = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        // 1. Scan managed ~/.lux/python directory
        let managed_dir = Self::managed_python_dir();
        if managed_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&managed_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        #[cfg(target_os = "windows")]
                        let exe = path.join("python.exe");
                        #[cfg(not(target_os = "windows"))]
                        let exe = path.join("bin").join("python3");

                        if exe.is_file() {
                            let ver = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();
                            seen_paths.insert(exe.clone());
                            installed.push(InstalledPython {
                                version: ver,
                                path: exe,
                                is_managed: true,
                            });
                        }
                    }
                }
            }
        }

        // 2. Scan PATH environment variable
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                #[cfg(target_os = "windows")]
                let candidates = ["python.exe", "python3.exe"];
                #[cfg(not(target_os = "windows"))]
                let candidates = [
                    "python3",
                    "python3.13",
                    "python3.12",
                    "python3.11",
                    "python3.10",
                    "python",
                ];

                for cand in candidates {
                    let candidate_path = dir.join(cand);
                    if candidate_path.is_file() && !seen_paths.contains(&candidate_path) {
                        seen_paths.insert(candidate_path.clone());
                        let ver = query_python_version(&candidate_path)
                            .unwrap_or_else(|| "3.x".to_string());
                        installed.push(InstalledPython {
                            version: ver,
                            path: candidate_path,
                            is_managed: false,
                        });
                    }
                }
            }
        }

        // 3. Scan common OS installation directories on Windows
        #[cfg(target_os = "windows")]
        {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let py_dir = PathBuf::from(local_app_data).join("Programs").join("Python");
                if py_dir.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(&py_dir) {
                        for entry in entries.flatten() {
                            let exe = entry.path().join("python.exe");
                            if exe.is_file() && !seen_paths.contains(&exe) {
                                seen_paths.insert(exe.clone());
                                let ver = query_python_version(&exe)
                                    .unwrap_or_else(|| "3.x".to_string());
                                installed.push(InstalledPython {
                                    version: ver,
                                    path: exe,
                                    is_managed: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        installed.sort_by(|a, b| a.version.cmp(&b.version));
        installed
    }

    /// List all downloadable Python distributions available from python-build-standalone.
    #[must_use]
    pub fn list_available() -> Vec<AvailablePython> {
        let platform = if cfg!(target_os = "windows") {
            "x86_64-pc-windows-msvc"
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "aarch64-apple-darwin"
            } else {
                "x86_64-apple-darwin"
            }
        } else {
            if cfg!(target_arch = "aarch64") {
                "aarch64-unknown-linux-gnu"
            } else {
                "x86_64-unknown-linux-gnu"
            }
        };

        vec![
            AvailablePython {
                version: "3.13.0".to_string(),
                release: "20241016".to_string(),
                target: platform.to_string(),
                url: format!("https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.13.0+{platform}-install_only.tar.gz"),
            },
            AvailablePython {
                version: "3.12.7".to_string(),
                release: "20241016".to_string(),
                target: platform.to_string(),
                url: format!("https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.12.7+{platform}-install_only.tar.gz"),
            },
            AvailablePython {
                version: "3.11.10".to_string(),
                release: "20241016".to_string(),
                target: platform.to_string(),
                url: format!("https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.11.10+{platform}-install_only.tar.gz"),
            },
            AvailablePython {
                version: "3.10.15".to_string(),
                release: "20241016".to_string(),
                target: platform.to_string(),
                url: format!("https://github.com/astral-sh/python-build-standalone/releases/download/20241016/cpython-3.10.15+{platform}-install_only.tar.gz"),
            },
        ]
    }

    /// Find an installed Python executable matching a version prefix (e.g. `"3.12"` or `"3"`).
    #[must_use]
    pub fn find_matching_python(prefix: Option<&str>) -> Option<PathBuf> {
        let installed = Self::list_installed();
        if let Some(pref) = prefix {
            for py in &installed {
                if py.version.starts_with(pref) {
                    return Some(py.path.clone());
                }
            }
        } else if let Some(py) = installed.first() {
            return Some(py.path.clone());
        }

        None
    }
}

/// Query Python version by launching `python --version`.
fn query_python_version(exe: &Path) -> Option<String> {
    let output = std::process::Command::new(exe)
        .arg("--version")
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let out = if stdout.trim().is_empty() { stderr } else { stdout };
        let parts: Vec<&str> = out.split_whitespace().collect();
        if parts.len() >= 2 {
            return Some(parts[1].trim().to_string());
        }
    }
    None
}
