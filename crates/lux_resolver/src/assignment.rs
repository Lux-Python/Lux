use std::collections::HashMap;
use lux_core::types::Version;

use super::incompatibility::IncompatibilityId;
use super::term::{PackageId, Term};
use super::version_set::VersionSet;

/// A single assignment entry recorded in the solver's trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Assignment {
    /// An active heuristic decision to try a package at a specific version.
    Decision {
        package: PackageId,
        version: Version,
        level: usize,
    },
    /// A deduction derived via unit propagation from an incompatibility clause.
    Derivation {
        term: Term,
        cause: IncompatibilityId,
        level: usize,
    },
}

impl Assignment {
    #[inline]
    #[must_use]
    pub const fn level(&self) -> usize {
        match *self {
            Self::Decision { level, .. } | Self::Derivation { level, .. } => level,
        }
    }

    #[inline]
    #[must_use]
    pub const fn package(&self) -> PackageId {
        match *self {
            Self::Decision { package, .. } => package,
            Self::Derivation { ref term, .. } => term.package(),
        }
    }
}

/// The accumulated resolution state for a single package.
#[derive(Clone, Debug)]
pub struct PackageAssignment {
    pub decision: Option<Version>,
    pub positive: Option<VersionSet>,
    pub negative: VersionSet,
}

impl Default for PackageAssignment {
    fn default() -> Self {
        Self {
            decision: None,
            positive: None,
            negative: VersionSet::empty(),
        }
    }
}

impl PackageAssignment {
    /// Compute the current permitted version set for this package.
    #[must_use]
    pub fn allowed_versions(&self) -> VersionSet {
        if let Some(ref ver) = self.decision {
            return VersionSet::singleton(ver);
        }

        let base = self.positive.clone().unwrap_or_else(VersionSet::full);
        base.difference(&self.negative)
    }

    /// Check if this package state satisfies the given term.
    #[must_use]
    pub fn satisfies(&self, term: &Term) -> bool {
        if let Some(ref ver) = self.decision {
            if term.is_positive() {
                return term.versions().contains(ver);
            }
            return !term.versions().contains(ver);
        }

        if term.is_positive() {
            if let Some(ref pos) = self.positive {
                let allowed = pos.difference(&self.negative);
                return !allowed.is_empty() && allowed.difference(term.versions()).is_empty();
            }
            false
        } else {
            let allowed = self.allowed_versions();
            allowed.is_disjoint(term.versions())
        }
    }

    /// Check if this package state contradicts the given term.
    #[must_use]
    pub fn contradicts(&self, term: &Term) -> bool {
        if let Some(ref ver) = self.decision {
            if term.is_positive() {
                return !term.versions().contains(ver);
            }
            return term.versions().contains(ver);
        }

        if term.is_positive() {
            let allowed = self.allowed_versions();
            allowed.is_disjoint(term.versions())
        } else if let Some(ref pos) = self.positive {
            let allowed = pos.difference(&self.negative);
            !allowed.is_empty() && allowed.difference(term.versions()).is_empty()
        } else {
            false
        }
    }
}

/// The trail of assignments and decisions forming the `PubGrub` state machine.
#[derive(Default, Clone, Debug)]
pub struct PartialAssignment {
    trail: Vec<Assignment>,
    packages: HashMap<PackageId, PackageAssignment>,
    decision_level: usize,
}

impl PartialAssignment {
    pub fn new() -> Self {
        Self {
            trail: Vec::new(),
            packages: HashMap::new(),
            decision_level: 0,
        }
    }

    #[inline]
    #[must_use]
    pub const fn decision_level(&self) -> usize {
        self.decision_level
    }

    #[inline]
    #[must_use]
    pub fn trail(&self) -> &[Assignment] {
        &self.trail
    }

    /// Add root virtual decision at level 0 (permanent foundation).
    pub fn decide_root(&mut self, package: PackageId, version: Version) {
        let entry = self.packages.entry(package).or_default();
        entry.decision = Some(version.clone());
        self.trail.push(Assignment::Decision {
            package,
            version,
            level: 0,
        });
    }

    /// Add a decision at a new decision level.
    pub fn decide(&mut self, package: PackageId, version: Version) {
        self.decision_level += 1;
        let level = self.decision_level;

        let entry = self.packages.entry(package).or_default();
        entry.decision = Some(version.clone());

        self.trail.push(Assignment::Decision {
            package,
            version,
            level,
        });
    }

    /// Add a unit derivation caused by an incompatibility.
    pub fn derive(&mut self, term: Term, cause: IncompatibilityId) {
        let level = self.decision_level;
        let pkg = term.package();

        let entry = self.packages.entry(pkg).or_default();
        if term.is_positive() {
            entry.positive = Some(
                entry.positive.take().map_or_else(
                    || term.versions().clone(),
                    |prev| prev.intersection(term.versions()),
                ),
            );
        } else {
            entry.negative = entry.negative.union(term.versions());
        }

        self.trail.push(Assignment::Derivation { term, cause, level });
    }

    /// Query the current assignment for a package.
    #[must_use]
    pub fn get_package(&self, package: PackageId) -> Option<&PackageAssignment> {
        self.packages.get(&package)
    }

    /// Whether the partial assignment satisfies a term.
    #[must_use]
    pub fn satisfies(&self, term: &Term) -> bool {
        self.packages
            .get(&term.package())
            .is_some_and(|pkg| pkg.satisfies(term))
    }

    /// Whether the partial assignment contradicts a term.
    #[must_use]
    pub fn contradicts(&self, term: &Term) -> bool {
        self.packages
            .get(&term.package())
            .is_some_and(|pkg| pkg.contradicts(term))
    }

    /// Returns the decision level at which a term was satisfied, if any.
    #[must_use]
    pub fn satisfaction_level(&self, term: &Term) -> Option<usize> {
        if !self.satisfies(term) {
            return None;
        }

        let mut max_level = 0;
        for assign in &self.trail {
            if assign.package() == term.package() {
                max_level = max_level.max(assign.level());
            }
        }
        Some(max_level)
    }

    /// Backjump to a target decision level, undoing all assignments made at higher levels.
    pub fn backtrack(&mut self, target_level: usize) {
        self.decision_level = target_level;

        // Pop trail assignments above target_level
        while let Some(last) = self.trail.last() {
            if last.level() <= target_level {
                break;
            }
            self.trail.pop();
        }

        // Reconstruct package aggregates from remaining trail
        self.packages.clear();
        for assign in &self.trail {
            match assign {
                Assignment::Decision { package, version, .. } => {
                    let entry = self.packages.entry(*package).or_default();
                    entry.decision = Some(version.clone());
                }
                Assignment::Derivation { term, .. } => {
                    let entry = self.packages.entry(term.package()).or_default();
                    if term.is_positive() {
                        entry.positive = Some(
                            entry.positive.take().map_or_else(
                                || term.versions().clone(),
                                |prev| prev.intersection(term.versions()),
                            ),
                        );
                    } else {
                        entry.negative = entry.negative.union(term.versions());
                    }
                }
            }
        }
    }

    /// Returns all packages that have positive requirements but no decision made yet,
    /// ordered chronologically by when they were first introduced into the trail.
    #[must_use]
    pub fn undecided_packages(&self) -> Vec<PackageId> {
        let mut seen = std::collections::HashSet::new();
        let mut undecided = Vec::new();

        for assign in &self.trail {
            let pkg = assign.package();
            if seen.insert(pkg) {
                if let Some(state) = self.packages.get(&pkg) {
                    if state.decision.is_none() && state.positive.is_some() {
                        undecided.push(pkg);
                    }
                }
            }
        }

        for (&pkg, state) in &self.packages {
            if seen.insert(pkg) && state.decision.is_none() && state.positive.is_some() {
                undecided.push(pkg);
            }
        }

        undecided
    }

    /// Extract resolved package versions if resolution is complete.
    #[must_use]
    pub fn extract_solution(&self) -> Vec<(PackageId, Version)> {
        self.packages
            .iter()
            .filter_map(|(&pkg, state)| state.decision.as_ref().map(|v| (pkg, v.clone())))
            .collect()
    }
}
