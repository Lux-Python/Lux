use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug, Clone)]
pub enum ResolverError {
    #[error("Dependency resolution failed due to an unresolvable conflict: \n{report}")]
    #[diagnostic(
        code(lux::resolver::unresolvable_conflict),
        help("Examine conflicting dependencies in the report above. Consider loosening version constraints or updating dependencies.")
    )]
    UnresolvableConflict {
        report: String,
    },

    #[error("Package '{package}' was not found in available registries")]
    #[diagnostic(
        code(lux::resolver::package_not_found),
        help("Ensure the package name is spelled correctly and published on PyPI.")
    )]
    PackageNotFound {
        package: String,
    },

    #[error("Failed to fetch dependencies for '{package}=={version}': {reason}")]
    #[diagnostic(code(lux::resolver::dependency_fetch_error))]
    DependencyFetchFailed {
        package: String,
        version: String,
        reason: String,
    },
}
