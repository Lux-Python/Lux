#![allow(clippy::pedantic, clippy::nursery)]

use std::env;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::sync::Mutex;

use lux_cli::commands::{
    run_add, run_cache, run_doctor, run_init, run_remove, run_resolve, run_sync,
};
use lux_core::manifest::lockfile::LuxLockfile;
use lux_core::manifest::PyProjectToml;

static PROCESS_DIR_LOCK: Mutex<()> = Mutex::const_new(());

struct CwdGuard {
    original_cwd: PathBuf,
}

impl CwdGuard {
    fn switch_to(new_dir: &Path) -> Self {
        let original_cwd = env::current_dir().expect("valid original cwd");
        env::set_current_dir(new_dir).expect("switch to temp cwd");
        Self { original_cwd }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.original_cwd);
    }
}

#[tokio::test]
async fn test_cli_full_lifecycle_e2e() {
    let _lock = PROCESS_DIR_LOCK.lock().await;

    let temp_workspace = TempDir::new().expect("temp workspace");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    // 1. Test `lux init`
    run_init(None, Some("hermetic-app".to_string())).expect("init succeeds");

    let manifest_path = temp_workspace.path().join("pyproject.toml");
    assert!(manifest_path.is_file(), "pyproject.toml must exist");

    let manifest = PyProjectToml::from_file(&manifest_path).expect("valid manifest");
    assert_eq!(manifest.name(), "hermetic-app");
    assert_eq!(manifest.version(), Some("0.1.0"));

    let venv_dir = temp_workspace.path().join(".venv");
    assert!(venv_dir.is_dir(), ".venv must exist");
    assert!(venv_dir.join("pyvenv.cfg").is_file(), "pyvenv.cfg must exist");

    // 2. Test `lux add`
    let packages_to_add = vec![
        "requests>=2.31.0".to_string(),
        "urllib3>=2.0.0".to_string(),
    ];
    run_add(&packages_to_add, None, false).await.expect("add succeeds");

    // Verify pyproject.toml updated
    let updated_manifest = PyProjectToml::from_file(&manifest_path).expect("read updated manifest");
    assert_eq!(updated_manifest.dependencies().len(), 2);
    assert!(updated_manifest.dependencies().contains(&"requests>=2.31.0".to_string()));
    assert!(updated_manifest.dependencies().contains(&"urllib3>=2.0.0".to_string()));

    // Verify lux.lock created and populated
    let lock_path = temp_workspace.path().join("lux.lock");
    assert!(lock_path.is_file(), "lux.lock must exist");
    let lockfile = LuxLockfile::from_file(&lock_path).expect("valid lockfile");
    assert_eq!(lockfile.packages.len(), 2);
    assert!(lockfile.find_package("requests").is_some());
    assert!(lockfile.find_package("urllib3").is_some());

    // Verify virtual environment site-packages contains linked packages
    let (site_packages_dir, _) = if cfg!(windows) {
        (venv_dir.join("Lib").join("site-packages"), venv_dir.join("Scripts"))
    } else {
        (venv_dir.join("lib").join("site-packages"), venv_dir.join("bin"))
    };

    assert!(site_packages_dir.join("requests").join("__init__.py").is_file());
    assert!(site_packages_dir.join("urllib3").join("__init__.py").is_file());

    // 3. Test `lux resolve --tree`
    run_resolve(&[], true).expect("resolve --tree succeeds");

    // 4. Test `lux sync`
    run_sync().expect("sync succeeds");
    assert!(site_packages_dir.join("requests").join("__init__.py").is_file());
    assert!(site_packages_dir.join("urllib3").join("__init__.py").is_file());

    // 5. Test `lux remove`
    run_remove(&["requests".to_string()]).expect("remove succeeds");

    let post_remove_manifest = PyProjectToml::from_file(&manifest_path).expect("read manifest");
    assert_eq!(post_remove_manifest.dependencies().len(), 1);
    assert_eq!(post_remove_manifest.dependencies()[0], "urllib3>=2.0.0");

    let post_remove_lock = LuxLockfile::from_file(&lock_path).expect("read lock");
    assert_eq!(post_remove_lock.packages.len(), 1);
    assert!(post_remove_lock.find_package("requests").is_none());
    assert!(post_remove_lock.find_package("urllib3").is_some());

    assert!(!site_packages_dir.join("requests").exists(), "requests directory should be unlinked");
    assert!(site_packages_dir.join("urllib3").join("__init__.py").is_file(), "urllib3 remains intact");

    // 6. Test `lux cache`
    run_cache(None).expect("cache telemetry succeeds");
    run_cache(Some("dir")).expect("cache dir succeeds");
    let cache_layout = lux_cache::cas::CasLayout::new(lux_cache::cas::CasLayout::default_root());
    assert!(cache_layout.root().is_dir(), "cache root directory should exist");
    assert!(cache_layout.blobs_dir().is_dir(), "blobs directory should exist");

    run_cache(Some("clean")).expect("cache clean succeeds");
    assert!(cache_layout.tmp_dir().is_dir(), "staging directory should exist after clean");

    // 7. Test `lux doctor` (empty site-packages)
    run_doctor().expect("doctor diagnostics succeeds");

    // 8. Test `lux doctor` with native extension binary in site-packages
    let current_exe = env::current_exe().expect("current exe");
    let ext_name = if cfg!(windows) { "_native_accel.pyd" } else { "_native_accel.so" };
    let mock_ext = site_packages_dir.join(ext_name);
    std::fs::copy(&current_exe, &mock_ext).expect("copy native extension");
    assert!(mock_ext.is_file(), "native binary extension should exist for doctor inspection");
    run_doctor().expect("doctor diagnostics with native binary succeeds");
    let _ = std::fs::remove_file(mock_ext);
}
