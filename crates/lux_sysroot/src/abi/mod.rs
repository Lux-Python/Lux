//! ABI dynamic dependency inspector for native Python extensions (.so, .pyd, .dylib).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use object::Object;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// ABI inspection errors.
#[derive(Debug, Error)]
pub enum AbiError {
    #[error("I/O error while reading binary {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse binary format for {path}: {source}")]
    Parse {
        path: PathBuf,
        source: object::Error,
    },
}

/// Dynamic binary format kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryFormat {
    Elf,
    Pe,
    MachO,
    Unknown,
}

impl std::fmt::Display for BinaryFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Elf => write!(f, "ELF (Linux)"),
            Self::Pe => write!(f, "PE/COFF (Windows)"),
            Self::MachO => write!(f, "Mach-O (macOS)"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Representation of a required dynamic library dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DynamicDependency {
    /// Name of the required dynamic library (e.g. `libgfortran.so.5`, `openblas.dll`).
    pub name: String,
    /// Absolute path if resolved, or `None` if missing.
    pub resolved_path: Option<PathBuf>,
    /// Whether this dependency is typically provided by the OS system runtime.
    pub is_system: bool,
}

/// Comprehensive ABI health report for a compiled Python extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryAbiReport {
    /// Path to the inspected binary file.
    pub path: PathBuf,
    /// Binary format (PE, ELF, Mach-O).
    pub format: BinaryFormat,
    /// Architecture (e.g. `x86_64`, `aarch64`).
    pub architecture: String,
    /// Extracted dynamic dependencies.
    pub dependencies: Vec<DynamicDependency>,
}

impl BinaryAbiReport {
    /// Returns `true` if all dynamic dependencies are satisfied (resolved).
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.dependencies.iter().all(|d| d.resolved_path.is_some())
    }

    /// Returns list of missing dynamic dependencies.
    #[must_use]
    pub fn missing_dependencies(&self) -> Vec<&DynamicDependency> {
        self.dependencies.iter().filter(|d| d.resolved_path.is_none()).collect()
    }
}

/// Inspector capable of analyzing binary dependencies and resolving against sysroots.
#[derive(Debug, Clone)]
pub struct AbiInspector {
    /// Search paths used to resolve dynamic library dependencies.
    search_paths: Vec<PathBuf>,
}

impl Default for AbiInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl AbiInspector {
    /// Creates a new `AbiInspector` populated with standard system and environment search paths.
    #[must_use]
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // 1. PATH environment variable
        if let Ok(path_var) = std::env::var("PATH") {
            for entry in std::env::split_paths(&path_var) {
                if entry.exists() && entry.is_dir() {
                    search_paths.push(entry);
                }
            }
        }

        // 2. LD_LIBRARY_PATH (Unix)
        if let Ok(ld_var) = std::env::var("LD_LIBRARY_PATH") {
            for entry in std::env::split_paths(&ld_var) {
                if entry.exists() && entry.is_dir() {
                    search_paths.push(entry);
                }
            }
        }

        // 3. Common OS directories
        #[cfg(target_os = "windows")]
        {
            if let Ok(sys_root) = std::env::var("SystemRoot") {
                let p = PathBuf::from(&sys_root).join("System32");
                if p.exists() {
                    search_paths.push(p);
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            for dir in &["/lib", "/usr/lib", "/usr/local/lib", "/lib64", "/usr/lib64"] {
                let p = PathBuf::from(dir);
                if p.exists() {
                    search_paths.push(p);
                }
            }
        }

        Self { search_paths }
    }

    /// Add a custom search path (e.g. `.venv/sysroot/lib` or `.venv/Lib/site-packages`).
    pub fn add_search_path(&mut self, path: impl Into<PathBuf>) {
        let p = path.into();
        if !self.search_paths.contains(&p) {
            self.search_paths.push(p);
        }
    }

    /// Inspect a binary on disk.
    pub fn inspect_file(&self, path: &Path) -> Result<BinaryAbiReport, AbiError> {
        let bytes = std::fs::read(path).map_err(|source| AbiError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        self.inspect_bytes(&bytes, path)
    }

    /// Inspect binary contents from in-memory byte slice.
    pub fn inspect_bytes(&self, bytes: &[u8], path: &Path) -> Result<BinaryAbiReport, AbiError> {
        let file = object::File::parse(bytes).map_err(|source| AbiError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

        let format = match file.format() {
            object::BinaryFormat::Elf => BinaryFormat::Elf,
            object::BinaryFormat::Pe => BinaryFormat::Pe,
            object::BinaryFormat::MachO => BinaryFormat::MachO,
            _ => BinaryFormat::Unknown,
        };

        let architecture = format!("{:?}", file.architecture()).to_lowercase();

        // Collect unique library names required by the binary
        let mut required_libs = HashSet::new();

        // 1. Through object imports API (PE, Mach-O, and ELF symbols)
        if let Ok(imports) = file.imports() {
            for import in imports {
                let lib_bytes = import.library();
                if !lib_bytes.is_empty() {
                    if let Ok(lib_name) = std::str::from_utf8(lib_bytes) {
                        required_libs.insert(lib_name.to_string());
                    }
                }
            }
        }

        // 2. Binary-specific segment/table inspection
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));

        let mut dependencies = Vec::new();
        for lib in required_libs {
            let is_sys = is_system_library(&lib);
            let resolved_path = self.resolve_library(&lib, parent_dir);
            dependencies.push(DynamicDependency {
                name: lib,
                resolved_path,
                is_system: is_sys,
            });
        }

        // Sort dependencies alphabetically for deterministic output
        dependencies.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(BinaryAbiReport {
            path: path.to_path_buf(),
            format,
            architecture,
            dependencies,
        })
    }

    /// Recursively scan a directory for compiled Python native extensions (`.pyd`, `.so`, `.dylib`).
    pub fn scan_directory(&self, root: &Path) -> Result<Vec<BinaryAbiReport>, AbiError> {
        let mut reports = Vec::new();
        if !root.exists() {
            return Ok(reports);
        }

        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if is_native_extension(&path) {
                    if let Ok(report) = self.inspect_file(&path) {
                        reports.push(report);
                    }
                }
            }
        }

        reports.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(reports)
    }

    /// Attempt to resolve a dynamic library name against search paths.
    fn resolve_library(&self, lib_name: &str, origin: &Path) -> Option<PathBuf> {
        // Check relative to origin binary directory ($ORIGIN / package directory)
        let direct_origin = origin.join(lib_name);
        if direct_origin.exists() && direct_origin.is_file() {
            return Some(direct_origin);
        }

        // Check common vendor library subdirectories (e.g. numpy.libs, scipy.libs, .libs)
        if let Ok(entries) = std::fs::read_dir(origin) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".libs")) {
                    let candidate = p.join(lib_name);
                    if candidate.exists() && candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }

        // Check configured search paths (sysroot, venv, system)
        for search_dir in &self.search_paths {
            let candidate = search_dir.join(lib_name);
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }
            // On Windows, also check case-insensitively or with .dll extension if missing
            #[cfg(target_os = "windows")]
            {
                if !lib_name.to_lowercase().ends_with(".dll") {
                    let candidate_dll = search_dir.join(format!("{lib_name}.dll"));
                    if candidate_dll.exists() && candidate_dll.is_file() {
                        return Some(candidate_dll);
                    }
                }
            }
        }

        None
    }
}

/// Checks if a file path is a compiled native Python extension.
fn is_native_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    let ext = ext.to_lowercase();
    if ext == "pyd" || ext == "dylib" {
        return true;
    }

    if ext == "so" {
        return true;
    }

    // Handles compound extensions like `.cpython-311-x86_64-linux-gnu.so`
    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        if file_name.ends_with(".so") || file_name.contains(".so.") {
            return true;
        }
    }

    false
}

/// Determines if a library is a standard OS runtime component.
fn is_system_library(lib_name: &str) -> bool {
    let lower = lib_name.to_lowercase();

    // Windows base system DLLs
    if lower.starts_with("kernel32")
        || lower.starts_with("user32")
        || lower.starts_with("advapi32")
        || lower.starts_with("ws2_32")
        || lower.starts_with("ntdll")
        || lower.starts_with("api-ms-win")
        || lower.starts_with("msvcrt")
        || lower.starts_with("vcruntime")
        || lower.starts_with("ucrtbase")
    {
        return true;
    }

    // Linux base dynamic loader and libc
    if lower.starts_with("libc.so")
        || lower.starts_with("libm.so")
        || lower.starts_with("libpthread.so")
        || lower.starts_with("libdl.so")
        || lower.starts_with("librt.so")
        || lower.starts_with("ld-linux")
    {
        return true;
    }

    // macOS base runtime
    if lower.contains("libsystem") || lower.contains("libc++") || lower.contains("libobjc") {
        return true;
    }

    false
}
