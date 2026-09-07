//! Lux Resolver: Pure, decoupled `PubGrub`-CDCL SAT solver implementation.

#![deny(unsafe_code)]

pub mod assignment;
pub mod dag;
pub mod error;
pub mod incompatibility;
pub mod provider;
pub mod solver;
pub mod term;
pub mod version_set;

pub use error::ResolverError;
pub use incompatibility::{Incompatibility, IncompatibilityCause, IncompatibilityId};
pub use provider::{DependencyProvider, MemoryDependencyProvider};
pub use solver::PubGrubSolver;
pub use term::{PackageId, PackageRegistry, Term};
pub use version_set::VersionSet;
