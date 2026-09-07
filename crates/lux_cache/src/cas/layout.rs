use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::error::CacheError;

/// Deterministic directory layout for the Content-Addressable Storage (CAS) engine.
#[derive(Debug, Clone)]
pub struct CasLayout {
    root: PathBuf,
    blobs_dir: PathBuf,
    extracted_dir: PathBuf,
    tmp_dir: PathBuf,
}

impl CasLayout {
    /// Construct a new CAS layout under the specified root directory.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        let store_v1 = root.join("store").join("v1");
        let blobs_dir = store_v1.join("blobs").join("sha256");
        let extracted_dir = store_v1.join("extracted").join("sha256");
        let tmp_dir = store_v1.join("tmp");

        Self {
            root,
            blobs_dir,
            extracted_dir,
            tmp_dir,
        }
    }

    /// Derive the default Lux cache root directory based on environment variables or home directory.
    #[must_use]
    pub fn default_root() -> PathBuf {
        if let Ok(val) = env::var("LUX_CACHE_DIR") {
            if !val.trim().is_empty() {
                return PathBuf::from(val);
            }
        }

        env::var("USERPROFILE")
            .or_else(|_| env::var("HOME"))
            .map_or_else(
                |_| PathBuf::from(".lux").join("cache"),
                |home| PathBuf::from(home).join(".lux").join("cache"),
            )
    }

    /// Initialize all prerequisite cache directories on disk.
    pub fn init_dirs(&self) -> Result<(), CacheError> {
        for dir in [&self.blobs_dir, &self.extracted_dir, &self.tmp_dir] {
            fs::create_dir_all(dir).map_err(|e| CacheError::Io {
                path: dir.display().to_string(),
                source: e,
            })?;
        }
        Ok(())
    }

    /// Compute the deterministic path for a raw blob stored by SHA-256 hash.
    ///
    /// Formats as: `store/v1/blobs/sha256/[first-2-hex]/[rest-of-hash]`
    #[must_use]
    pub fn blob_path(&self, sha256_hex: &str) -> PathBuf {
        let lower = sha256_hex.to_ascii_lowercase();
        let (prefix, rest) = if lower.len() >= 2 {
            (&lower[0..2], &lower[2..])
        } else {
            ("00", lower.as_str())
        };

        self.blobs_dir.join(prefix).join(rest)
    }

    /// Compute the deterministic path for an unpacked package tree stored by SHA-256 hash.
    ///
    /// Formats as: `store/v1/extracted/sha256/[first-2-hex]/[rest-of-hash]`
    #[must_use]
    pub fn extracted_path(&self, sha256_hex: &str) -> PathBuf {
        let lower = sha256_hex.to_ascii_lowercase();
        let (prefix, rest) = if lower.len() >= 2 {
            (&lower[0..2], &lower[2..])
        } else {
            ("00", lower.as_str())
        };

        self.extracted_dir.join(prefix).join(rest)
    }

    /// Base blobs directory where CAS artifacts are stored.
    #[must_use]
    pub fn blobs_dir(&self) -> &Path {
        &self.blobs_dir
    }

    /// Base extracted directory where unpacked distributions reside.
    #[must_use]
    pub fn extracted_dir(&self) -> &Path {
        &self.extracted_dir
    }

    /// Temporary staging directory for atomic writes and unpack operations.
    #[must_use]
    pub fn tmp_dir(&self) -> &Path {
        &self.tmp_dir
    }

    /// Base root directory of the cache.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}