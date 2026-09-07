pub mod lockfile;
pub mod pep723;

pub use pep723::ScriptMetadata;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use serde::{Deserialize, Serialize};

use super::error::LuxError;
use super::types::Requirement;

/// Python monorepo workspace specification under `[tool.lux.workspace]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LuxWorkspaceSection {
    /// Workspace members (e.g. `["packages/*", "services/api"]`).
    #[serde(default)]
    pub members: Vec<String>,
    /// Excluded paths or patterns.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Tool configuration table for Lux under `[tool.lux]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LuxToolSection {
    /// Workspace configuration (`[tool.lux.workspace]`).
    pub workspace: Option<LuxWorkspaceSection>,
}

/// Root tool table (`[tool]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolSection {
    /// Lux configuration table (`[tool.lux]`).
    pub lux: Option<LuxToolSection>,
}

/// PEP 517 / PEP 518 build system definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BuildSystem {
    /// Build requirements (e.g. `["flit_core >=3.2,<4"]`).
    #[serde(default)]
    pub requires: Vec<String>,
    /// Build backend entry point (e.g. `"flit_core.buildapi"`).
    #[serde(rename = "build-backend", default)]
    pub build_backend: Option<String>,
}

/// PEP 621 project metadata table (`[project]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectSection {
    /// Canonical distribution name of the package.
    pub name: String,
    /// Project version (PEP 440).
    #[serde(default)]
    pub version: Option<String>,
    /// Short description of the project.
    #[serde(default)]
    pub description: Option<String>,
    /// Supported Python version specifiers (e.g. `">=3.10"`).
    #[serde(rename = "requires-python", default)]
    pub requires_python: Option<String>,
    /// Direct runtime dependencies (PEP 508 strings).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Optional / extra dependencies (e.g. `test = ["pytest"]`).
    #[serde(rename = "optional-dependencies", default)]
    pub optional_dependencies: BTreeMap<String, Vec<String>>,
}

/// Standardized `pyproject.toml` manifest conforming to PEP 517, PEP 518, PEP 621, PEP 735, and Lux workspaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PyProjectToml {
    /// Optional build system specifications.
    #[serde(rename = "build-system", default)]
    pub build_system: Option<BuildSystem>,
    /// Core PEP 621 project definition.
    #[serde(default)]
    pub project: Option<ProjectSection>,
    /// PEP 735 dependency groups (`[dependency-groups]`).
    #[serde(rename = "dependency-groups", default)]
    pub dependency_groups: BTreeMap<String, Vec<String>>,
    /// Tool configuration table (`[tool]`).
    #[serde(default)]
    pub tool: Option<ToolSection>,
}

impl PyProjectToml {
    /// Parse a `pyproject.toml` manifest from raw TOML string content.
    pub fn parse(content: &str) -> Result<Self, LuxError> {
        toml::from_str(content).map_err(LuxError::from)
    }

    /// Read and parse a `pyproject.toml` file from the specified path.
    pub fn from_file(path: &Path) -> Result<Self, LuxError> {
        if !path.is_file() {
            return Err(LuxError::ManifestNotFound {
                path: path.display().to_string(),
            });
        }
        let content = fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Serialize and atomically persist the manifest back to disk.
    pub fn save_to_file(&self, path: &Path) -> Result<(), LuxError> {
        let serialized = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serialized)?;
        Ok(())
    }

    /// Construct a default, idiomatic `pyproject.toml` for a newly initialized project.
    #[must_use]
    pub fn init_default(name: &str) -> Self {
        let project = ProjectSection {
            name: name.to_string(),
            version: Some("0.1.0".to_string()),
            description: Some(format!("Python package '{name}' powered by Lux")),
            requires_python: Some(">=3.10".to_string()),
            dependencies: Vec::new(),
            optional_dependencies: BTreeMap::new(),
        };

        let build_system = BuildSystem {
            requires: vec!["flit_core >=3.2,<4".to_string()],
            build_backend: Some("flit_core.buildapi".to_string()),
        };

        Self {
            build_system: Some(build_system),
            project: Some(project),
            dependency_groups: BTreeMap::new(),
            tool: None,
        }
    }

    /// Package name declared in `[project]`. Defaults to `"unnamed"`.
    #[must_use]
    pub fn name(&self) -> &str {
        self.project.as_ref().map_or("unnamed", |p| &p.name)
    }

    /// Package version declared in `[project]`.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.project.as_ref().and_then(|p| p.version.as_deref())
    }

    /// Slice of declared direct dependencies.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        self.project.as_ref().map_or(&[], |p| &p.dependencies)
    }

    /// Add a dependency string (e.g. `"requests>=2.31.0"`) to `[project.dependencies]`.
    /// Replaces an existing requirement for the same normalized package name if present.
    pub fn add_dependency(&mut self, raw_dep: &str) -> Result<(), LuxError> {
        let new_req = Requirement::from_str(raw_dep).map_err(|e| LuxError::PackageParseError {
            reason: format!("failed to parse '{raw_dep}': {e}"),
        })?;

        let proj = self.project.get_or_insert_with(ProjectSection::default);

        // Remove existing requirement for the same package if already listed
        proj.dependencies.retain(|existing| {
            Requirement::from_str(existing)
                .map_or(true, |existing_req| existing_req.name() != new_req.name())
        });

        proj.dependencies.push(raw_dep.to_string());
        proj.dependencies.sort();
        Ok(())
    }

    /// Remove a dependency by package name from `[project.dependencies]`.
    /// Returns `true` if a dependency was removed.
    pub fn remove_dependency(&mut self, pkg_name: &str) -> bool {
        let Some(proj) = self.project.as_mut() else {
            return false;
        };

        let initial_len = proj.dependencies.len();
        let normalized = crate::types::normalize_name(pkg_name);

        proj.dependencies.retain(|existing| {
            Requirement::from_str(existing).map_or_else(
                |_| !existing.eq_ignore_ascii_case(pkg_name),
                |req| crate::types::normalize_name(req.name().as_str()) != normalized,
            )
        });

        proj.dependencies.len() < initial_len
    }

    /// Parse all declared `[project.dependencies]` into typed `Requirement` structs.
    pub fn parsed_requirements(&self) -> Result<Vec<Requirement>, LuxError> {
        let mut reqs = Vec::new();
        for dep in self.dependencies() {
            let req = Requirement::from_str(dep).map_err(|e| LuxError::PackageParseError {
                reason: format!("invalid requirement '{dep}': {e}"),
            })?;
            reqs.push(req);
        }
        Ok(reqs)
    }

    /// Check if this project is declared as a monorepo workspace root.
    #[must_use]
    pub fn is_workspace(&self) -> bool {
        self.tool
            .as_ref()
            .and_then(|t| t.lux.as_ref())
            .and_then(|l| l.workspace.as_ref())
            .is_some()
    }

    /// Retrieve declared workspace member patterns if present.
    #[must_use]
    pub fn workspace_members(&self) -> Option<&[String]> {
        self.tool
            .as_ref()
            .and_then(|t| t.lux.as_ref())
            .and_then(|l| l.workspace.as_ref())
            .map(|w| w.members.as_slice())
    }

    /// Discover and parse all member projects in the workspace given the workspace root directory.
    pub fn discover_workspace_members(
        &self,
        workspace_root: &Path,
    ) -> Result<Vec<(PathBuf, Self)>, LuxError> {
        let Some(members) = self.workspace_members() else {
            return Ok(Vec::new());
        };

        let mut discovered = Vec::new();
        for pattern in members {
            if pattern.ends_with("/*") || pattern.ends_with("\\*") {
                let base_rel = &pattern[..pattern.len() - 2];
                let base_dir = workspace_root.join(base_rel);
                if base_dir.is_dir() {
                    if let Ok(entries) = fs::read_dir(&base_dir) {
                        for entry in entries.flatten() {
                            let member_dir = entry.path();
                            let member_toml = member_dir.join("pyproject.toml");
                            if member_toml.is_file() {
                                if let Ok(manifest) = Self::from_file(&member_toml) {
                                    discovered.push((member_dir, manifest));
                                }
                            }
                        }
                    }
                }
            } else {
                let member_dir = workspace_root.join(pattern);
                let member_toml = member_dir.join("pyproject.toml");
                if member_toml.is_file() {
                    if let Ok(manifest) = Self::from_file(&member_toml) {
                        discovered.push((member_dir, manifest));
                    }
                }
            }
        }
        discovered.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(discovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pep621_pyproject() {
        let toml_content = r#"
        [build-system]
        requires = ["setuptools>=61.0"]
        build-backend = "setuptools.build_meta"

        [project]
        name = "my-awesome-app"
        version = "1.2.3"
        description = "A fast app"
        requires-python = ">=3.11"
        dependencies = [
            "requests>=2.28.0",
            "pydantic>=2.0",
        ]

        [dependency-groups]
        dev = ["pytest>=7.0", "ruff>=0.1.0"]
        "#;

        let manifest = PyProjectToml::parse(toml_content).expect("parsed pyproject");
        assert_eq!(manifest.name(), "my-awesome-app");
        assert_eq!(manifest.version(), Some("1.2.3"));
        assert_eq!(manifest.dependencies().len(), 2);

        let dev_group = manifest.dependency_groups.get("dev").expect("dev group");
        assert_eq!(dev_group.len(), 2);

        let reqs = manifest.parsed_requirements().expect("parsed requirements");
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].name().as_str(), "requests");
    }

    #[test]
    fn test_add_and_remove_dependencies() {
        let mut manifest = PyProjectToml::init_default("sample-pkg");
        assert!(manifest.dependencies().is_empty());

        manifest.add_dependency("urllib3>=2.0.0").expect("added dep");
        assert_eq!(manifest.dependencies(), &["urllib3>=2.0.0".to_string()]);

        // Overwrite existing dependency
        manifest.add_dependency("urllib3>=2.1.0").expect("overwrote dep");
        assert_eq!(manifest.dependencies(), &["urllib3>=2.1.0".to_string()]);

        manifest.add_dependency("certifi>=2024.2.2").expect("added certifi");
        assert_eq!(manifest.dependencies().len(), 2);

        let removed = manifest.remove_dependency("urllib3");
        assert!(removed);
        assert_eq!(manifest.dependencies().len(), 1);
        assert_eq!(manifest.dependencies()[0], "certifi>=2024.2.2");

        let not_found = manifest.remove_dependency("nonexistent");
        assert!(!not_found);
    }

    #[test]
    fn test_workspace_manifest_parsing() {
        let toml_content = r#"
        [project]
        name = "root-workspace"
        version = "0.1.0"

        [tool.lux.workspace]
        members = ["packages/*", "tools/cli"]
        exclude = ["packages/ignored"]
        "#;

        let manifest = PyProjectToml::parse(toml_content).expect("parsed workspace pyproject");
        assert!(manifest.is_workspace());
        let members = manifest.workspace_members().expect("members");
        assert_eq!(members, &["packages/*", "tools/cli"]);
    }
}
