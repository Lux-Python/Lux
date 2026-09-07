use std::fmt;
use id_arena::Id;
use lux_core::types::Version;
use smallvec::SmallVec;

use super::term::{PackageId, Term};
use super::version_set::VersionSet;

/// Arena-allocated identifier for an Incompatibility.
pub type IncompatibilityId = Id<Incompatibility>;

/// The origin or reason an incompatibility exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IncompatibilityCause {
    /// Root requirement established by the user.
    Root,
    /// Direct dependency relation: `package@version` requires `target` in a spec.
    Dependency {
        package: PackageId,
        version: Version,
        target: PackageId,
    },
    /// A package has no versions matching a requested range.
    NoVersions {
        package: PackageId,
    },
    /// Learned clause synthesized via CDCL resolution between two conflicting clauses.
    Conflict {
        conflict: IncompatibilityId,
        other: IncompatibilityId,
    },
}

/// A set of terms that cannot simultaneously be satisfied.
///
/// Backed by [`SmallVec`] to guarantee inline stack allocation for common 1-term and 2-term clauses.
#[derive(Clone, PartialEq, Eq)]
pub struct Incompatibility {
    terms: SmallVec<[Term; 2]>,
    cause: IncompatibilityCause,
}

impl Incompatibility {
    /// Create a new incompatibility with terms and an origin cause.
    pub const fn new(terms: SmallVec<[Term; 2]>, cause: IncompatibilityCause) -> Self {
        Self { terms, cause }
    }

    /// Root incompatibility stating that the root package version 1.0.0 must be selected.
    pub fn from_root(root_pkg: PackageId, root_ver: &Version) -> Self {
        let mut terms = SmallVec::new();
        terms.push(Term::negative(root_pkg, VersionSet::singleton(root_ver)));
        Self {
            terms,
            cause: IncompatibilityCause::Root,
        }
    }

    /// Dependency incompatibility: `pkg == ver` implies `dep in range`.
    pub fn from_dependency(
        pkg: PackageId,
        ver: &Version,
        dep: PackageId,
        dep_range: VersionSet,
    ) -> Self {
        let mut terms = SmallVec::new();
        terms.push(Term::positive(pkg, VersionSet::singleton(ver)));
        terms.push(Term::negative(dep, dep_range));
        Self {
            terms,
            cause: IncompatibilityCause::Dependency {
                package: pkg,
                version: ver.clone(),
                target: dep,
            },
        }
    }

    /// No versions exist for a package in the required range.
    pub fn from_no_versions(pkg: PackageId, range: VersionSet) -> Self {
        let mut terms = SmallVec::new();
        terms.push(Term::positive(pkg, range));
        Self {
            terms,
            cause: IncompatibilityCause::NoVersions { package: pkg },
        }
    }

    #[inline]
    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    #[inline]
    pub const fn terms_mut(&mut self) -> &mut SmallVec<[Term; 2]> {
        &mut self.terms
    }

    #[inline]
    pub const fn cause(&self) -> &IncompatibilityCause {
        &self.cause
    }

    /// Whether this incompatibility is the root clause.
    #[inline]
    pub const fn is_root(&self) -> bool {
        matches!(self.cause, IncompatibilityCause::Root)
    }
}

impl fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not(")?;
        for (i, term) in self.terms.iter().enumerate() {
            if i > 0 {
                write!(f, " and ")?;
            }
            write!(f, "{term}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Debug for Incompatibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Incompatibility({self}, cause={:?})", self.cause)
    }
}
