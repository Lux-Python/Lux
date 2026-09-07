use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Indicator for whether dist-info metadata is available without downloading the full archive (PEP 658).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetadataInfo {
    /// Boolean flag: `true` indicates metadata is available at `${url}.metadata`.
    Available(bool),
    /// Hash mapping: dictionary of cryptographic hashes for the metadata file.
    Hashes(HashMap<String, String>),
}

impl MetadataInfo {
    /// Whether metadata is explicitly declared available.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        match self {
            Self::Available(b) => *b,
            Self::Hashes(_) => true,
        }
    }

    /// Retrieve the SHA-256 hash of the metadata file, if specified.
    #[must_use]
    pub fn sha256(&self) -> Option<&str> {
        match self {
            Self::Hashes(hashes) => hashes.get("sha256").map(String::as_str),
            Self::Available(_) => None,
        }
    }
}

/// Package file release yank status (PEP 592).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum Yanked {
    /// Package is not yanked.
    #[default]
    NotYanked,
    /// Boolean indicator.
    Bool(bool),
    /// Yanked with a documented rationale.
    Reason(String),
}

impl Yanked {
    /// Whether this file release was yanked from the index.
    #[must_use]
    pub const fn is_yanked(&self) -> bool {
        match self {
            Self::NotYanked => false,
            Self::Bool(b) => *b,
            Self::Reason(_) => true,
        }
    }

    /// Retrieve the rationale for why this file was yanked, if present.
    #[must_use]
    pub const fn reason(&self) -> Option<&str> {
        match self {
            Self::Reason(r) => Some(r.as_str()),
            Self::NotYanked | Self::Bool(_) => None,
        }
    }
}

/// Metadata header block in PEP 691 JSON response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleMeta {
    /// Version of the Simple API specification implemented by the index server (e.g., "1.0").
    #[serde(rename = "api-version")]
    pub api_version: String,
}

/// A downloadable distribution file listed in a PEP 691 project index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleFile {
    /// The archive filename (e.g. `numpy-2.2.0-cp312-cp312-win_amd64.whl`).
    pub filename: String,
    /// Absolute or relative URL to download the distribution file.
    pub url: String,
    /// Cryptographic checksums keyed by algorithm (e.g., `sha256`).
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    /// Environment marker specifying supported Python version intervals (PEP 503 / PEP 691).
    #[serde(rename = "requires-python", default)]
    pub requires_python: Option<String>,
    /// Legacy PEP 658 metadata field.
    #[serde(rename = "dist-info-metadata", default)]
    pub dist_info_metadata: Option<MetadataInfo>,
    /// Updated PEP 714 core metadata field.
    #[serde(rename = "core-metadata", default)]
    pub core_metadata: Option<MetadataInfo>,
    /// Whether the file was yanked.
    #[serde(default)]
    pub yanked: Yanked,
}

impl SimpleFile {
    /// Whether this distribution artifact is a binary Python wheel (`.whl`).
    #[must_use]
    pub fn is_wheel(&self) -> bool {
        std::path::Path::new(&self.filename)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
    }

    /// Retrieve the expected SHA-256 digest of the archive file, if provided.
    #[must_use]
    pub fn sha256(&self) -> Option<&str> {
        self.hashes.get("sha256").map(String::as_str)
    }

    /// Check if remote `dist-info/METADATA` can be fetched directly without downloading the wheel (PEP 658).
    #[must_use]
    pub const fn has_remote_metadata(&self) -> bool {
        if let Some(ref meta) = self.core_metadata {
            if meta.is_available() {
                return true;
            }
        }
        if let Some(ref meta) = self.dist_info_metadata {
            if meta.is_available() {
                return true;
            }
        }
        false
    }

    /// Retrieve the expected SHA-256 digest of the `.metadata` file if provided.
    #[must_use]
    pub fn metadata_sha256(&self) -> Option<&str> {
        if let Some(ref meta) = self.core_metadata {
            if let Some(hash) = meta.sha256() {
                return Some(hash);
            }
        }
        if let Some(ref meta) = self.dist_info_metadata {
            if let Some(hash) = meta.sha256() {
                return Some(hash);
            }
        }
        None
    }
}

/// A project index response adhering to the PEP 691 Simple Repository API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleProject {
    /// Specification metadata.
    pub meta: SimpleMeta,
    /// Canonical package name.
    pub name: String,
    /// List of distribution files available for this package.
    #[serde(default)]
    pub files: Vec<SimpleFile>,
}

impl SimpleProject {
    /// Filter and return only active, non-yanked binary wheel files.
    #[must_use]
    pub fn active_wheels(&self) -> Vec<&SimpleFile> {
        self.files
            .iter()
            .filter(|f| f.is_wheel() && !f.yanked.is_yanked())
            .collect()
    }
}