//! `PyPI` / package registry publishing command (`lux publish`).

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use miette::{IntoDiagnostic, Result};
use crate::ui::reporter::Status;
use crate::ui::theme::Style;

/// Execute the `lux publish` command.
pub async fn execute(files: &[PathBuf], repository: Option<&str>, token: Option<&str>) -> Result<()> {
    let start = Instant::now();
    let repo_url = repository.unwrap_or("https://upload.pypi.org/legacy/");

    let mut target_files = files.to_vec();
    if target_files.is_empty() {
        let dist_dir = Path::new("dist");
        if dist_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(dist_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "whl" || ext == "gz" || ext == "tar" {
                            target_files.push(path);
                        }
                    }
                }
            }
        }
    }

    if target_files.is_empty() {
        Status::warn("No distribution artifacts found in dist/. Run `lux build` first.");
        return Ok(());
    }

    println!("Publishing {} distribution artifact(s) to {}", target_files.len(), Style::bold(repo_url));
    let auth_token = token
        .map(String::from)
        .or_else(|| std::env::var("LUX_PYPI_TOKEN").ok());

    if auth_token.is_some() {
        println!("   Authentication: {}", Style::green("API Token configured"));
    } else {
        println!("   Authentication: {}", Style::yellow("Dry run (no API token provided)"));
    }
    println!();

    for file in &target_files {
        let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("artifact");
        Status::action("Verifying", &format!("{}...", Style::bold(file_name)));

        // Read file and compute cryptographic digest
        let bytes = std::fs::read(file).into_diagnostic()?;
        let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
        let hex_digest = hex_encode(digest.as_ref());

        println!("   Size:   {} bytes", bytes.len());
        println!("   SHA256: {}", Style::dim(&hex_digest));

        if let Some(ref tok) = auth_token {
            Status::action("Uploading", &format!("{} to {repo_url}...", Style::bold(file_name)));
            // Real publish: upload via multipart form to repository
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .into_diagnostic()?;

            let form = reqwest::multipart::Form::new()
                .text(":action", "file_upload")
                .text("protocol_version", "1")
                .part(
                    "content",
                    reqwest::multipart::Part::bytes(bytes.clone())
                        .file_name(file_name.to_string()),
                );

            let res = client
                .post(repo_url)
                .bearer_auth(tok)
                .multipart(form)
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    Status::completed("Published", file_name, None);
                }
                Ok(resp) => {
                    Status::warn(&format!(
                        "Repository returned status {} for {}",
                        resp.status(),
                        file_name
                    ));
                }
                Err(e) => {
                    Status::warn(&format!("Network error uploading {file_name}: {e}"));
                }
            }
        } else {
            Status::completed("Verified", &format!("{file_name} (dry run)"), None);
        }
    }

    println!();
    Status::completed(
        "Finished",
        &format!("{} package(s) processed", target_files.len()),
        Some(start.elapsed()),
    );
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}
