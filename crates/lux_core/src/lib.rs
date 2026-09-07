//! Lux Core: Domain types, environment management, and project manifest parsing.

#![deny(unsafe_code)]

pub mod env;
pub mod error;
pub mod manifest;
pub mod types;

pub use env::{PythonVersionFile, VirtualEnv};
pub use error::LuxError;
pub use manifest::lockfile::{LockedPackage, LuxLockfile, PackageHash};
pub use manifest::{BuildSystem, ProjectSection, PyProjectToml, ScriptMetadata};
pub use types::{
    normalize_name, EnvironmentMarker, PackageName, Requirement, TypeError, Version,
    VersionSpecifier, VersionSpecifiers,
};
