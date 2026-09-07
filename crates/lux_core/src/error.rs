use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum LuxError {
    #[error("Network error: {url}: {reason}")]
    #[diagnostic(code(lux::network_error), help("Verify internet connectivity and registry status"))]
    Network { url: String, reason: String },

    #[error("Serialization / Deserialization error: {0}")]
    #[diagnostic(code(lux::serialization_error))]
    Serialization(#[from] serde_json::Error),

    #[error("TOML parsing error: {0}")]
    #[diagnostic(code(lux::toml_error))]
    Toml(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    #[diagnostic(code(lux::toml_ser_error))]
    TomlSer(#[from] toml::ser::Error),

    #[error("Manifest pyproject.toml not found at '{path}'")]
    #[diagnostic(code(lux::manifest_not_found), help("Run `lux init` to initialize a new project."))]
    ManifestNotFound { path: String },

    #[error("Manifest error: {reason}")]
    #[diagnostic(code(lux::manifest_error))]
    ManifestError { reason: String },

    #[error("Virtual environment error: {reason}")]
    #[diagnostic(code(lux::virtualenv_error), help("Run `lux init` to create a fresh virtual environment."))]
    VirtualEnvError { reason: String },

    #[error("Requirement parse error: {reason}")]
    #[diagnostic(code(lux::package_parse_error))]
    PackageParseError { reason: String },

    #[error("Domain type error: {0}")]
    #[diagnostic(code(lux::type_error))]
    TypeError(#[from] crate::types::TypeError),

    #[error("I/O error: {0}")]
    #[diagnostic(code(lux::io_error))]
    Io(#[from] std::io::Error),
}
