//! Hermetic PEP 517 / PEP 518 source distribution (sdist) build orchestrator.
//!
//! Executes build backend hooks (`build_wheel`, `build_sdist`) in an isolated environment
//! with sysroot compiler flags (`CFLAGS`, `LDFLAGS`, `CPATH`, `LIBRARY_PATH`) automatically injected.

use std::path::{Path, PathBuf};
use lux_core::manifest::PyProjectToml;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::overlay::SysrootOverlay;

/// Errors that can occur during PEP 517 builds.
#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("Manifest error: {0}")]
    Manifest(String),
    #[error("I/O error during build orchestration: {0}")]
    Io(#[from] std::io::Error),
    #[error("Build backend execution failed with exit code {code:?}: {stderr}")]
    Execution {
        code: Option<i32>,
        stderr: String,
    },
    #[error("Build produced no artifacts in output directory: {0}")]
    MissingArtifact(PathBuf),
}

/// Metadata and configuration for an isolated PEP 517 build session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pep517BuildConfig {
    /// Directory containing the source project (`pyproject.toml` or `setup.py`).
    pub source_dir: PathBuf,
    /// Build backend specified in `[build-system]` (e.g. `setuptools.build_meta`, `flit_core.buildapi`, `hatchling.build`).
    pub build_backend: String,
    /// Packages required to build the project (`[build-system].requires`).
    pub build_requires: Vec<String>,
}

/// Isolated PEP 517 build orchestrator.
#[derive(Debug)]
pub struct Pep517Builder {
    config: Pep517BuildConfig,
    sysroot: Option<SysrootOverlay>,
}

impl Pep517Builder {
    /// Inspects the target source directory and initializes a builder based on `pyproject.toml`.
    pub fn from_source_dir(source_dir: impl Into<PathBuf>) -> Result<Self, BuilderError> {
        let source_dir = source_dir.into();
        let pyproject_path = source_dir.join("pyproject.toml");

        if pyproject_path.exists() {
            let content = std::fs::read_to_string(&pyproject_path)?;
            let manifest = PyProjectToml::parse(&content)
                .map_err(|e| BuilderError::Manifest(e.to_string()))?;

            let (backend, requires) = manifest
                .build_system
                .as_ref()
                .map(|b| {
                    let backend = b.build_backend.clone().unwrap_or_else(|| "setuptools.build_meta".to_string());
                    (backend, b.requires.clone())
                })
                .unwrap_or_else(|| {
                    ("setuptools.build_meta".to_string(), vec!["setuptools".to_string(), "wheel".to_string()])
                });

            Ok(Self {
                config: Pep517BuildConfig {
                    source_dir,
                    build_backend: backend,
                    build_requires: requires,
                },
                sysroot: None,
            })
        } else {
            // Fallback for legacy projects with only setup.py
            Ok(Self {
                config: Pep517BuildConfig {
                    source_dir,
                    build_backend: "setuptools.build_meta".to_string(),
                    build_requires: vec!["setuptools".to_string(), "wheel".to_string()],
                },
                sysroot: None,
            })
        }
    }

    /// Attaches a hermetic sysroot overlay to the build session.
    pub fn with_sysroot(mut self, sysroot: SysrootOverlay) -> Self {
        self.sysroot = Some(sysroot);
        self
    }

    /// Returns the build configuration.
    #[must_use]
    pub const fn config(&self) -> &Pep517BuildConfig {
        &self.config
    }

    /// Generates the hermetic Python script that imports the PEP 517 build backend and executes `build_wheel`.
    #[must_use]
    pub fn generate_runner_script(&self, hook: &str, out_dir: &Path) -> String {
        let backend = &self.config.build_backend;
        let out_dir_str = out_dir.to_string_lossy().replace('\\', "/");
        let (module, object) = if let Some((m, o)) = backend.split_once(':') {
            (m, o)
        } else {
            (backend.as_str(), "")
        };

        format!(
            r#"import sys
import importlib

# Ensure build backend is loaded
try:
    mod = importlib.import_module("{module}")
    if "{object}":
        backend_obj = getattr(mod, "{object}")
    else:
        backend_obj = mod
    backend_func = getattr(backend_obj, "{hook}")
except Exception as e:
    sys.stderr.write(f"Failed to load PEP 517 backend '{module}': {{e}}\n")
    sys.exit(1)

try:
    # Execute build hook
    wheel_filename = backend_func("{out_dir_str}")
    sys.stdout.write(str(wheel_filename) + "\n")
except Exception as e:
    sys.stderr.write(f"Error during {hook}: {{e}}\n")
    sys.exit(2)
"#
        )
    }

    /// Builds the project into a wheel inside `output_dir`.
    pub async fn build_wheel(
        &self,
        output_dir: &Path,
        python_executable: Option<&Path>,
    ) -> Result<PathBuf, BuilderError> {
        std::fs::create_dir_all(output_dir)?;

        let temp_dir = tempfile::tempdir()?;
        let runner_path = temp_dir.path().join("pep517_runner.py");
        let script = self.generate_runner_script("build_wheel", output_dir);
        std::fs::write(&runner_path, script)?;

        let python = python_executable
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "python".to_string());

        let mut cmd = tokio::process::Command::new(&python);
        cmd.arg(&runner_path)
            .current_dir(&self.config.source_dir);

        // Inject sysroot environment overrides
        if let Some(sysroot) = &self.sysroot {
            for (key, val) in sysroot.environment_overrides() {
                cmd.env(key, val);
            }
        }

        // Hermetic flags: prevent picking up global site-packages
        cmd.env("PYTHONNOUSERSITE", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");

        let output = cmd.output().await.map_err(BuilderError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(BuilderError::Execution {
                code: output.status.code(),
                stderr,
            });
        }

        // Locate generated .whl in output_dir
        for entry in std::fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("whl") {
                return Ok(path);
            }
        }

        Err(BuilderError::MissingArtifact(output_dir.to_path_buf()))
    }

    /// Builds the project into a source distribution (.tar.gz or .zip) inside `output_dir`.
    pub async fn build_sdist(
        &self,
        output_dir: &Path,
        python_executable: Option<&Path>,
    ) -> Result<PathBuf, BuilderError> {
        std::fs::create_dir_all(output_dir)?;

        let temp_dir = tempfile::tempdir()?;
        let runner_path = temp_dir.path().join("pep517_runner.py");
        let script = self.generate_runner_script("build_sdist", output_dir);
        std::fs::write(&runner_path, script)?;

        let python = python_executable
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "python".to_string());

        let mut cmd = tokio::process::Command::new(&python);
        cmd.arg(&runner_path)
            .current_dir(&self.config.source_dir);

        // Inject sysroot environment overrides
        if let Some(sysroot) = &self.sysroot {
            for (key, val) in sysroot.environment_overrides() {
                cmd.env(key, val);
            }
        }

        // Hermetic flags: prevent picking up global site-packages
        cmd.env("PYTHONNOUSERSITE", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");

        let output = cmd.output().await.map_err(BuilderError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(BuilderError::Execution {
                code: output.status.code(),
                stderr,
            });
        }

        // Locate generated .tar.gz or .zip in output_dir
        for entry in std::fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".tar.gz") || name.ends_with(".zip") {
                return Ok(path);
            }
        }

        Err(BuilderError::MissingArtifact(output_dir.to_path_buf()))
    }
}
