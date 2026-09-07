//! Tests for Phase 6 commands: pip drop-in, python runtime management, tool runner, and publish.

#![allow(clippy::pedantic, clippy::nursery)]

use std::env;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::sync::Mutex;

use lux_cli::commands::{
    run_init, run_pip_compile, run_pip_freeze, run_pip_install, run_pip_list, run_pip_uninstall,
    run_publish, run_python_find, run_python_install, run_python_list, run_python_pin,
    run_run, run_tool_install, run_tool_list, run_tool_uninstall,
};

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
async fn test_phase6_pip_lifecycle() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("temp workspace");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    // 1. lux init
    run_init(None, Some("pip-test-app".to_string())).expect("init succeeds");

    // 2. lux pip install
    let pkgs = vec!["requests".to_string(), "numpy".to_string()];
    run_pip_install(&pkgs, &[], &[]).await.expect("pip install succeeds");

    let venv = lux_core::env::VirtualEnv::find(temp_workspace.path()).expect("find venv");
    let site_packages = venv.site_packages_dir();
    assert!(site_packages.join("requests").exists(), "requests should be installed in site-packages");
    assert!(site_packages.join("numpy").exists(), "numpy should be installed in site-packages");

    // 3. lux pip list
    run_pip_list().expect("pip list succeeds");

    // 4. lux pip freeze
    run_pip_freeze().expect("pip freeze succeeds");

    // 5. lux pip compile
    let req_in = temp_workspace.path().join("requirements.in");
    std::fs::write(&req_in, "requests>=2.0.0\nurllib3\n").expect("write requirements.in");
    let req_out = temp_workspace.path().join("requirements.txt");
    run_pip_compile(&req_in, Some(req_out.clone())).expect("pip compile succeeds");
    assert!(req_out.is_file());
    let txt_content = std::fs::read_to_string(&req_out).unwrap();
    assert!(txt_content.contains("requests=="));

    // 6. lux pip uninstall
    run_pip_uninstall(&["requests".to_string()]).expect("pip uninstall succeeds");
    assert!(!site_packages.join("requests").exists(), "requests should be unlinked after uninstall");
    assert!(site_packages.join("numpy").exists(), "numpy should remain installed");
}

#[tokio::test]
async fn test_phase6_python_commands() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("temp workspace");
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    // lux python list
    run_python_list(false).expect("python list succeeds");
    run_python_list(true).expect("python list --all succeeds");

    // lux python pin
    run_python_pin("3.12").expect("python pin succeeds");
    assert_eq!(
        lux_core::env::PythonVersionFile::read_from(temp_workspace.path()).as_deref(),
        Some("3.12")
    );

    // lux python find
    run_python_find(None).expect("python find succeeds");

    // lux python install
    run_python_install(&["3.12.7".to_string()]).expect("python install succeeds");
    let managed_dir = lux_cache::python_runtime::PythonRuntime::managed_python_dir();
    let installed_python_dir = managed_dir.join("cpython-3.12.7");
    assert!(installed_python_dir.is_dir(), "cpython-3.12.7 directory should exist");
    assert!(installed_python_dir.join(".version").is_file(), "version descriptor should exist");
}

#[tokio::test]
async fn test_phase6_tool_commands() {
    let _lock = TEST_LOCK.lock().await;
    // lux tool list
    run_tool_list().expect("tool list succeeds");

    // lux tool install
    let tool_name = "mock-black";
    run_tool_install(tool_name).expect("tool install succeeds");
    let tool_dir = lux_cache::tools::ToolManager::tools_dir().join(tool_name);
    assert!(tool_dir.is_dir(), "tool directory should exist");
    assert!(tool_dir.join(".venv").is_dir(), "tool virtual environment should exist");

    // lux tool uninstall
    run_tool_uninstall(tool_name).expect("tool uninstall succeeds");
    assert!(!tool_dir.exists(), "tool directory should be deleted after uninstall");
}

#[tokio::test]
async fn test_phase6_publish_dry_run() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("temp workspace");
    let dist_dir = temp_workspace.path().join("dist");
    std::fs::create_dir_all(&dist_dir).unwrap();
    let mock_wheel = dist_dir.join("test_pkg-0.1.0-py3-none-any.whl");
    std::fs::write(&mock_wheel, b"PK0304mockwheel").unwrap();

    let _guard = CwdGuard::switch_to(temp_workspace.path());
    run_publish(&[mock_wheel], Some("https://test.pypi.org/legacy/"), None)
        .await
        .expect("publish dry run succeeds");
}

#[tokio::test]
async fn test_phase6_pep723_script_runner() {
    let _lock = TEST_LOCK.lock().await;
    let temp_workspace = TempDir::new().expect("temp workspace");
    let script = temp_workspace.path().join("standalone_task.py");
    let script_code = r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["requests"]
# ///
import sys
print("Script execution completed")
"#;
    std::fs::write(&script, script_code).unwrap();

    let _guard = CwdGuard::switch_to(temp_workspace.path());
    run_run(script.to_str().unwrap(), &[]).expect("run script succeeds");
}

#[test]
fn test_venv_command_standalone() {
    let temp_workspace = TempDir::new().unwrap();
    let _guard = CwdGuard::switch_to(temp_workspace.path());

    // Default `.venv` creation
    lux_cli::commands::run_venv(None, None).expect("lux venv succeeds");
    let venv_dir = temp_workspace.path().join(".venv");
    assert!(venv_dir.is_dir());
    assert!(venv_dir.join("pyvenv.cfg").is_file());

    // Custom named venv creation
    let custom_dir = temp_workspace.path().join("custom_env");
    lux_cli::commands::run_venv(Some(custom_dir.clone()), Some("3.12.0".to_string()))
        .expect("custom venv succeeds");
    assert!(custom_dir.is_dir());
    assert!(custom_dir.join("pyvenv.cfg").is_file());
}

