//! Tests for Phase 7 commands: requirements.txt (-r), editable installs (-e),
//! lockfile export, shell completions, monorepo workspaces, and multi-platform lockfiles.

#![allow(clippy::pedantic, clippy::nursery)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::sync::Mutex;

use lux_cli::commands::{
    run_add, run_completions, run_export, run_init, run_pip_install, run_sync,
};
use lux_core::env::VirtualEnv;
use lux_core::manifest::lockfile::{LockedPackage, LuxLockfile, PackageHash};
use lux_core::manifest::PyProjectToml;

static TEST_LOCK: Mutex<()> = Mutex::const_new(());

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
async fn test_phase7_pip_requirements_file() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("create tempdir");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    run_init(None, Some("req-test-app".to_string())).expect("init succeeds");

    // Write a requirements.txt with comments and dependencies
    let req_content = r#"
# Core application dependencies
requests>=2.0.0 # HTTP client
flask>=2.0.0
"#;
    let req_file = temp_workspace.path().join("requirements.txt");
    fs::write(&req_file, req_content).expect("write requirements.txt");

    // Run `lux pip install -r requirements.txt`
    run_pip_install(&[], &[req_file], &[])
        .await
        .expect("pip install from requirements.txt succeeds");

    let venv = VirtualEnv::find(temp_workspace.path()).expect("find venv");
    let venv_site = venv.site_packages_dir();
    assert!(venv_site.join("requests").is_dir());
    assert!(venv_site.join("flask").is_dir());
}

#[tokio::test]
async fn test_phase7_editable_install() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("create tempdir");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    run_init(None, Some("editable-host".to_string())).expect("init succeeds");

    // Create a local package to install as editable
    let local_pkg_dir = temp_workspace.path().join("my_local_lib");
    fs::create_dir_all(&local_pkg_dir).expect("create local pkg dir");

    let local_pyproject = r#"
    [project]
    name = "my-local-lib"
    version = "0.3.0"
    "#;
    fs::write(local_pkg_dir.join("pyproject.toml"), local_pyproject).expect("write local pyproject");
    fs::write(local_pkg_dir.join("__init__.py"), b"__version__ = '0.3.0'").expect("write local init");

    // Run `lux pip install -e ./my_local_lib`
    run_pip_install(&[], &[], std::slice::from_ref(&local_pkg_dir))
        .await
        .expect("editable install succeeds");

    let venv = VirtualEnv::find(temp_workspace.path()).expect("find venv");
    let venv_site = venv.site_packages_dir();
    let pth_file = venv_site.join("my_local_lib.pth");
    assert!(
        pth_file.is_file() || venv_site.join("my-local-lib.pth").is_file(),
        "my_local_lib.pth must exist"
    );

    let dist_info = venv_site.join("my-local-lib-0.3.0.dist-info");
    assert!(dist_info.is_dir());
    assert!(dist_info.join("direct_url.json").is_file());
}

#[tokio::test]
async fn test_phase7_add_editable() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("create tempdir");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    run_init(None, Some("add-editable-app".to_string())).expect("init succeeds");

    // Create a local package
    let local_dir = temp_workspace.path().join("local_tool");
    fs::create_dir_all(&local_dir).expect("create local tool dir");
    fs::write(
        local_dir.join("pyproject.toml"),
        r#"
        [project]
        name = "local-tool"
        version = "1.0.0"
        "#,
    )
    .expect("write tool pyproject");

    // Run `lux add -e ./local_tool`
    run_add(&[], Some(&local_dir), false)
        .await
        .expect("add editable succeeds");

    let manifest = PyProjectToml::from_file(&temp_workspace.path().join("pyproject.toml"))
        .expect("read manifest");
    assert!(manifest.dependencies().contains(&"local-tool".to_string()));

    let venv = VirtualEnv::find(temp_workspace.path()).expect("find venv");
    let venv_site = venv.site_packages_dir();
    assert!(
        venv_site.join("local_tool.pth").is_file() || venv_site.join("local-tool.pth").is_file(),
        "local_tool.pth must exist"
    );
}

#[tokio::test]
async fn test_phase7_export_lockfile() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("create tempdir");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    run_init(None, Some("export-app".to_string())).expect("init succeeds");

    // Create a lux.lock with hashes
    let mut lockfile = LuxLockfile::new();
    lockfile.add_or_update_package(LockedPackage {
        name: "requests".to_string(),
        version: "2.31.0".to_string(),
        dependencies: vec![],
        hashes: vec![PackageHash {
            algorithm: "sha256".to_string(),
            digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        }],
    });
    lockfile.add_or_update_package(LockedPackage {
        name: "urllib3".to_string(),
        version: "2.0.7".to_string(),
        dependencies: vec![],
        hashes: vec![PackageHash {
            algorithm: "sha256".to_string(),
            digest: "1b9a9d20c5d26ff38a4d46c4f039a8db4d0f62b1b369c3a078ec39fb7cb6a0df".to_string(),
        }],
    });
    lockfile
        .save_to_file(&temp_workspace.path().join("lux.lock"))
        .expect("save lockfile");

    // Test export with hashes
    let out_txt = temp_workspace.path().join("exported_requirements.txt");
    run_export("requirements-txt", Some(out_txt.clone()), false).expect("export succeeds");

    assert!(out_txt.is_file());
    let content = fs::read_to_string(&out_txt).expect("read exported");
    assert!(content.contains("requests==2.31.0"));
    assert!(content.contains("--hash=sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    assert!(content.contains("urllib3==2.0.7"));

    // Test export with --no-hashes
    let out_no_hashes = temp_workspace.path().join("no_hashes.txt");
    run_export("requirements-txt", Some(out_no_hashes.clone()), true).expect("export no hashes succeeds");
    let content_no_hashes = fs::read_to_string(&out_no_hashes).expect("read no hashes");
    assert!(content_no_hashes.contains("requests==2.31.0"));
    assert!(!content_no_hashes.contains("--hash="));
}

#[tokio::test]
async fn test_phase7_completions() {
    let mut cmd = clap::Command::new("lux");
    // Verify generation for all major shells completes without panicking
    run_completions(clap_complete::Shell::PowerShell, &mut cmd).expect("powershell completions");
    let mut cmd_bash = clap::Command::new("lux");
    run_completions(clap_complete::Shell::Bash, &mut cmd_bash).expect("bash completions");
    let mut cmd_zsh = clap::Command::new("lux");
    run_completions(clap_complete::Shell::Zsh, &mut cmd_zsh).expect("zsh completions");
    let mut cmd_fish = clap::Command::new("lux");
    run_completions(clap_complete::Shell::Fish, &mut cmd_fish).expect("fish completions");
}

#[tokio::test]
async fn test_phase7_monorepo_workspace_sync() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("create tempdir");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    // Root project with workspace
    let root_toml = r#"
    [project]
    name = "mono-root"
    version = "0.1.0"

    [tool.lux.workspace]
    members = ["packages/*"]
    "#;
    fs::write(temp_workspace.path().join("pyproject.toml"), root_toml).expect("write root pyproject");

    // Member 1: packages/core
    let pkg_core = temp_workspace.path().join("packages").join("core");
    fs::create_dir_all(&pkg_core).expect("create packages/core");
    fs::write(
        pkg_core.join("pyproject.toml"),
        r#"
        [project]
        name = "mono-core"
        version = "1.0.0"
        "#,
    )
    .expect("write core pyproject");

    // Member 2: packages/web
    let pkg_web = temp_workspace.path().join("packages").join("web");
    fs::create_dir_all(&pkg_web).expect("create packages/web");
    fs::write(
        pkg_web.join("pyproject.toml"),
        r#"
        [project]
        name = "mono-web"
        version = "2.0.0"
        "#,
    )
    .expect("write web pyproject");

    // Run `lux sync`
    run_sync().expect("workspace sync succeeds");

    let venv = VirtualEnv::find(temp_workspace.path()).expect("find venv");
    let venv_site = venv.site_packages_dir();
    assert!(
        venv_site.join("mono_core.pth").is_file() || venv_site.join("mono-core.pth").is_file(),
        "mono_core.pth must be linked"
    );
    assert!(
        venv_site.join("mono_web.pth").is_file() || venv_site.join("mono-web.pth").is_file(),
        "mono_web.pth must be linked"
    );

    let core_pth = if venv_site.join("mono_core.pth").is_file() {
        fs::read_to_string(venv_site.join("mono_core.pth")).expect("read core pth")
    } else {
        fs::read_to_string(venv_site.join("mono-core.pth")).expect("read core pth")
    };
    assert!(core_pth.contains("core"));
}

#[test]
fn test_phase7_multiplatform_lockfile_metadata() {
    let mut lockfile = LuxLockfile::new();
    lockfile.add_target("x86_64-pc-windows-msvc");
    lockfile.add_target("x86_64-unknown-linux-gnu");
    lockfile.add_target("aarch64-apple-darwin");

    assert!(lockfile.supports_target("x86_64-pc-windows-msvc"));
    assert!(lockfile.supports_target("x86_64-unknown-linux-gnu"));
    assert!(lockfile.supports_target("aarch64-apple-darwin"));
    assert!(!lockfile.supports_target("wasm32-unknown-unknown"));

    let serialized = toml::to_string(&lockfile).expect("serialize lockfile");
    assert!(serialized.contains("targets = ["));
    assert!(serialized.contains("x86_64-pc-windows-msvc"));

    let parsed = LuxLockfile::parse(&serialized).expect("parse lockfile");
    assert_eq!(parsed.targets.len(), 3);
    assert_eq!(parsed.targets[0], "aarch64-apple-darwin");
}
