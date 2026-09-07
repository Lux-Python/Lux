use std::fs;
use std::path::Path;
use miette::Result;

use lux_cache::cas::CasStore;

use super::super::ui::{format_bytes, Status, Style};

/// Execute cache inspection and management commands (`lux cache`).
pub fn run_cache(action: Option<&str>) -> Result<()> {
    let store = CasStore::open(None).map_err(|e| miette::miette!("Failed to open CAS: {e}"))?;
    let layout = store.layout();

    match action {
        Some("dir") => {
            println!("{}", layout.root().display());
        }
        Some("clean") => {
            let tmp_dir = layout.tmp_dir();
            let mut cleaned_count = 0;
            if tmp_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(tmp_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let _ = fs::remove_file(&path);
                            cleaned_count += 1;
                        } else if path.is_dir() {
                            let _ = fs::remove_dir_all(&path);
                            cleaned_count += 1;
                        }
                    }
                }
            }
            Status::completed("Cleaned", &format!("{cleaned_count} temporary staging files"), None);
        }
        _ => {
            println!("Cache Directory: {}", layout.root().display());
            println!("Blobs:           {}", layout.blobs_dir().display());
            println!("Extracted:       {}", layout.extracted_dir().display());
            println!("Staging:         {}", layout.tmp_dir().display());

            let (file_count, total_bytes) = compute_dir_stats(layout.root());
            println!();
            println!(
                "Stored Artifacts: {} files ({})",
                Style::bold(&file_count.to_string()),
                Style::cyan(&format_bytes(total_bytes))
            );
        }
    }

    Ok(())
}

fn compute_dir_stats(dir: &Path) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                files += 1;
                if let Ok(meta) = entry.metadata() {
                    bytes += meta.len();
                }
            } else if path.is_dir() {
                let (sub_files, sub_bytes) = compute_dir_stats(&path);
                files += sub_files;
                bytes += sub_bytes;
            }
        }
    }

    (files, bytes)
}
