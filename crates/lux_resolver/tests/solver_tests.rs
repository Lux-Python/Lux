use std::str::FromStr;
use lux_core::types::{PackageName, Requirement, Version};
use lux_resolver::{MemoryDependencyProvider, PubGrubSolver, ResolverError};

#[test]
fn test_linear_dependencies() {
    let mut provider = MemoryDependencyProvider::new();
    provider.add_package("pkg-a", &["1.0.0", "1.1.0"]);
    provider.add_package("pkg-b", &["2.0.0", "2.1.0"]);
    provider.add_package("pkg-c", &["3.0.0", "3.5.0"]);

    provider.add_dependency("pkg-a", "1.1.0", "pkg-b >= 2.0.0");
    provider.add_dependency("pkg-b", "2.1.0", "pkg-c >= 3.0.0");

    let mut solver = PubGrubSolver::new(&provider);
    let root_req = Requirement::from_str("pkg-a >= 1.0.0").unwrap();

    let solution = solver.resolve(&[root_req]).expect("resolution should succeed");

    assert_eq!(
        solution.get(&PackageName::new("pkg-a").unwrap()),
        Some(&Version::from_str("1.1.0").unwrap())
    );
    assert_eq!(
        solution.get(&PackageName::new("pkg-b").unwrap()),
        Some(&Version::from_str("2.1.0").unwrap())
    );
    assert_eq!(
        solution.get(&PackageName::new("pkg-c").unwrap()),
        Some(&Version::from_str("3.5.0").unwrap())
    );
}

#[test]
fn test_diamond_dependency_resolution() {
    let mut provider = MemoryDependencyProvider::new();
    provider.add_package("pkg-a", &["1.0.0"]);
    provider.add_package("pkg-b", &["1.0.0"]);
    provider.add_package("pkg-c", &["1.0.0"]);
    provider.add_package("pkg-d", &["1.0.0", "1.2.0", "1.5.0", "1.8.0", "2.0.0", "2.5.0"]);

    // Diamond: root -> A, root -> B
    // A -> D >= 1.0.0, < 2.0.0
    // B -> D >= 1.5.0, < 3.0.0
    provider.add_dependency("pkg-a", "1.0.0", "pkg-d >= 1.0.0, < 2.0.0");
    provider.add_dependency("pkg-b", "1.0.0", "pkg-d >= 1.5.0, < 3.0.0");

    let mut solver = PubGrubSolver::new(&provider);
    let req_a = Requirement::from_str("pkg-a == 1.0.0").unwrap();
    let req_b = Requirement::from_str("pkg-b == 1.0.0").unwrap();

    let solution = solver.resolve(&[req_a, req_b]).expect("diamond should resolve");

    // Highest version of D satisfying both [1.0.0, 2.0.0) and [1.5.0, 3.0.0) is 1.8.0
    assert_eq!(
        solution.get(&PackageName::new("pkg-d").unwrap()),
        Some(&Version::from_str("1.8.0").unwrap())
    );
}

#[test]
fn test_cyclic_dependency_trap() {
    let mut provider = MemoryDependencyProvider::new();
    provider.add_package("pkg-a", &["1.0.0"]);
    provider.add_package("pkg-b", &["1.0.0"]);

    // Cyclic: A -> B, B -> A
    provider.add_dependency("pkg-a", "1.0.0", "pkg-b >= 1.0.0");
    provider.add_dependency("pkg-b", "1.0.0", "pkg-a >= 1.0.0");

    let mut solver = PubGrubSolver::new(&provider);
    let req_a = Requirement::from_str("pkg-a >= 1.0.0").unwrap();

    let solution = solver.resolve(&[req_a]).expect("cyclic graph should resolve safely");

    assert_eq!(
        solution.get(&PackageName::new("pkg-a").unwrap()),
        Some(&Version::from_str("1.0.0").unwrap())
    );
    assert_eq!(
        solution.get(&PackageName::new("pkg-b").unwrap()),
        Some(&Version::from_str("1.0.0").unwrap())
    );
}

#[test]
fn test_unresolvable_conflict_diagnostic() {
    let mut provider = MemoryDependencyProvider::new();
    provider.add_package("pkg-a", &["1.0.0"]);
    provider.add_package("pkg-b", &["1.0.0"]);
    provider.add_package("pkg-c", &["0.9.0", "1.5.0", "2.1.0"]);

    // Incompatible transitive requirements:
    // A -> C >= 2.0.0
    // B -> C < 1.0.0
    provider.add_dependency("pkg-a", "1.0.0", "pkg-c >= 2.0.0");
    provider.add_dependency("pkg-b", "1.0.0", "pkg-c < 1.0.0");

    let mut solver = PubGrubSolver::new(&provider);
    let req_a = Requirement::from_str("pkg-a == 1.0.0").unwrap();
    let req_b = Requirement::from_str("pkg-b == 1.0.0").unwrap();

    let result = solver.resolve(&[req_a, req_b]);

    match result {
        Err(ResolverError::UnresolvableConflict { report }) => {
            // Verify DAG diagnostic synthesis precision
            assert!(report.contains("conflict graph"), "Report missing graph section: {report}");
            assert!(report.contains("pkg-a==1.0.0 depends on pkg-c"), "Report missing pkg-a edge: {report}");
            assert!(report.contains("pkg-b==1.0.0 depends on pkg-c"), "Report missing pkg-b edge: {report}");
            assert!(report.contains("Remediation suggestions"), "Report missing remediation: {report}");
        }
        res => panic!("Expected UnresolvableConflict error, got: {res:?}"),
    }
}
