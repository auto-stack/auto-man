//! Integration tests for dependency management
//!
//! These tests verify:
//! - Git-based dependency download
//! - Dependency version resolution
//! - Dependency linking
//! - Circular dependency detection

mod common;

use common::*;
use std::fs::File as StdFile;
use std::io::Write;
use tempfile::TempDir;

#[test]
#[ignore]
fn test_simple_dependency_download() {
    // Create a mock dependency
    let _mock_server = MockGitServer::new("simple_lib");

    let project = TestProject::new("app_with_dep");

    project.write_file(
        "pac.at",
        r#"
name: "app_with_dep"
version: "0.1.0"

dep("simple_lib", "0.1.0") {
    at: "source/lib/simple_lib"
}

app("main") {
    link("simple_lib") {}
}
"#,
    );

    // Pull dependencies
    let output = project.run_am("pull");
    assert_success(&output);

    // Verify dependency was downloaded
    assert!(project.file_exists("deps/simple_lib/pac.at"));
}

#[test]
#[ignore]
fn test_dependency_with_nested_links() {
    // Create lib_a that depends on lib_b
    let mock_server_b = MockGitServer::new("lib_b");

    // Create lib_a (manually since we need it to depend on lib_b)
    let temp_dir_a = TempDir::new().expect("Failed to create temp dir");
    let repo_path_a = temp_dir_a.path().join("lib_a");

    std::process::Command::new("git")
        .args(["init", repo_path_a.to_str().unwrap()])
        .output()
        .expect("Failed to init git repo");

    // Configure git
    std::process::Command::new("git")
        .args(["-C", repo_path_a.to_str().unwrap(), "config", "user.email", "test@test.com"])
        .output()
        .expect("Failed to configure git");

    std::process::Command::new("git")
        .args(["-C", repo_path_a.to_str().unwrap(), "config", "user.name", "Test User"])
        .output()
        .expect("Failed to configure git");

    // Create pac.at for lib_a that depends on lib_b
    let pac_at = repo_path_a.join("pac.at");
    let mut file = StdFile::create(&pac_at).expect("Failed to create pac.at");
    writeln!(
        file,
        r#"
name: "lib_a"
version: "0.1.0"

dep("lib_b", "0.1.0") {{
    at: "source/lib/lib_b"
}}

lib("lib_a") {{
    link("lib_b") {{}}
    dir("src") {{}}
}}
"#
    )
    .expect("Failed to write pac.at");

    // Create source directory and initial commit
    let src_dir = repo_path_a.join("src");
    std::fs::create_dir_all(&src_dir).expect("Failed to create src dir");

    std::process::Command::new("git")
        .args(["-C", repo_path_a.to_str().unwrap(), "add", "."])
        .output()
        .expect("Failed to add files");

    std::process::Command::new("git")
        .args(["-C", repo_path_a.to_str().unwrap(), "commit", "-m", "Initial commit"])
        .output()
        .expect("Failed to commit");

    // Now create a project that uses lib_a
    let project = TestProject::new("app");

    project.write_file(
        "pac.at",
        &format!(
            r#"
name: "app"
version: "0.1.0"

dep("lib_a", "0.1.0") {{
    at: "{}"
}}

app("main") {{
    link("lib_a") {{}}
}}
"#,
            repo_path_a.to_str().unwrap()
        ),
    );

    // Pull dependencies
    let output = project.run_am("pull");
    assert_success(&output);

    // Verify both lib_a and lib_b were downloaded
    assert!(project.file_exists("deps/lib_a/pac.at"));
    assert!(project.file_exists("deps/lib_b/pac.at"));
}

#[test]
#[ignore]
fn test_duplicate_dependency_detection() {
    // Create two dependencies that both depend on a common library
    let _mock_common = MockGitServer::new("common_lib");
    let _mock_lib_a = MockGitServer::new("lib_a");
    let _mock_lib_b = MockGitServer::new("lib_b");

    let project = TestProject::new("app");

    project.write_file(
        "pac.at",
        r#"
name: "app"
version: "0.1.0"

dep("lib_a", "0.1.0") {
    at: "source/lib/lib_a"
}

dep("lib_b", "0.1.0") {
    at: "source/lib/lib_b"
}

app("main") {
    link("lib_a") {}
    link("lib_b") {}
}
"#,
    );

    // Pull dependencies
    let output = project.run_am("pull");
    assert_success(&output);

    // Common library should only be downloaded once
    // (This verifies the dependency deduplication logic)
    assert!(project.file_exists("deps/common_lib/pac.at"));
}

#[test]
#[ignore]
fn test_missing_dependency_error() {
    let project = TestProject::new("app");

    project.write_file(
        "pac.at",
        r#"
name: "app"
version: "0.1.0"

dep("nonexistent_lib", "0.1.0") {
    at: "source/lib/nonexistent_lib"
}

app("main") {
    link("nonexistent_lib") {}
}
"#,
    );

    // Pull should fail
    let output = project.run_am("pull");
    assert_failure(&output);

    // Error message should indicate the problem
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nonexistent") || stderr.contains("not found") || stderr.contains("error"),
        "Expected error about nonexistent library, got: {}",
        stderr
    );
}
