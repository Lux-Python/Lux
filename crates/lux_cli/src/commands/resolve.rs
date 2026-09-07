use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;
use miette::Result;

use lux_core::manifest::PyProjectToml;
use lux_core::types::Requirement;
use lux_resolver::PubGrubSolver;

use super::super::ui::{render_dependency_tree, DependencyNode, Status, Style};

/// Execute the `lux resolve` / `lux tree` command.
pub fn run_resolve(packages: &[String], show_tree: bool) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let reqs = if packages.is_empty() {
        let manifest_path = cwd.join("pyproject.toml");
        if !manifest_path.is_file() {
            return Err(miette::miette!(
                "No packages specified and no pyproject.toml found in {}.",
                cwd.display()
            ));
        }
        let manifest = PyProjectToml::from_file(&manifest_path)
            .map_err(|e| miette::miette!("Failed to read pyproject.toml: {e}"))?;
        manifest
            .parsed_requirements()
            .map_err(|e| miette::miette!("Failed to parse requirements: {e}"))?
    } else {
        let mut parsed = Vec::new();
        for p in packages {
            let req = Requirement::from_str(p).map_err(|e| miette::miette!("Invalid requirement '{p}': {e}"))?;
            parsed.push(req);
        }
        parsed
    };

    if reqs.is_empty() {
        Status::info("Empty", "No dependencies declared to resolve.");
        return Ok(());
    }

    let provider = crate::provider::RegistryDependencyProvider::new(None)
        .map_err(|e| miette::miette!("Failed to initialize dependency provider: {e}"))?;
    let p_clone = provider.clone();
    let r_clone = reqs.clone();
    crate::provider::run_async_safe(async move {
        p_clone.prefetch_or_fallback(&r_clone).await;
    });

    let solve_start = Instant::now();
    let mut solver = PubGrubSolver::new(&provider);
    let solution = solver
        .resolve(&reqs)
        .map_err(|e| miette::miette!("PubGrub SAT resolution failed: {e}"))?;
    let duration = solve_start.elapsed();

    let root_name = cwd.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    let mut root_node = DependencyNode::new(root_name, "0.1.0");

    for (pkg_name, version) in &solution {
        let node = DependencyNode::new(pkg_name.as_str(), version.to_string());
        root_node.add_child(node);
    }

    if show_tree {
        render_dependency_tree(&root_node);
    } else {
        for (pkg_name, version) in &solution {
            println!("   {} v{}", Style::bold(pkg_name.as_str()), Style::cyan(&version.to_string()));
        }
    }
    println!();

    Status::completed("Resolved", &format!("{} package(s)", solution.len()), Some(duration));

    Ok(())
}
