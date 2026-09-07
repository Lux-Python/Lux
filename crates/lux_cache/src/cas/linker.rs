use std::fs;
use std::path::Path;

use super::super::error::CacheError;

/// Strategy employed when establishing a file link into an environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMethod {
    /// Zero-cost atomic filesystem hardlink (`std::fs::hard_link`).
    HardLink,
    /// Fast file copy fallback when crossing disk volume boundaries (`EXDEV`).
    Copy,
}

/// Diagnostic report detailing the linking throughput and methods used.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinkReport {
    /// Number of files successfully hardlinked.
    pub files_hardlinked: usize,
    /// Number of files copied (due to volume boundary or filesystem limitations).
    pub files_copied: usize,
    /// Total bytes referenced.
    pub total_bytes: u64,
}

impl LinkReport {
    /// Total number of files installed into the target environment.
    #[must_use]
    pub const fn total_files(&self) -> usize {
        self.files_hardlinked + self.files_copied
    }
}

/// Filesystem-aware environment linker for projecting CAS artifacts into virtual environments.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentLinker;

impl EnvironmentLinker {
    /// Link an individual file from source to target, preferring zero-copy hardlinking
    /// and falling back to copying if cross-device boundaries or permissions prevent it.
    pub fn link_file(source: &Path, target: &Path) -> Result<LinkMethod, CacheError> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| CacheError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        // Remove existing target file if present so hard_link or copy succeeds
        if target.is_file() {
            let _ = fs::remove_file(target);
        }

        // Attempt zero-cost hardlink first
        if fs::hard_link(source, target).is_ok() {
            Ok(LinkMethod::HardLink)
        } else {
            // Fallback to copy (handles EXDEV, non-NTFS volumes, or hardlink quota limits)
            fs::copy(source, target).map_err(|e| CacheError::LinkFailed {
                source_path: source.display().to_string(),
                target_path: target.display().to_string(),
                reason: e.to_string(),
            })?;
            Ok(LinkMethod::Copy)
        }
    }

    /// Recursively project an extracted CAS package tree into a target environment directory (e.g. `site-packages`).
    pub fn link_tree(cas_extracted_dir: &Path, target_dir: &Path) -> Result<LinkReport, CacheError> {
        if !cas_extracted_dir.is_dir() {
            return Err(CacheError::CasCommitFailed {
                path: cas_extracted_dir.display().to_string(),
                reason: "source extracted CAS path is not a directory".to_string(),
            });
        }

        fs::create_dir_all(target_dir).map_err(|e| CacheError::Io {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let mut report = LinkReport::default();
        Self::traverse_and_link(cas_extracted_dir, cas_extracted_dir, target_dir, &mut report)?;

        Ok(report)
    }

    fn traverse_and_link(
        root: &Path,
        current: &Path,
        target_root: &Path,
        report: &mut LinkReport,
    ) -> Result<(), CacheError> {
        let entries = fs::read_dir(current).map_err(|e| CacheError::Io {
            path: current.display().to_string(),
            source: e,
        })?;

        for entry_res in entries {
            let entry = entry_res.map_err(|e| CacheError::Io {
                path: current.display().to_string(),
                source: e,
            })?;

            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| CacheError::Io {
                path: path.display().to_string(),
                source: e,
            })?;

            if file_type.is_dir() {
                Self::traverse_and_link(root, &path, target_root, report)?;
            } else if file_type.is_file() {
                let relative = path.strip_prefix(root).map_err(|e| CacheError::LinkFailed {
                    source_path: path.display().to_string(),
                    target_path: target_root.display().to_string(),
                    reason: e.to_string(),
                })?;

                let dest = target_root.join(relative);
                let metadata = entry.metadata().map_err(|e| CacheError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;

                report.total_bytes += metadata.len();

                match Self::link_file(&path, &dest)? {
                    LinkMethod::HardLink => report.files_hardlinked += 1,
                    LinkMethod::Copy => report.files_copied += 1,
                }
            }
        }

        Ok(())
    }
}