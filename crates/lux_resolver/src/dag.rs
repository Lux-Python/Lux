use id_arena::Arena;
use std::fmt::Write;

use super::incompatibility::{Incompatibility, IncompatibilityCause, IncompatibilityId};
use super::term::PackageRegistry;

/// Directed Acyclic Graph tracing the causal derivations that produced an unresolvable contradiction.
#[derive(Debug)]
pub struct ConflictDag {
    root_conflict: IncompatibilityId,
}

impl ConflictDag {
    pub const fn new(root_conflict: IncompatibilityId) -> Self {
        Self { root_conflict }
    }

    /// Synthesizes a human-readable diagnosis and remediation report from the conflict graph.
    #[must_use]
    pub fn build_remediation_report(
        &self,
        arena: &Arena<Incompatibility>,
        registry: &PackageRegistry,
    ) -> String {
        let mut report = String::new();
        let _ = writeln!(report, "Direct dependency conflict graph:");

        let mut visited = Vec::new();
        Self::render_node(self.root_conflict, arena, registry, &mut report, "", true, &mut visited);

        let _ = writeln!(report, "\nRemediation suggestions:");
        let _ = writeln!(
            report,
            "  * Check for mutually exclusive version ranges between transitive dependencies."
        );
        let _ = writeln!(
            report,
            "  * Consider loosening upper bounds or upgrading companion packages to allow compatible releases."
        );

        report
    }

    fn render_node(
        id: IncompatibilityId,
        arena: &Arena<Incompatibility>,
        registry: &PackageRegistry,
        out: &mut String,
        prefix: &str,
        is_last: bool,
        visited: &mut Vec<IncompatibilityId>,
    ) {
        if visited.contains(&id) {
            return;
        }
        visited.push(id);

        let incomp = &arena[id];
        let branch = if is_last { "└── " } else { "├── " };

        match incomp.cause() {
            IncompatibilityCause::Root => {
                let _ = writeln!(out, "{prefix}{branch}Root project specifications");
            }
            IncompatibilityCause::Dependency { package, version, target } => {
                let pkg_name = registry.name(*package).unwrap_or("unknown");
                let target_name = registry.name(*target).unwrap_or("unknown");
                let _ = writeln!(
                    out,
                    "{prefix}{branch}{pkg_name}=={version} depends on {target_name}"
                );
            }
            IncompatibilityCause::NoVersions { package } => {
                let pkg_name = registry.name(*package).unwrap_or("unknown");
                let _ = writeln!(
                    out,
                    "{prefix}{branch}No available versions found for package '{pkg_name}'"
                );
            }
            IncompatibilityCause::Conflict { conflict, other } => {
                let _ = writeln!(out, "{prefix}{branch}Conflicting requirement branch:");
                let next_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
                Self::render_node(*conflict, arena, registry, out, &next_prefix, false, visited);
                Self::render_node(*other, arena, registry, out, &next_prefix, true, visited);
            }
        }
    }
}
