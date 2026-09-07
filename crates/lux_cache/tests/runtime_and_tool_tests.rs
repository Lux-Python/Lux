//! Tests for `PythonRuntime` manager and `ToolManager`.

use lux_cache::python_runtime::PythonRuntime;
use lux_cache::tools::ToolManager;

#[test]
fn test_python_runtime_discovery_and_catalog() {
    let installed = PythonRuntime::list_installed();
    println!("Discovered {} installed Python runtime(s):", installed.len());
    for py in &installed {
        println!("  - v{} at {} (managed: {})", py.version, py.path.display(), py.is_managed);
    }

    let available = PythonRuntime::list_available();
    assert!(!available.is_empty(), "Available Python catalog should not be empty");
    for av in &available {
        assert!(av.url.starts_with("https://"));
        assert!(!av.target.is_empty());
    }

    // Matching prefix
    let matched = PythonRuntime::find_matching_python(Some("3"));
    if !installed.is_empty() {
        assert!(matched.is_some());
    }
}

#[test]
fn test_tool_manager_directories_and_lifecycle() {
    let tools_dir = ToolManager::tools_dir();
    assert!(!tools_dir.as_os_str().is_empty());

    let bin_dir = ToolManager::bin_dir();
    assert!(!bin_dir.as_os_str().is_empty());

    let tool_name = "test-mock-tool";
    let env_dir = ToolManager::get_or_create_tool_dir(tool_name).unwrap();
    assert!(env_dir.is_dir());

    let list = ToolManager::list_installed_tools();
    assert!(list.contains(&tool_name.to_string()));

    ToolManager::uninstall_tool(tool_name).unwrap();
    let post_list = ToolManager::list_installed_tools();
    assert!(!post_list.contains(&tool_name.to_string()));
}
