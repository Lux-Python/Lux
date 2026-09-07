use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use ring::digest::{digest, SHA256};
use tempfile::Builder;

use super::super::archive::wheel::extract_wheel;
use super::super::error::CacheError;
use super::layout::CasLayout;

/// Content-addressable blob storage with atomic writes.
#[derive(Debug, Clone)]
pub struct CasStore {
    layout: CasLayout,
}

impl CasStore {
    /// Initialize or open the CAS store at a specified or default root directory.
    pub fn open(custom_root: Option<PathBuf>) -> Result<Self, CacheError> {
        let root = custom_root.unwrap_or_else(CasLayout::default_root);
        let layout = CasLayout::new(root);
        layout.init_dirs()?;
        Ok(Self { layout })
    }

    /// Access the underlying directory layout.
    #[must_use]
    pub const fn layout(&self) -> &CasLayout {
        &self.layout
    }

    /// Compute the lowercase hex SHA-256 digest of a byte slice.
    #[must_use]
    pub fn compute_sha256(data: &[u8]) -> String {
        let hash = digest(&SHA256, data);
        hash.as_ref().iter().fold(String::with_capacity(64), |mut acc, b| {
            use std::fmt::Write;
            let _ = write!(acc, "{b:02x}");
            acc
        })
    }

    /// Check if a raw blob is already committed in the CAS store.
    #[must_use]
    pub fn contains_blob(&self, sha256_hex: &str) -> bool {
        self.layout.blob_path(sha256_hex).is_file()
    }

    /// Check if an extracted package directory is already committed in the CAS store.
    #[must_use]
    pub fn contains_extracted(&self, sha256_hex: &str) -> bool {
        self.layout.extracted_path(sha256_hex).is_dir()
    }

    /// Retrieve the deterministic path to a blob by its SHA-256 digest.
    #[must_use]
    pub fn blob_path(&self, sha256_hex: &str) -> PathBuf {
        self.layout.blob_path(sha256_hex)
    }

    /// Retrieve the deterministic path to an extracted package tree by its SHA-256 digest.
    #[must_use]
    pub fn extracted_path(&self, sha256_hex: &str) -> PathBuf {
        self.layout.extracted_path(sha256_hex)
    }

    /// Store a raw blob into the CAS store, computing its SHA-256 digest on the fly.
    /// Returns `(sha256_hex, target_file_path)`.
    pub fn store_blob(&self, bytes: &[u8]) -> Result<(String, PathBuf), CacheError> {
        let sha256_hex = Self::compute_sha256(bytes);
        let target_path = self.layout.blob_path(&sha256_hex);

        if target_path.is_file() {
            return Ok((sha256_hex, target_path));
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| CacheError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        // Atomically write to a temp file in layout.tmp_dir()
        let mut temp_file = Builder::new()
            .prefix(".blob-")
            .tempfile_in(self.layout.tmp_dir())
            .map_err(|e| CacheError::Io {
                path: self.layout.tmp_dir().display().to_string(),
                source: e,
            })?;

        temp_file.write_all(bytes).map_err(|e| CacheError::Io {
            path: "temp blob".to_string(),
            source: e,
        })?;

        temp_file.flush().map_err(|e| CacheError::Io {
            path: "temp blob flush".to_string(),
            source: e,
        })?;

        // Commit atomically via rename. If target was created concurrently, ignore error.
        match temp_file.persist(&target_path) {
            Ok(_) => Ok((sha256_hex, target_path)),
            Err(_) if target_path.is_file() => Ok((sha256_hex, target_path)),
            Err(e) => Err(CacheError::CasCommitFailed {
                path: target_path.display().to_string(),
                reason: e.to_string(),
            }),
        }
    }

    /// Store a raw blob with validation against an expected SHA-256 digest.
    pub fn store_blob_with_hash(&self, bytes: &[u8], expected_sha256: &str) -> Result<PathBuf, CacheError> {
        let computed = Self::compute_sha256(bytes);
        if !computed.eq_ignore_ascii_case(expected_sha256) {
            return Err(CacheError::ChecksumMismatch {
                file: "raw blob".to_string(),
                expected: expected_sha256.to_string(),
                computed,
            });
        }

        let (_, path) = self.store_blob(bytes)?;
        Ok(path)
    }

    /// Read an existing blob from the store into memory.
    pub fn get_blob(&self, sha256_hex: &str) -> Result<Vec<u8>, CacheError> {
        let path = self.layout.blob_path(sha256_hex);
        let mut file = File::open(&path).map_err(|e| CacheError::Io {
            path: path.display().to_string(),
            source: e,
        })?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| CacheError::Io {
            path: path.display().to_string(),
            source: e,
        })?;

        Ok(buf)
    }

    /// Decompress and unpack a binary Python wheel into an extracted CAS directory tree.
    ///
    /// Writes atomically to an isolated staging directory before committing to the pool.
    pub fn store_extracted_wheel(&self, wheel_bytes: &[u8]) -> Result<(String, PathBuf), CacheError> {
        let sha256_hex = Self::compute_sha256(wheel_bytes);
        let target_dir = self.layout.extracted_path(&sha256_hex);

        if target_dir.is_dir() {
            return Ok((sha256_hex, target_dir));
        }

        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent).map_err(|e| CacheError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        // Staging directory
        let staging_temp_dir = Builder::new()
            .prefix(".wheel-extract-")
            .tempdir_in(self.layout.tmp_dir())
            .map_err(|e| CacheError::Io {
                path: self.layout.tmp_dir().display().to_string(),
                source: e,
            })?;

        extract_wheel(wheel_bytes, staging_temp_dir.path())?;

        // Atomically rename staging directory to final target directory
        match fs::rename(staging_temp_dir.path(), &target_dir) {
            Ok(()) => Ok((sha256_hex, target_dir)),
            Err(_) if target_dir.is_dir() => {
                // Another process finished concurrently; safe to clean up temp
                let _ = fs::remove_dir_all(staging_temp_dir.path());
                Ok((sha256_hex, target_dir))
            }
            Err(e) => Err(CacheError::CasCommitFailed {
                path: target_dir.display().to_string(),
                reason: e.to_string(),
            }),
        }
    }
}