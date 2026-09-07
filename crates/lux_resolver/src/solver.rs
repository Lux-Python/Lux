use std::collections::{HashMap, VecDeque};
use id_arena::Arena;
use lux_core::types::{PackageName, Requirement, Version};
use smallvec::SmallVec;

use super::assignment::{Assignment, PartialAssignment};
use super::dag::ConflictDag;
use super::error::ResolverError;
use super::incompatibility::{Incompatibility, IncompatibilityCause, IncompatibilityId};
use super::provider::DependencyProvider;
use super::term::{PackageId, PackageRegistry, Term};
use super::version_set::VersionSet;

/// PubGrub-style dependency solver.
pub struct PubGrubSolver<'a, P: DependencyProvider> {
    provider: &'a P,
    arena: Arena<Incompatibility>,
    package_incompatibilities: HashMap<PackageId, Vec<IncompatibilityId>>,
    assignments: PartialAssignment,
    registry: PackageRegistry,
    root_pkg: PackageId,
    root_ver: Version,
    latest_conflict: Option<IncompatibilityId>,
}

impl<'a, P: DependencyProvider> PubGrubSolver<'a, P> {
    pub fn new(provider: &'a P) -> Self {
        let mut registry = PackageRegistry::new();
        let root_pkg = registry.intern("$root");
        let root_ver = Version::parse("1.0.0").expect("valid static root version");

        Self {
            provider,
            arena: Arena::new(),
            package_incompatibilities: HashMap::new(),
            assignments: PartialAssignment::new(),
            registry,
            root_pkg,
            root_ver,
            latest_conflict: None,
        }
    }

    /// Add an incompatibility to the arena and update the package index.
    fn add_incompatibility(&mut self, incomp: Incompatibility) -> IncompatibilityId {
        let id = self.arena.alloc(incomp);
        for term in self.arena[id].terms() {
            self.package_incompatibilities
                .entry(term.package())
                .or_default()
                .push(id);
        }
        id
    }

    /// Resolve a set of root requirements into a locked dependency mapping.
    pub fn resolve(
        &mut self,
        root_requirements: &[Requirement],
    ) -> Result<HashMap<PackageName, Version>, ResolverError> {
        let root_incomp = Incompatibility::from_root(self.root_pkg, &self.root_ver);
        let root_id = self.add_incompatibility(root_incomp);

        let mut queue = VecDeque::new();
        queue.push_back(root_id);

        for req in root_requirements {
            let dep_id = self.registry.intern(req.name().as_str());
            let version_set = match req.as_pep508().version_or_url {
                Some(pep508_rs::VersionOrUrl::VersionSpecifier(ref specs)) => {
                    VersionSet::from(version_ranges::Ranges::from(specs.clone()))
                }
                Some(pep508_rs::VersionOrUrl::Url(_)) | None => VersionSet::full(),
            };

            let incomp = Incompatibility::from_dependency(
                self.root_pkg,
                &self.root_ver,
                dep_id,
                version_set,
            );
            let incomp_id = self.add_incompatibility(incomp);
            queue.push_back(incomp_id);
        }

        // Establish root package at level 0 (permanent)
        self.assignments.decide_root(self.root_pkg, self.root_ver.clone());

        loop {
            if let Some(conflict_id) = self.propagate(&mut queue) {
                self.latest_conflict = Some(conflict_id);
                let learned_id = self.resolve_conflict(conflict_id)?;
                queue.push_back(learned_id);
                continue;
            }

            let undecided = self.assignments.undecided_packages();
            if undecided.is_empty() {
                let raw_solution = self.assignments.extract_solution();
                let mut solution = HashMap::new();

                for (pkg_id, version) in raw_solution {
                    if pkg_id == self.root_pkg {
                        continue;
                    }
                    if let Some(name_str) = self.registry.name(pkg_id) {
                        let pkg_name = PackageName::new(name_str)
                            .map_err(|e| ResolverError::DependencyFetchFailed {
                                package: name_str.to_string(),
                                version: version.to_string(),
                                reason: e.to_string(),
                            })?;
                        solution.insert(pkg_name, version);
                    }
                }

                return Ok(solution);
            }

            let chosen_pkg = undecided[0];
            let pkg_name = self.registry.name(chosen_pkg).unwrap_or("unknown");

            let candidates = self.provider.get_candidates(pkg_name)?;
            let allowed_set = self
                .assignments
                .get_package(chosen_pkg)
                .map_or_else(VersionSet::full, super::assignment::PackageAssignment::allowed_versions);

            let matching_versions: Vec<Version> = candidates
                .into_iter()
                .filter(|v| allowed_set.contains(v))
                .collect();

            if matching_versions.is_empty() {
                let pos_set = self
                    .assignments
                    .get_package(chosen_pkg)
                    .and_then(|p| p.positive.clone())
                    .unwrap_or_else(VersionSet::full);

                let no_ver_incomp = Incompatibility::from_no_versions(chosen_pkg, pos_set);
                let id = self.add_incompatibility(no_ver_incomp);
                queue.push_back(id);
            } else {
                let selected_version = matching_versions.last().cloned().unwrap();
                self.assignments.decide(chosen_pkg, selected_version.clone());

                let deps = self.provider.get_dependencies(pkg_name, &selected_version)?;
                for dep in deps {
                    let dep_id = self.registry.intern(dep.name().as_str());
                    let dep_set = match dep.as_pep508().version_or_url {
                        Some(pep508_rs::VersionOrUrl::VersionSpecifier(ref specs)) => {
                            VersionSet::from(version_ranges::Ranges::from(specs.clone()))
                        }
                        Some(pep508_rs::VersionOrUrl::Url(_)) | None => VersionSet::full(),
                    };

                    let dep_incomp = Incompatibility::from_dependency(
                        chosen_pkg,
                        &selected_version,
                        dep_id,
                        dep_set,
                    );
                    let id = self.add_incompatibility(dep_incomp);
                    queue.push_back(id);
                }
            }
        }
    }

    /// Unit propagation engine. Returns `Some(conflict_id)` if a contradiction occurs.
    fn propagate(&mut self, queue: &mut VecDeque<IncompatibilityId>) -> Option<IncompatibilityId> {
        while let Some(incomp_id) = queue.pop_front() {
            let terms = self.arena[incomp_id].terms().to_vec();

            let mut unsatisfied_terms = Vec::new();
            let mut all_satisfied = true;

            for term in &terms {
                if self.assignments.contradicts(term) {
                    all_satisfied = false;
                    break;
                }
                if !self.assignments.satisfies(term) {
                    all_satisfied = false;
                    unsatisfied_terms.push(term.clone());
                }
            }

            if all_satisfied {
                return Some(incomp_id);
            }

            if unsatisfied_terms.len() == 1 {
                let satisfied_count = terms.iter().filter(|t| self.assignments.satisfies(t)).count();
                if satisfied_count == terms.len() - 1 {
                    let derived_term = unsatisfied_terms[0].negate();
                    let pkg = derived_term.package();

                    self.assignments.derive(derived_term, incomp_id);

                    if let Some(dependents) = self.package_incompatibilities.get(&pkg) {
                        for &dep_id in dependents {
                            if dep_id != incomp_id && !queue.contains(&dep_id) {
                                queue.push_back(dep_id);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Resolves a conflict clause using CDCL resolution heuristics and non-chronological backjumping.
    fn resolve_conflict(&mut self, mut conflict_id: IncompatibilityId) -> Result<IncompatibilityId, ResolverError> {
        loop {
            if self.assignments.decision_level() == 0 {
                let conflict_root = self.latest_conflict.unwrap_or(conflict_id);
                let dag = ConflictDag::new(conflict_root);
                let report = dag.build_remediation_report(&self.arena, &self.registry);
                return Err(ResolverError::UnresolvableConflict { report });
            }

            let terms = self.arena[conflict_id].terms().to_vec();
            let current_level = self.assignments.decision_level();

            // Find any term in conflict_id that was derived (i.e. not an active decision)
            let mut derived_pkg = None;
            for assign in self.assignments.trail().iter().rev() {
                if let Assignment::Derivation { term, .. } = assign {
                    if terms.iter().any(|t| t.package() == term.package()) {
                        let is_pure_derivation = self
                            .assignments
                            .get_package(term.package())
                            .is_none_or(|p| p.decision.is_none());

                        if is_pure_derivation || assign.level() == current_level {
                            derived_pkg = Some(term.package());
                            break;
                        }
                    }
                }
            }

            // If no term needs resolution: all remaining terms are decisions
            let Some(target_pkg) = derived_pkg else {
                let mut levels: Vec<usize> = terms
                    .iter()
                    .filter_map(|t| self.assignments.satisfaction_level(t))
                    .collect();
                levels.sort_unstable();
                levels.dedup();

                let target_level = if levels.len() <= 1 {
                    0
                } else {
                    levels[levels.len() - 2]
                };

                self.assignments.backtrack(target_level);
                return Ok(conflict_id);
            };

            // Find cause of the derived package
            let mut cause_id = None;
            for assign in self.assignments.trail().iter().rev() {
                if let Assignment::Derivation { term, cause, .. } = assign {
                    if term.package() == target_pkg {
                        cause_id = Some(*cause);
                        break;
                    }
                }
            }

            let Some(cause_incomp_id) = cause_id else {
                let target_level = current_level.saturating_sub(1);
                self.assignments.backtrack(target_level);
                return Ok(conflict_id);
            };

            // Resolve conflict_id with cause_incomp_id on target_pkg
            let cause_terms = self.arena[cause_incomp_id].terms().to_vec();
            let mut resolved_terms: SmallVec<[Term; 2]> = SmallVec::new();

            for term in &terms {
                if term.package() != target_pkg {
                    resolved_terms.push(term.clone());
                }
            }

            for term in &cause_terms {
                if term.package() != target_pkg {
                    if let Some(existing) = resolved_terms.iter_mut().find(|t| t.package() == term.package()) {
                        if let Some(combined) = existing.intersect(term) {
                            *existing = combined;
                        }
                    } else {
                        resolved_terms.push(term.clone());
                    }
                }
            }

            let new_incomp = Incompatibility::new(
                resolved_terms,
                IncompatibilityCause::Conflict {
                    conflict: conflict_id,
                    other: cause_incomp_id,
                },
            );

            conflict_id = self.add_incompatibility(new_incomp);
            self.latest_conflict = Some(conflict_id);

            // Check if resolution has reached level 0 or a learned UIP clause
            let new_terms = self.arena[conflict_id].terms().to_vec();
            let current_level_count = new_terms
                .iter()
                .filter(|t| self.assignments.satisfaction_level(t) == Some(current_level))
                .count();

            if current_level_count <= 1 {
                let mut target_level = 0;
                for term in &new_terms {
                    if let Some(lvl) = self.assignments.satisfaction_level(term) {
                        if lvl < current_level && lvl > target_level {
                            target_level = lvl;
                        }
                    }
                }
                self.assignments.backtrack(target_level);
                return Ok(conflict_id);
            }
        }
    }
}
