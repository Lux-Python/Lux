use std::collections::HashMap;
use std::str::FromStr;
use lux_core::types::{Requirement, Version};

use super::error::ResolverError;

/// Trait abstracting package metadata discovery from local caches or remote indexes.
pub trait DependencyProvider {
    /// Return all available candidate versions for a given package name, ordered ascending.
    fn get_candidates(&self, package: &str) -> Result<Vec<Version>, ResolverError>;

    /// Return the list of direct requirements for a given package version.
    fn get_dependencies(&self, package: &str, version: &Version) -> Result<Vec<Requirement>, ResolverError>;
}

/// In-memory mock dependency provider designed for deterministic unit tests and SAT benchmarks.
#[derive(Default, Clone, Debug)]
pub struct MemoryDependencyProvider {
    candidates: HashMap<String, Vec<Version>>,
    dependencies: HashMap<(String, String), Vec<Requirement>>,
}

impl MemoryDependencyProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register available versions for a package.
    pub fn add_package(&mut self, name: &str, versions: &[&str]) {
        let normalized = lux_core::types::normalize_name(name);
        let mut parsed_versions: Vec<Version> = versions
            .iter()
            .filter_map(|s| Version::from_str(s).ok())
            .collect();
        parsed_versions.sort();
        self.candidates.insert(normalized, parsed_versions);
    }

    /// Register a dependency specification for a package version.
    pub fn add_dependency(&mut self, name: &str, version: &str, req: &str) {
        let normalized = lux_core::types::normalize_name(name);
        let key = (normalized, version.to_string());
        let requirement = Requirement::from_str(req).expect("valid test requirement");
        self.dependencies.entry(key).or_default().push(requirement);
    }
}

impl DependencyProvider for MemoryDependencyProvider {
    fn get_candidates(&self, package: &str) -> Result<Vec<Version>, ResolverError> {
        let normalized = lux_core::types::normalize_name(package);
        self.candidates
            .get(&normalized)
            .cloned()
            .ok_or_else(|| ResolverError::PackageNotFound {
                package: package.to_string(),
            })
    }

    fn get_dependencies(&self, package: &str, version: &Version) -> Result<Vec<Requirement>, ResolverError> {
        let normalized = lux_core::types::normalize_name(package);
        let key = (normalized, version.to_string());
        Ok(self.dependencies.get(&key).cloned().unwrap_or_default())
    }
}
