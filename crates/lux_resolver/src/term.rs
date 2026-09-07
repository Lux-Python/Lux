use std::fmt;
use smol_str::SmolStr;
use super::version_set::VersionSet;

/// Interned package identifier for zero-allocation indexing.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct PackageId(pub u32);

impl PackageId {
    /// Reserved `PackageId` for the root virtual package.
    pub const ROOT: Self = Self(0);
}

/// A logical proposition over a package version.
///
/// * **Positive term (`P in V`)**: Package `P` must be selected with a version in `V`.
/// * **Negative term (`not P in V`)**: Package `P` cannot be selected with a version in `V`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Term {
    package: PackageId,
    is_positive: bool,
    versions: VersionSet,
}

impl Term {
    /// Create a positive term: `P in V`.
    pub const fn positive(package: PackageId, versions: VersionSet) -> Self {
        Self {
            package,
            is_positive: true,
            versions,
        }
    }

    /// Create a negative term: `not P in V`.
    pub const fn negative(package: PackageId, versions: VersionSet) -> Self {
        Self {
            package,
            is_positive: false,
            versions,
        }
    }

    #[inline]
    pub const fn package(&self) -> PackageId {
        self.package
    }

    #[inline]
    pub const fn is_positive(&self) -> bool {
        self.is_positive
    }

    #[inline]
    pub const fn is_negative(&self) -> bool {
        !self.is_positive
    }

    #[inline]
    pub const fn versions(&self) -> &VersionSet {
        &self.versions
    }

    /// Return the logical negation (not T) of this term.
    #[must_use]
    pub fn negate(&self) -> Self {
        Self {
            package: self.package,
            is_positive: !self.is_positive,
            versions: self.versions.clone(),
        }
    }

    /// Whether this term logically satisfies `other`.
    ///
    /// For example, `P in [1.0, 2.0]` satisfies `P in [1.0, 3.0]`.
    #[must_use]
    pub fn satisfies(&self, other: &Self) -> bool {
        if self.package != other.package {
            return false;
        }

        match (self.is_positive, other.is_positive) {
            (true, true) => self.versions.difference(&other.versions).is_empty(),
            (true, false) => self.versions.is_disjoint(&other.versions),
            (false, true) => false,
            (false, false) => other.versions.difference(&self.versions).is_empty(),
        }
    }

    /// Whether this term contradicts `other`.
    ///
    /// For example, `P in [1.0, 2.0]` contradicts `P in [3.0, 4.0]`.
    #[must_use]
    pub fn contradicts(&self, other: &Self) -> bool {
        if self.package != other.package {
            return false;
        }

        match (self.is_positive, other.is_positive) {
            (true, true) => self.versions.is_disjoint(&other.versions),
            (true, false) => self.versions.difference(&other.versions).is_empty(),
            (false, true) => other.versions.difference(&self.versions).is_empty(),
            (false, false) => false,
        }
    }

    /// Computes the intersection of two terms for the same package.
    /// Returns `None` if the intersection is unsatisfiable / contradictory.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        if self.package != other.package {
            return None;
        }

        match (self.is_positive, other.is_positive) {
            (true, true) => {
                let inter = self.versions.intersection(&other.versions);
                if inter.is_empty() {
                    None
                } else {
                    Some(Self::positive(self.package, inter))
                }
            }
            (true, false) => {
                let diff = self.versions.difference(&other.versions);
                if diff.is_empty() {
                    None
                } else {
                    Some(Self::positive(self.package, diff))
                }
            }
            (false, true) => {
                let diff = other.versions.difference(&self.versions);
                if diff.is_empty() {
                    None
                } else {
                    Some(Self::positive(self.package, diff))
                }
            }
            (false, false) => {
                let union = self.versions.union(&other.versions);
                Some(Self::negative(self.package, union))
            }
        }
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_positive {
            write!(f, "pkg#{} in {}", self.package.0, self.versions)
        } else {
            write!(f, "pkg#{} not in {}", self.package.0, self.versions)
        }
    }
}

impl fmt::Debug for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Term({self})")
    }
}

use std::collections::HashMap;

/// Two-way interning registry for mapping package names to lightweight `PackageId`.
#[derive(Default, Clone, Debug)]
pub struct PackageRegistry {
    names: Vec<SmolStr>,
    index: HashMap<SmolStr, PackageId>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            names: Vec::new(),
            index: HashMap::new(),
        };
        // Seed root package at id 0
        reg.intern("$root");
        reg
    }

    /// Intern a package name, returning its unique `PackageId`.
    pub fn intern(&mut self, name: &str) -> PackageId {
        if let Some(&id) = self.index.get(name) {
            return id;
        }

        let id = PackageId(u32::try_from(self.names.len()).unwrap_or(u32::MAX));
        let smol = SmolStr::new(name);
        self.names.push(smol.clone());
        self.index.insert(smol, id);
        id
    }

    /// Retrieve the package name corresponding to a `PackageId`.
    #[must_use]
    pub fn name(&self, id: PackageId) -> Option<&str> {
        self.names
            .get(usize::try_from(id.0).unwrap_or(usize::MAX))
            .map(SmolStr::as_str)
    }

    /// Total number of interned packages.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the registry contains no packages.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}
