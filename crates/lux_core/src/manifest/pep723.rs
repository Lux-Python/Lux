//! PEP 723: Inline script metadata parser.
//!
//! Extracts `# /// script` ... `# ///` comment blocks from standalone Python files
//! and parses their embedded TOML configuration for dependencies and Python constraints.

use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::error::LuxError;

/// PEP 723 parsed script metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScriptMetadata {
    /// Supported Python version specifier (e.g. `">=3.10"`).
    #[serde(rename = "requires-python", default)]
    pub requires_python: Option<String>,
    /// List of direct package requirements (PEP 508 strings).
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl ScriptMetadata {
    /// Parse PEP 723 inline script metadata from raw Python script source code.
    ///
    /// Returns `Ok(None)` if no `# /// script` block is found.
    pub fn parse(content: &str) -> Result<Option<Self>, LuxError> {
        let mut in_block = false;
        let mut toml_lines = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "# /// script" {
                in_block = true;
                continue;
            }

            if in_block {
                if trimmed == "# ///" {
                    break;
                }

                // Strip leading '#' and optional single space
                if let Some(stripped) = trimmed.strip_prefix('#') {
                    let cleaned = stripped.strip_prefix(' ').unwrap_or(stripped);
                    toml_lines.push(cleaned);
                }
            }
        }

        if toml_lines.is_empty() {
            return Ok(None);
        }

        let toml_str = toml_lines.join("\n");
        let metadata: Self = toml::from_str(&toml_str)
            .map_err(|e| LuxError::ManifestError {
                reason: format!("Failed to parse PEP 723 script metadata: {e}"),
            })?;

        Ok(Some(metadata))
    }

    /// Read and parse PEP 723 metadata directly from a script file on disk.
    pub fn from_file(path: &Path) -> Result<Option<Self>, LuxError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Returns list of package dependencies declared in the script.
    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// Returns the Python version requirement, if specified.
    #[must_use]
    pub fn requires_python(&self) -> Option<&str> {
        self.requires_python.as_deref()
    }
}
