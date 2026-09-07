use std::sync::Arc;
use std::time::Duration;
use moka::future::Cache;
use reqwest::header::{ACCEPT, RANGE, USER_AGENT};
use reqwest::Client;
use ring::digest::{Context, SHA256};

use super::super::archive::zip_range::{
    decompress_entry, parse_central_directory, parse_local_header_data_offset, EocdRecord,
};
use super::super::error::CacheError;
use super::simple_api::{SimpleFile, SimpleProject};

const PEP691_ACCEPT: &str =
    "application/vnd.pypi.simple.v1+json, application/vnd.pypi.simple.v1+html;q=0.2, text/html;q=0.01";
const DEFAULT_INDEX_URL: &str = "https://pypi.org/simple";
const USER_AGENT_STR: &str = "lux/0.1.0";

/// High-throughput HTTP/2 `PyPI` client with connection pooling, in-memory LRU metadata cache,
/// and lazy wheel Central Directory HTTP Range extraction.
#[derive(Clone)]
pub struct PyPiClient {
    client: Client,
    base_url: String,
    project_cache: Arc<Cache<String, SimpleProject>>,
}

impl PyPiClient {
    /// Create a new `PyPI` client pointing to an index URL (defaulting to `PyPI` Simple).
    pub fn new(base_url: Option<&str>) -> Result<Self, CacheError> {
        let client = Client::builder()
            .tcp_nodelay(true)
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| CacheError::Network {
                url: base_url.unwrap_or(DEFAULT_INDEX_URL).to_string(),
                source: e,
            })?;

        let base = base_url.unwrap_or(DEFAULT_INDEX_URL).trim_end_matches('/').to_string();
        let project_cache = Arc::new(
            Cache::builder()
                .max_capacity(2048)
                .time_to_live(Duration::from_secs(600))
                .build(),
        );

        Ok(Self {
            client,
            base_url: base,
            project_cache,
        })
    }

    /// Query the PEP 691 Simple API for project file distributions, using the in-memory LRU cache if hot.
    pub async fn get_project(&self, package_name: &str) -> Result<SimpleProject, CacheError> {
        let normalized = lux_core::types::normalize_name(package_name);

        if let Some(cached) = self.project_cache.get(&normalized).await {
            return Ok(cached);
        }

        let url = format!("{}/{}/", self.base_url, normalized);
        let resp = self
            .client
            .get(&url)
            .header(ACCEPT, PEP691_ACCEPT)
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: url.clone(),
                source: e,
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(CacheError::Http {
                url,
                status: status.as_u16(),
            });
        }

        let body_bytes = resp.bytes().await.map_err(|e| CacheError::Network {
            url: url.clone(),
            source: e,
        })?;

        let project: SimpleProject =
            serde_json::from_slice(&body_bytes).map_err(|e| CacheError::Serialization {
                reason: format!("failed to parse PEP 691 JSON for '{normalized}': {e}"),
            })?;

        self.project_cache.insert(normalized, project.clone()).await;
        Ok(project)
    }

    /// Download PEP 658 metadata directly from the `${url}.metadata` endpoint if available.
    pub async fn get_metadata_pep658(&self, file: &SimpleFile) -> Result<Option<String>, CacheError> {
        if !file.has_remote_metadata() {
            return Ok(None);
        }

        let metadata_url = format!("{}.metadata", file.url);
        let resp = self
            .client
            .get(&metadata_url)
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: metadata_url.clone(),
                source: e,
            })?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let bytes = resp.bytes().await.map_err(|e| CacheError::Network {
            url: metadata_url.clone(),
            source: e,
        })?;

        if let Some(expected_sha256) = file.metadata_sha256() {
            let mut ctx = Context::new(&SHA256);
            ctx.update(&bytes);
            let digest = ctx.finish();
            let computed = digest
                .as_ref()
                .iter()
                .fold(String::with_capacity(64), |mut acc, b| {
                    use std::fmt::Write;
                    let _ = write!(acc, "{b:02x}");
                    acc
                });

            if !computed.eq_ignore_ascii_case(expected_sha256) {
                return Err(CacheError::ChecksumMismatch {
                    file: metadata_url,
                    expected: expected_sha256.to_string(),
                    computed,
                });
            }
        }

        let content = String::from_utf8_lossy(&bytes).to_string();
        Ok(Some(content))
    }

    /// Issue HTTP Range requests to stream and parse only the Central Directory of a remote `.whl`,
    /// extracting `.dist-info/METADATA` without downloading the full archive payload.
    pub async fn get_metadata_lazy_zip(&self, file_url: &str) -> Result<String, CacheError> {
        // Fetch the last 65,536 bytes of the remote file
        let tail_resp = self
            .client
            .get(file_url)
            .header(RANGE, "bytes=-65536")
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: file_url.to_string(),
                source: e,
            })?;

        let tail_bytes = tail_resp.bytes().await.map_err(|e| CacheError::Network {
            url: file_url.to_string(),
            source: e,
        })?;

        let eocd = EocdRecord::parse(&tail_bytes)?;
        let cd_size = usize::try_from(eocd.cd_size).map_err(|_| CacheError::InvalidZip {
            reason: "central directory size exceeds address space".to_string(),
        })?;

        // Determine if the central directory is entirely contained in our tail buffer
        let cd_bytes = if tail_bytes.len() >= cd_size + 22 {
            let start = tail_bytes.len() - cd_size - 22;
            tail_bytes.slice(start..start + cd_size)
        } else {
            // Fetch the exact central directory slice via byte range
            let range_header = format!("bytes={}-{}", eocd.cd_offset, eocd.cd_offset + eocd.cd_size - 1);
            let cd_resp = self
                .client
                .get(file_url)
                .header(RANGE, range_header)
                .header(USER_AGENT, USER_AGENT_STR)
                .send()
                .await
                .map_err(|e| CacheError::Network {
                    url: file_url.to_string(),
                    source: e,
                })?;

            cd_resp.bytes().await.map_err(|e| CacheError::Network {
                url: file_url.to_string(),
                source: e,
            })?
        };

        let entries = parse_central_directory(&cd_bytes)?;
        let metadata_entry = entries
            .iter()
            .find(|e| e.is_metadata())
            .ok_or_else(|| CacheError::MetadataNotFound {
                package: file_url.to_string(),
                version: "unknown".to_string(),
            })?;

        // Fetch local header to calculate variable data offset
        let header_range = format!(
            "bytes={}-{}",
            metadata_entry.local_header_offset,
            metadata_entry.local_header_offset + 1024
        );
        let header_resp = self
            .client
            .get(file_url)
            .header(RANGE, header_range)
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: file_url.to_string(),
                source: e,
            })?;

        let header_bytes = header_resp.bytes().await.map_err(|e| CacheError::Network {
            url: file_url.to_string(),
            source: e,
        })?;

        let rel_data_offset = parse_local_header_data_offset(&header_bytes)?;
        let data_start = metadata_entry.local_header_offset + rel_data_offset;
        let data_end = data_start + metadata_entry.compressed_size - 1;

        let data_range = format!("bytes={data_start}-{data_end}");
        let data_resp = self
            .client
            .get(file_url)
            .header(RANGE, data_range)
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: file_url.to_string(),
                source: e,
            })?;

        let compressed_data = data_resp.bytes().await.map_err(|e| CacheError::Network {
            url: file_url.to_string(),
            source: e,
        })?;

        let decompressed = decompress_entry(&compressed_data, metadata_entry.compression_method)?;
        let metadata_text = String::from_utf8_lossy(&decompressed).to_string();

        Ok(metadata_text)
    }

    /// Stream and download full wheel archive bytes, validating cryptographic SHA-256 on the fly.
    pub async fn download_wheel(&self, file: &SimpleFile) -> Result<Vec<u8>, CacheError> {
        let resp = self
            .client
            .get(&file.url)
            .header(USER_AGENT, USER_AGENT_STR)
            .send()
            .await
            .map_err(|e| CacheError::Network {
                url: file.url.clone(),
                source: e,
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(CacheError::Http {
                url: file.url.clone(),
                status: status.as_u16(),
            });
        }

        let bytes = resp.bytes().await.map_err(|e| CacheError::Network {
            url: file.url.clone(),
            source: e,
        })?;

        if let Some(expected_sha256) = file.sha256() {
            let mut ctx = Context::new(&SHA256);
            ctx.update(&bytes);
            let digest = ctx.finish();
            let computed = digest
                .as_ref()
                .iter()
                .fold(String::with_capacity(64), |mut acc, b| {
                    use std::fmt::Write;
                    let _ = write!(acc, "{b:02x}");
                    acc
                });

            if !computed.eq_ignore_ascii_case(expected_sha256) {
                return Err(CacheError::ChecksumMismatch {
                    file: file.filename.clone(),
                    expected: expected_sha256.to_string(),
                    computed,
                });
            }
        }

        Ok(bytes.to_vec())
    }
}