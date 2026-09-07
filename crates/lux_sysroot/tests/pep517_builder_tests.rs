//! Tests for PEP 517 build orchestrator.

use std::path::Path;
use lux_sysroot::builder::Pep517Builder;
use lux_sysroot::overlay::SysrootOverlay;

#[test]
fn test_pep517_builder_manifest_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let pyproject = r#"
[build-system]
requires = ["flit_core >=3.8,<4"]
build-backend = "flit_core.buildapi"

[project]
name = "demo-pkg"
version = "0.1.0"
"#;
    std::fs::write(temp.path().join("pyproject.toml"), pyproject).unwrap();

    let builder = Pep517Builder::from_source_dir(temp.path()).unwrap();
    let config = builder.config();
    assert_eq!(config.build_backend, "flit_core.buildapi");
    assert_eq!(config.build_requires, vec!["flit_core >=3.8,<4"]);

    // Test runner script generation
    let out_dir = Path::new("dist");
    let script = builder.generate_runner_script("build_wheel", out_dir);
    assert!(script.contains("flit_core.buildapi"));
    assert!(script.contains("build_wheel"));
}

#[test]
fn test_pep517_builder_legacy_fallback() {
    let temp = tempfile::tempdir().unwrap();
    // Directory without pyproject.toml
    let builder = Pep517Builder::from_source_dir(temp.path()).unwrap();
    let config = builder.config();
    assert_eq!(config.build_backend, "setuptools.build_meta");
    assert_eq!(config.build_requires, vec!["setuptools", "wheel"]);

    // Test attaching sysroot
    let sysroot_dir = temp.path().join("sysroot");
    let sysroot = SysrootOverlay::new(sysroot_dir);
    let _builder = builder.with_sysroot(sysroot);
}
