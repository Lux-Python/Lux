use miette::Diagnostic;
use thiserror::Error;

/// Domain error variants encountered within network transport, CAS caching, and archive extraction.
#[derive(Debug, Error, Diagnostic)]
pub enum CacheError {
    /// Failure communicating with a remote index or server.
    #[error("network request failed for URL '{url}': {source}")]
    #[diagnostic(
        code(lux::cache::network),
        help("Check your internet connection, proxy settings, or custom index URL.")
    )]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    /// Non-success HTTP status code returned by the index server.
    #[error("HTTP error {status} for URL '{url}'")]
    #[diagnostic(
        code(lux::cache::http),
        help("Verify that the package exists on the registry and that access credentials are valid.")
    )]
    Http { url: String, status: u16 },

    /// Cryptographic checksum mismatch between downloaded artifact and manifest digest.
    #[error("SHA-256 checksum mismatch for '{file}': expected {expected}, computed {computed}")]
    #[diagnostic(
        code(lux::cache::checksum_mismatch),
        help("The downloaded archive may be corrupted or tampered with. The cache entry has been purged.")
    )]
    ChecksumMismatch {
        file: String,
        expected: String,
        computed: String,
    },

    /// Corrupted, truncated, or unsupported zip archive format.
    #[error("corrupted or unsupported zip archive: {reason}")]
    #[diagnostic(
        code(lux::cache::invalid_zip),
        help("Ensure the wheel file is a valid, uncorrupted PKZIP archive.")
    )]
    InvalidZip { reason: String },

    /// PEP 658 or wheel metadata could not be discovered or extracted.
    #[error("failed to find dist-info METADATA for package '{package}' version '{version}'")]
    #[diagnostic(
        code(lux::cache::metadata_not_found),
        help("The wheel archive does not contain a standard .dist-info/METADATA file.")
    )]
    MetadataNotFound { package: String, version: String },

    /// Failure atomically committing an artifact into the Content-Addressable Store.
    #[error("failed to commit artifact to CAS path '{path}': {reason}")]
    #[diagnostic(
        code(lux::cache::cas_commit_failed),
        help("Verify filesystem write permissions and disk space for the Lux cache directory.")
    )]
    CasCommitFailed { path: String, reason: String },

    /// Linking failure when hardlinking or copying an extracted file into an environment.
    #[error("failed to link '{source_path}' to '{target_path}': {reason}")]
    #[diagnostic(
        code(lux::cache::link_failed),
        help("Ensure target directory is writable and the filesystem supports hardlinks or copies.")
    )]
    LinkFailed {
        source_path: String,
        target_path: String,
        reason: String,
    },

    /// Low-level filesystem I/O error.
    #[error("filesystem I/O error at '{path}': {source}")]
    #[diagnostic(code(lux::cache::io))]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// JSON / format serialization or deserialization failure.
    #[error("serialization or deserialization error: {reason}")]
    #[diagnostic(code(lux::cache::serialization))]
    Serialization { reason: String },
}