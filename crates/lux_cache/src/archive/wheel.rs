use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::super::error::CacheError;
use super::zip_range::{decompress_entry, parse_central_directory, parse_local_header_data_offset, EocdRecord};

/// Sanitize a zip entry path to prevent directory traversal attacks (zip slips).
fn sanitize_zip_path(entry_path: &str) -> Result<PathBuf, CacheError> {
    let path = Path::new(entry_path);
    let mut clean_path = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::Normal(part) => clean_path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CacheError::InvalidZip {
                    reason: format!("unsafe path traversal component in entry '{entry_path}'"),
                });
            }
        }
    }

    if clean_path.as_os_str().is_empty() {
        return Err(CacheError::InvalidZip {
            reason: format!("empty path component for entry '{entry_path}'"),
        });
    }

    Ok(clean_path)
}

/// Unpack a binary Python wheel from memory into a target directory.
/// Returns the list of extracted relative file paths.
pub fn extract_wheel(archive_bytes: &[u8], destination: &Path) -> Result<Vec<PathBuf>, CacheError> {
    let eocd = EocdRecord::parse(archive_bytes)?;
    let cd_start = usize::try_from(eocd.cd_offset).map_err(|_| CacheError::InvalidZip {
        reason: "central directory offset exceeds address space".to_string(),
    })?;
    let cd_end = cd_start + usize::try_from(eocd.cd_size).map_err(|_| CacheError::InvalidZip {
        reason: "central directory size exceeds address space".to_string(),
    })?;

    if cd_end > archive_bytes.len() {
        return Err(CacheError::InvalidZip {
            reason: "central directory bounds exceed archive byte length".to_string(),
        });
    }

    let entries = parse_central_directory(&archive_bytes[cd_start..cd_end])?;
    let mut extracted_files = Vec::with_capacity(entries.len());

    fs::create_dir_all(destination).map_err(|e| CacheError::Io {
        path: destination.display().to_string(),
        source: e,
    })?;

    for entry in &entries {
        if entry.is_dir() {
            continue;
        }

        let relative_path = sanitize_zip_path(&entry.filename)?;
        let target_file_path = destination.join(&relative_path);

        if let Some(parent) = target_file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| CacheError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        let local_offset = usize::try_from(entry.local_header_offset).map_err(|_| CacheError::InvalidZip {
            reason: "local header offset exceeds address space".to_string(),
        })?;

        if local_offset + 30 > archive_bytes.len() {
            return Err(CacheError::InvalidZip {
                reason: format!("truncated local header for entry '{}'", entry.filename),
            });
        }

        let data_rel_offset = usize::try_from(parse_local_header_data_offset(&archive_bytes[local_offset..])?)
            .map_err(|_| CacheError::InvalidZip {
                reason: "data offset exceeds address space".to_string(),
            })?;

        let data_start = local_offset + data_rel_offset;
        let data_end = data_start + usize::try_from(entry.compressed_size).map_err(|_| CacheError::InvalidZip {
            reason: "compressed size exceeds address space".to_string(),
        })?;

        if data_end > archive_bytes.len() {
            return Err(CacheError::InvalidZip {
                reason: format!("compressed payload truncated for entry '{}'", entry.filename),
            });
        }

        let compressed_bytes = &archive_bytes[data_start..data_end];
        let decompressed_bytes = decompress_entry(compressed_bytes, entry.compression_method)?;

        let mut file = File::create(&target_file_path).map_err(|e| CacheError::Io {
            path: target_file_path.display().to_string(),
            source: e,
        })?;

        file.write_all(&decompressed_bytes).map_err(|e| CacheError::Io {
            path: target_file_path.display().to_string(),
            source: e,
        })?;

        extracted_files.push(relative_path);
    }

    Ok(extracted_files)
}