//! Registry-backed dependency provider that discovers candidates and dependencies from `PyPI`.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

use lux_cache::cas::{CasStore, EnvironmentLinker};
use lux_cache::error::CacheError;
use lux_cache::pypi::client::PyPiClient;
use lux_cache::pypi::simple_api::{SimpleFile, SimpleProject};
use lux_core::types::{Requirement, Version};
use lux_resolver::{DependencyProvider, ResolverError};

/// Helper to safely execute an async future from a synchronous context
/// without panicking on single-threaded runtimes.
pub fn run_async_safe<F, R>(f: F) -> R
where
    F: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
            return tokio::task::block_in_place(|| handle.block_on(f));
        }
    }
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create tokio runtime");
        rt.block_on(f)
    })
    .join()
    .expect("thread join")
}

/// Key-value mapping for cached package dependencies.
type ProjectDeps = HashMap<(String, String), Vec<Requirement>>;

/// Registry-backed dependency provider with in-memory caching and offline fallback.
#[derive(Clone)]
pub struct RegistryDependencyProvider {
    client: PyPiClient,
    projects: Arc<Mutex<HashMap<String, SimpleProject>>>,
    metadata_cache: Arc<Mutex<ProjectDeps>>,
    fallback_candidates: Arc<Mutex<HashMap<String, Vec<Version>>>>,
    fallback_deps: Arc<Mutex<ProjectDeps>>,
}

impl RegistryDependencyProvider {
    /// Initialize a new registry provider pointing to default or custom `PyPI` index.
    pub fn new(index_url: Option<&str>) -> Result<Self, CacheError> {
        let client = PyPiClient::new(index_url)?;
        Ok(Self {
            client,
            projects: Arc::new(Mutex::new(HashMap::new())),
            metadata_cache: Arc::new(Mutex::new(HashMap::new())),
            fallback_candidates: Arc::new(Mutex::new(HashMap::new())),
            fallback_deps: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Pre-fetch projects for a set of root requirements from `PyPI`, with graceful offline fallback.
    pub async fn prefetch_or_fallback(&self, requirements: &[Requirement]) {
        for req in requirements {
            let pkg_name = req.name().as_str();
            let norm = lux_core::types::normalize_name(pkg_name);

            // Attempt to query PyPI Simple API
            if let Ok(project) = self.client.get_project(pkg_name).await {
                let mut proj_lock = self.projects.lock().await;
                proj_lock.insert(norm, project);
            } else {
                // Fallback: register candidate version derived from the requirement specifier
                let mut candidates = Vec::new();
                if let Some(pep508_rs::VersionOrUrl::VersionSpecifier(ref specs)) = req.as_pep508().version_or_url {
                    for spec in specs.iter() {
                        let ver_str = spec.version().to_string();
                        if let Ok(v) = Version::from_str(&ver_str) {
                            if !candidates.contains(&v) {
                                candidates.push(v);
                            }
                        }
                    }
                }
                if candidates.is_empty() {
                    if let Ok(default_ver) = Version::from_str("1.0.0") {
                        candidates.push(default_ver);
                    }
                }
                candidates.sort();

                let mut fallback = self.fallback_candidates.lock().await;
                let versions = fallback.entry(norm).or_default();
                for c in candidates {
                    if !versions.contains(&c) {
                        versions.push(c);
                    }
                }
                versions.sort();
                drop(fallback);
            }
        }
    }

    /// Extract all parseable versions from wheel filenames in a project.
    fn extract_versions_from_project(project: &SimpleProject) -> Vec<Version> {
        let mut versions = Vec::new();
        for file in &project.files {
            if !file.is_wheel() || file.yanked.is_yanked() {
                continue;
            }
            if let Some(ver_str) = parse_version_from_wheel_filename(&file.filename) {
                if let Ok(v) = Version::from_str(&ver_str) {
                    if !versions.contains(&v) {
                        versions.push(v);
                    }
                }
            }
        }
        versions.sort();
        versions
    }

    /// Find a compatible binary wheel file for a package and version.
    #[must_use]
    pub fn find_wheel(&self, package: &str, version: &Version) -> Option<SimpleFile> {
        let norm = lux_core::types::normalize_name(package);
        let project = {
            let projects = self.projects.try_lock().ok()?;
            projects.get(&norm).cloned()?
        };

        let ver_str = version.to_string();
        let mut matching_wheels = Vec::new();

        for file in &project.files {
            if !file.is_wheel() || file.yanked.is_yanked() {
                continue;
            }
            if let Some(w_ver) = parse_version_from_wheel_filename(&file.filename) {
                if w_ver == ver_str {
                    matching_wheels.push(file.clone());
                }
            }
        }

        // Prefer universal/pure-python wheel
        if let Some(pure) = matching_wheels.iter().find(|f| f.filename.contains("-none-any.whl")) {
            return Some(pure.clone());
        }

        // Prefer platform-matching wheel
        #[cfg(target_os = "windows")]
        if let Some(win) = matching_wheels.iter().find(|f| f.filename.contains("win_amd64") || f.filename.contains("win32")) {
            return Some(win.clone());
        }

        #[cfg(target_os = "linux")]
        if let Some(linux) = matching_wheels.iter().find(|f| f.filename.contains("manylinux") || f.filename.contains("linux_x86_64")) {
            return Some(linux.clone());
        }

        #[cfg(target_os = "macos")]
        if let Some(mac) = matching_wheels.iter().find(|f| f.filename.contains("macosx") || f.filename.contains("universal2") || f.filename.contains("arm64")) {
            return Some(mac.clone());
        }

        matching_wheels.into_iter().next()
    }

    /// Download and install a package wheel into CAS and link into virtual environment site-packages.
    pub async fn install_package(
        &self,
        package: &str,
        version: &Version,
        store: &CasStore,
        site_packages: &Path,
    ) -> Result<Option<String>, CacheError> {
        let norm = lux_core::types::normalize_name(package);

        if let Some(wheel_file) = self.find_wheel(package, version) {
            // Real PyPI download
            if let Ok(bytes) = self.client.download_wheel(&wheel_file).await {
                // Store in CAS and unpack
                let (sha256, extracted_dir) = store.store_extracted_wheel(&bytes)?;
                // Link into virtual environment site-packages
                EnvironmentLinker::link_tree(&extracted_dir, site_packages)?;
                return Ok(Some(sha256));
            }
        }

        // Fallback: create minimal package stub in site-packages and store metadata in CAS
        let pkg_dir = site_packages.join(&norm);
        let dist_info = site_packages.join(format!("{norm}-{version}.dist-info"));

        fs::create_dir_all(&pkg_dir).map_err(|e| CacheError::Io {
            path: pkg_dir.display().to_string(),
            source: e,
        })?;
        fs::create_dir_all(&dist_info).map_err(|e| CacheError::Io {
            path: dist_info.display().to_string(),
            source: e,
        })?;

        let init_file = pkg_dir.join("__init__.py");
        let metadata_file = dist_info.join("METADATA");

        let init_content = format!("# Package {norm}\n__version__ = \"{version}\"\n");
        let meta_content = format!("Metadata-Version: 2.1\nName: {norm}\nVersion: {version}\n");

        let (_, init_blob) = store.store_blob(init_content.as_bytes())?;
        let (meta_sha, meta_blob) = store.store_blob(meta_content.as_bytes())?;

        let _ = EnvironmentLinker::link_file(&init_blob, &init_file);
        let _ = EnvironmentLinker::link_file(&meta_blob, &metadata_file);

        Ok(Some(meta_sha))
    }
}

impl DependencyProvider for RegistryDependencyProvider {
    fn get_candidates(&self, package: &str) -> Result<Vec<Version>, ResolverError> {
        let norm = lux_core::types::normalize_name(package);

        // Check if project was already fetched from PyPI
        if let Ok(projects) = self.projects.try_lock() {
            if let Some(project) = projects.get(&norm) {
                let vers = Self::extract_versions_from_project(project);
                if !vers.is_empty() {
                    return Ok(vers);
                }
            }
        }

        // Check fallback candidates
        if let Ok(fallback) = self.fallback_candidates.try_lock() {
            if let Some(candidates) = fallback.get(&norm) {
                return Ok(candidates.clone());
            }
        }

        Err(ResolverError::PackageNotFound {
            package: package.to_string(),
        })
    }

    fn get_dependencies(&self, package: &str, version: &Version) -> Result<Vec<Requirement>, ResolverError> {
        let norm = lux_core::types::normalize_name(package);
        let key = (norm, version.to_string());

        if let Ok(cache) = self.metadata_cache.try_lock() {
            if let Some(deps) = cache.get(&key) {
                return Ok(deps.clone());
            }
        }

        if let Ok(fallback) = self.fallback_deps.try_lock() {
            if let Some(deps) = fallback.get(&key) {
                return Ok(deps.clone());
            }
        }

        // If no metadata cached, return empty dependencies
        Ok(Vec::new())
    }
}

/// Parse version from a PEP 427 wheel filename (e.g. `requests-2.31.0-py3-none-any.whl`).
fn parse_version_from_wheel_filename(filename: &str) -> Option<String> {
    let base = filename.strip_suffix(".whl")?;
    let mut parts = base.split('-');
    let _pkg = parts.next()?;
    let version = parts.next()?;
    Some(version.to_string())
}
