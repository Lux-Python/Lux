use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum TypeError {
    #[error("Invalid package name '{name}': {reason}")]
    #[diagnostic(
        code(lux::types::invalid_package_name),
        help("Package names must start and end with an alphanumeric character and contain only letters, digits, '.', '_', or '-'.")
    )]
    InvalidPackageName {
        name: String,
        reason: String,
    },

    #[error("Invalid PEP 440 version '{version}': {source}")]
    #[diagnostic(
        code(lux::types::invalid_version),
        help("Specify a valid PEP 440 version, e.g., '1.0.0', '2.1.0a1', or '3.0.0.post1'.")
    )]
    InvalidVersion {
        version: String,
        #[source]
        source: pep440_rs::VersionParseError,
    },

    #[error("Invalid PEP 440 version specifier '{spec}': {source}")]
    #[diagnostic(
        code(lux::types::invalid_version_specifier),
        help("Specify valid PEP 440 specifiers, e.g., '>=1.20.0, <2.0.0' or '~=3.8'.")
    )]
    InvalidVersionSpecifier {
        spec: String,
        #[source]
        source: <pep440_rs::VersionSpecifier as std::str::FromStr>::Err,
    },

    #[error("Invalid PEP 440 compound version specifiers '{spec}': {source}")]
    #[diagnostic(
        code(lux::types::invalid_version_specifiers),
        help("Specify valid PEP 440 specifiers, e.g., '>=1.20.0, <2.0.0' or '~=3.8'.")
    )]
    InvalidVersionSpecifiers {
        spec: String,
        #[source]
        source: pep440_rs::VersionSpecifiersParseError,
    },

    #[error("Invalid PEP 508 environment marker '{marker}': {source}")]
    #[diagnostic(
        code(lux::types::invalid_environment_marker),
        help("Ensure marker syntax adheres to PEP 508, e.g., 'python_version >= \"3.8\" and os_name == \"posix\"'.")
    )]
    InvalidEnvironmentMarker {
        marker: String,
        #[source]
        source: pep508_rs::Pep508Error,
    },

    #[error("Invalid PEP 508 requirement '{requirement}': {source}")]
    #[diagnostic(
        code(lux::types::invalid_requirement),
        help("Requirement must follow PEP 508 format, e.g., 'requests[security] >= 2.28.0; python_version >= \"3.8\"'.")
    )]
    InvalidRequirement {
        requirement: String,
        #[source]
        source: pep508_rs::Pep508Error,
    },
}
