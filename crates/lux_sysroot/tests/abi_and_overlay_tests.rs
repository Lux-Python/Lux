//! Tests for ABI dynamic inspector and sysroot overlay engine.

use std::path::Path;
use lux_sysroot::abi::{AbiInspector, BinaryFormat};
use lux_sysroot::overlay::SysrootOverlay;

#[test]
fn test_sysroot_overlay_directory_structure_and_installation() {
    let temp = tempfile::tempdir().unwrap();
    let sysroot = SysrootOverlay::new(temp.path());

    sysroot.ensure_directories().unwrap();
    assert!(sysroot.lib_dir().exists());
    assert!(sysroot.include_dir().exists());
    assert!(sysroot.bin_dir().exists());
    assert!(sysroot.pkgconfig_dir().exists());

    // Test library installation
    let lib_bytes = b"fake-shared-library-data";
    let lib_path = sysroot.install_library("libtest.so", lib_bytes).unwrap();
    assert!(lib_path.exists());
    assert_eq!(std::fs::read(&lib_path).unwrap(), lib_bytes);

    // Test header installation
    let header_bytes = b"#pragma once\nvoid test_symbol(void);\n";
    let header_path = sysroot.install_header(Path::new("lux/test.h"), header_bytes).unwrap();
    assert!(header_path.exists());
    assert_eq!(std::fs::read(&header_path).unwrap(), header_bytes);

    // Test compiler flags
    let flags = sysroot.compiler_flags();
    assert!(flags.cflags.contains("-I"));
    assert!(flags.ldflags.contains("-L"));

    // Test environment overrides
    let env = sysroot.environment_overrides();
    assert!(env.contains_key("CFLAGS"));
    assert!(env.contains_key("LDFLAGS"));
    assert!(env.contains_key("PATH"));
}

#[test]
fn test_abi_inspector_current_executable() {
    let current_exe = std::env::current_exe().unwrap();
    let inspector = AbiInspector::new();

    let report = inspector.inspect_file(&current_exe).unwrap();
    assert_eq!(report.path, current_exe);
    assert!(report.format == BinaryFormat::Pe || report.format == BinaryFormat::Elf || report.format == BinaryFormat::MachO);
    assert!(!report.architecture.is_empty());

    // On Windows, the test runner executable will depend on KERNEL32.dll and msvcrt/ntdll
    println!("Inspected binary format: {}", report.format);
    println!("Architecture: {}", report.architecture);
    println!("Dynamic dependencies found: {}", report.dependencies.len());
    for dep in &report.dependencies {
        println!("  dep: {} (system: {}, resolved: {:?})", dep.name, dep.is_system, dep.resolved_path);
    }
}

#[test]
fn test_abi_inspector_custom_search_path() {
    let temp = tempfile::tempdir().unwrap();
    let fake_lib = temp.path().join("libcustom_cuda.so");
    std::fs::write(&fake_lib, b"CUDA STUB").unwrap();

    let mut inspector = AbiInspector::new();
    inspector.add_search_path(temp.path());

    // Verify scan directory with empty / non-native directory returns cleanly
    let reports = inspector.scan_directory(temp.path()).unwrap();
    assert!(reports.is_empty());
}
