//! Tests for PEP 723 script metadata and .python-version management.

use lux_core::env::PythonVersionFile;
use lux_core::manifest::ScriptMetadata;

#[test]
fn test_pep723_script_metadata_parsing() {
    let script = r#"# Standalone test script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "requests>=2.31.0",
#     "rich>=13.0.0",
# ]
# ///

import requests
import rich

print("Hello world")
"#;

    let meta = ScriptMetadata::parse(script).unwrap().expect("metadata present");
    assert_eq!(meta.requires_python(), Some(">=3.11"));
    assert_eq!(meta.dependencies().len(), 2);
    assert_eq!(meta.dependencies()[0], "requests>=2.31.0");
    assert_eq!(meta.dependencies()[1], "rich>=13.0.0");
}

#[test]
fn test_pep723_script_without_metadata() {
    let script = r#"import sys
print("No PEP 723 metadata here")
"#;

    let meta = ScriptMetadata::parse(script).unwrap();
    assert!(meta.is_none());
}

#[test]
fn test_python_version_file_read_write_walk() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    // No .python-version initially
    assert!(PythonVersionFile::read_from(root).is_none());
    assert!(PythonVersionFile::find_upwards(root).is_none());

    // Write .python-version
    let path = PythonVersionFile::write_to(root, "3.12.3").unwrap();
    assert!(path.is_file());

    assert_eq!(PythonVersionFile::read_from(root).as_deref(), Some("3.12.3"));

    // Check nested directory finds it upwards
    let sub = root.join("nested").join("deep");
    std::fs::create_dir_all(&sub).unwrap();

    let found = PythonVersionFile::find_upwards(&sub).expect("found upwards");
    assert_eq!(found.1, "3.12.3");
}
