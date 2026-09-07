#![allow(clippy::pedantic, clippy::nursery)]

use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinSet;

use lux_cache::archive::wheel::extract_wheel;
use lux_cache::archive::zip_range::{parse_central_directory, EocdRecord};
use lux_cache::cas::{CasStore, EnvironmentLinker};
use lux_cache::error::CacheError;

/// Helper to construct a standard binary ZIP/wheel byte buffer in memory.
fn create_mock_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut archive = Vec::new();
    let mut cd_entries = Vec::new();

    for (name, content, deflate) in entries {
        let (compressed, method) = if *deflate {
            (miniz_oxide::deflate::compress_to_vec(content, 6), 8u16)
        } else {
            (content.to_vec(), 0u16)
        };

        let local_offset = archive.len() as u32;

        // Local file header: 30 bytes + name + payload
        archive.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // signature
        archive.extend_from_slice(&20u16.to_le_bytes()); // version needed (2.0)
        archive.extend_from_slice(&0u16.to_le_bytes()); // flags
        archive.extend_from_slice(&method.to_le_bytes()); // compression method
        archive.extend_from_slice(&0u16.to_le_bytes()); // time
        archive.extend_from_slice(&0u16.to_le_bytes()); // date
        archive.extend_from_slice(&0u32.to_le_bytes()); // crc32
        archive.extend_from_slice(&(compressed.len() as u32).to_le_bytes()); // compressed size
        archive.extend_from_slice(&(content.len() as u32).to_le_bytes()); // uncompressed size
        archive.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name length
        archive.extend_from_slice(&0u16.to_le_bytes()); // extra length
        archive.extend_from_slice(name.as_bytes()); // name
        archive.extend_from_slice(&compressed); // compressed payload

        cd_entries.push((name, local_offset, method, compressed.len() as u32, content.len() as u32));
    }

    let cd_start = archive.len() as u32;

    // Central Directory headers
    for (name, local_offset, method, comp_size, uncomp_size) in cd_entries {
        archive.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // signature
        archive.extend_from_slice(&20u16.to_le_bytes()); // version made by
        archive.extend_from_slice(&20u16.to_le_bytes()); // version needed
        archive.extend_from_slice(&0u16.to_le_bytes()); // flags
        archive.extend_from_slice(&method.to_le_bytes()); // compression method
        archive.extend_from_slice(&0u16.to_le_bytes()); // time
        archive.extend_from_slice(&0u16.to_le_bytes()); // date
        archive.extend_from_slice(&0u32.to_le_bytes()); // crc32
        archive.extend_from_slice(&comp_size.to_le_bytes()); // compressed size
        archive.extend_from_slice(&uncomp_size.to_le_bytes()); // uncompressed size
        archive.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name length
        archive.extend_from_slice(&0u16.to_le_bytes()); // extra length
        archive.extend_from_slice(&0u16.to_le_bytes()); // comment length
        archive.extend_from_slice(&0u16.to_le_bytes()); // disk num
        archive.extend_from_slice(&0u16.to_le_bytes()); // internal attr
        archive.extend_from_slice(&0u32.to_le_bytes()); // external attr
        archive.extend_from_slice(&local_offset.to_le_bytes()); // local header offset
        archive.extend_from_slice(name.as_bytes()); // name
    }

    let cd_end = archive.len() as u32;
    let cd_size = cd_end - cd_start;

    // End of Central Directory (EOCD)
    archive.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // signature
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk number
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk start of CD
    archive.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // entries this disk
    archive.extend_from_slice(&(entries.len() as u16).to_le_bytes()); // total entries
    archive.extend_from_slice(&cd_size.to_le_bytes()); // cd size
    archive.extend_from_slice(&cd_start.to_le_bytes()); // cd offset
    archive.extend_from_slice(&0u16.to_le_bytes()); // comment length

    archive
}

#[test]
fn test_zip_range_and_wheel_extraction() {
    let metadata_content = b"Metadata-Version: 2.1\nName: hyper-pkg\nVersion: 1.0.0\n";
    let init_content = b"# __init__.py\nprint('hyper initialized')\n";
    let util_content = b"# util.py\ndef compute():\n    return 42\n";

    let entries = vec![
        ("hyper_pkg-1.0.0.dist-info/METADATA", &metadata_content[..], false),
        ("hyper_pkg/__init__.py", &init_content[..], true),
        ("hyper_pkg/util.py", &util_content[..], true),
    ];

    let zip_bytes = create_mock_zip(&entries);

    // 1. Verify EOCD parsing
    let eocd = EocdRecord::parse(&zip_bytes).expect("valid EOCD");
    assert_eq!(eocd.total_entries, 3);
    assert!(eocd.cd_size > 0);
    assert!(eocd.cd_offset > 0);

    // 2. Verify Central Directory parsing
    let cd_start = eocd.cd_offset as usize;
    let cd_end = cd_start + eocd.cd_size as usize;
    let headers = parse_central_directory(&zip_bytes[cd_start..cd_end]).expect("valid central directory");
    assert_eq!(headers.len(), 3);

    let metadata_entry = headers.iter().find(|h| h.is_metadata()).expect("found metadata entry");
    assert_eq!(metadata_entry.filename, "hyper_pkg-1.0.0.dist-info/METADATA");
    assert_eq!(metadata_entry.uncompressed_size, metadata_content.len() as u64);

    // 3. Extract wheel to disk and verify files and contents
    let temp_dir = TempDir::new().expect("temp dir");
    let extracted = extract_wheel(&zip_bytes, temp_dir.path()).expect("wheel extraction succeeds");
    assert_eq!(extracted.len(), 3);

    let read_metadata = fs::read_to_string(temp_dir.path().join("hyper_pkg-1.0.0.dist-info/METADATA")).unwrap();
    assert_eq!(read_metadata, String::from_utf8_lossy(metadata_content));

    let read_init = fs::read_to_string(temp_dir.path().join("hyper_pkg/__init__.py")).unwrap();
    assert_eq!(read_init, String::from_utf8_lossy(init_content));

    let read_util = fs::read_to_string(temp_dir.path().join("hyper_pkg/util.py")).unwrap();
    assert_eq!(read_util, String::from_utf8_lossy(util_content));

    // 4. Test zip-slip vulnerability rejection
    let evil_entries = vec![
        ("../../evil.txt", &b"malicious payload"[..], false),
    ];
    let evil_zip = create_mock_zip(&evil_entries);
    let evil_dest = TempDir::new().expect("evil temp dir");
    let result = extract_wheel(&evil_zip, evil_dest.path());
    assert!(result.is_err(), "zip-slip path traversal must be rejected");
    match result.err().unwrap() {
        CacheError::InvalidZip { reason } => {
            assert!(reason.contains("path traversal"));
        }
        other => panic!("expected InvalidZip error, got {other:?}"),
    }
}

#[test]
fn test_cas_store_blob_and_hash_verification() {
    let temp_dir = TempDir::new().expect("temp dir");
    let store = CasStore::open(Some(temp_dir.path().to_path_buf())).expect("opened CAS store");

    let sample_data = b"Hyper-optimized Rust wheel binary stream with SHA-256 validation.";
    let expected_hash = CasStore::compute_sha256(sample_data);

    // 1. Initial state: blob not present
    assert!(!store.contains_blob(&expected_hash));

    // 2. Store blob
    let (computed_hash, blob_path) = store.store_blob(sample_data).expect("blob stored successfully");
    assert_eq!(computed_hash, expected_hash);
    assert!(blob_path.is_file());
    assert!(store.contains_blob(&expected_hash));

    // 3. Retrieve blob and compare bytes
    let fetched = store.get_blob(&expected_hash).expect("retrieved blob");
    assert_eq!(fetched, sample_data);

    // 4. Store with hash validation
    let validated_path = store.store_blob_with_hash(sample_data, &expected_hash)
        .expect("hash validation matches");
    assert_eq!(validated_path, blob_path);

    // 5. Store with corrupt / mismatched hash
    let corrupt_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let mismatch = store.store_blob_with_hash(sample_data, corrupt_hash);
    assert!(mismatch.is_err(), "mismatched hash must return error");
    match mismatch.err().unwrap() {
        CacheError::ChecksumMismatch { expected, computed, .. } => {
            assert_eq!(expected, corrupt_hash);
            assert_eq!(computed, expected_hash);
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

#[test]
fn test_cas_zero_copy_linking() {
    let temp_cas = TempDir::new().expect("cas temp dir");
    let store = CasStore::open(Some(temp_cas.path().to_path_buf())).expect("opened CAS");

    let entries = vec![
        ("pkg_demo/__init__.py", &b"# pkg demo init\n"[..], false),
        ("pkg_demo/core.py", &b"def run(): pass\n"[..], true),
        ("pkg_demo-0.1.0.dist-info/METADATA", &b"Name: pkg_demo\nVersion: 0.1.0\n"[..], false),
    ];
    let wheel_bytes = create_mock_zip(&entries);

    // 1. Store extracted wheel into CAS
    let (sha256_hex, extracted_path) = store.store_extracted_wheel(&wheel_bytes).expect("wheel stored and extracted");
    assert!(store.contains_extracted(&sha256_hex));
    assert!(extracted_path.is_dir());

    // 2. Project into mock virtual environment site-packages
    let temp_env = TempDir::new().expect("env temp dir");
    let site_packages = temp_env.path().join("lib").join("python3.12").join("site-packages");

    let report = EnvironmentLinker::link_tree(&extracted_path, &site_packages).expect("linked successfully");
    assert_eq!(report.total_files(), 3);
    assert!(report.total_bytes > 0);

    // 3. Verify files exist in target environment
    assert!(site_packages.join("pkg_demo/__init__.py").is_file());
    assert!(site_packages.join("pkg_demo/core.py").is_file());
    assert!(site_packages.join("pkg_demo-0.1.0.dist-info/METADATA").is_file());

    let linked_init = fs::read(site_packages.join("pkg_demo/__init__.py")).unwrap();
    assert_eq!(linked_init, b"# pkg demo init\n");

    // 4. Test idempotency (re-linking does not fail)
    let re_report = EnvironmentLinker::link_tree(&extracted_path, &site_packages).expect("re-link succeeds");
    assert_eq!(re_report.total_files(), 3);
}

#[tokio::test]
async fn test_concurrency_stress_20_parallel_tasks() {
    let temp_cas = TempDir::new().expect("cas temp dir");
    let store = Arc::new(CasStore::open(Some(temp_cas.path().to_path_buf())).expect("opened CAS"));

    let entries = vec![
        ("stress_pkg/__init__.py", &b"# stress test package\n"[..], true),
        ("stress_pkg/math.py", &b"def add(x, y): return x + y\n"[..], true),
        ("stress_pkg/data.bin", &[42u8; 4096][..], true),
        ("stress_pkg-2.5.0.dist-info/METADATA", &b"Name: stress_pkg\nVersion: 2.5.0\n"[..], false),
    ];
    let wheel_bytes = Arc::new(create_mock_zip(&entries));
    let raw_payload = Arc::new(vec![0x7fu8; 16384]);

    let temp_envs_root = TempDir::new().expect("envs root");
    let envs_root_path = Arc::new(temp_envs_root.path().to_path_buf());

    let mut join_set = JoinSet::new();

    // Spawn 20 concurrent tasks racing on the exact same CAS store and artifacts
    for task_id in 0..20 {
        let store_clone = Arc::clone(&store);
        let wheel_clone = Arc::clone(&wheel_bytes);
        let payload_clone = Arc::clone(&raw_payload);
        let env_dir = envs_root_path.join(format!("venv_{task_id}"));

        join_set.spawn(async move {
            // Race to store blob
            let (blob_hash, blob_path) = store_clone.store_blob(&payload_clone).expect("store blob");
            assert!(blob_path.is_file());

            // Race to extract wheel into CAS
            let (_wheel_hash, extracted_dir) = store_clone.store_extracted_wheel(&wheel_clone).expect("store wheel");
            assert!(extracted_dir.is_dir());

            // Link into task's private virtual environment
            let venv_site_packages = env_dir.join("Lib").join("site-packages");
            let report = EnvironmentLinker::link_tree(&extracted_dir, &venv_site_packages).expect("link tree");

            assert_eq!(report.total_files(), 4);
            assert!(venv_site_packages.join("stress_pkg/__init__.py").is_file());
            assert!(venv_site_packages.join("stress_pkg/math.py").is_file());
            assert!(venv_site_packages.join("stress_pkg/data.bin").is_file());
            assert!(venv_site_packages.join("stress_pkg-2.5.0.dist-info/METADATA").is_file());

            // Verify content
            let read_bytes = fs::read(venv_site_packages.join("stress_pkg/data.bin")).unwrap();
            assert_eq!(read_bytes.len(), 4096);
            assert_eq!(read_bytes[0], 42);

            blob_hash
        });
    }

    let mut completed = 0;
    while let Some(res) = join_set.join_next().await {
        let _hash = res.expect("task panicked or failed");
        completed += 1;
    }

    assert_eq!(completed, 20, "all 20 concurrent tasks completed successfully");

    // Verify CAS integrity: exactly one extracted package dir and one blob file
    let wheel_hash = CasStore::compute_sha256(&wheel_bytes);
    let payload_hash = CasStore::compute_sha256(&raw_payload);

    assert!(store.contains_extracted(&wheel_hash));
    assert!(store.contains_blob(&payload_hash));
}
