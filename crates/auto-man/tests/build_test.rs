//! Integration tests for the AutoMan build process
//!
//! These tests verify end-to-end build functionality including:
//! - Project scanning and configuration parsing
//! - Source file discovery
//! - Build file generation
//! - Compilation

mod common;

use common::*;

#[test]
#[ignore] // Ignore by default as it requires am to be built
fn test_minimal_project_scan() {
    let project = TestProject::from_template(TestTemplate::minimal_app());

    // Run am scan
    let output = project.run_am("scan");

    // Should succeed
    assert_success(&output);

    // Check that build directory was created
    // Note: exact structure depends on builder implementation
}

#[test]
#[ignore]
fn test_project_with_sources() {
    let mut template = TestTemplate::app_with_deps();

    // Add more source files
    template.source_files.push((
        "src/utils.at".to_string(),
        r#"
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn multiply(a: i32, b: i32) -> i32 {
    return a * b;
}
"#
        .to_string(),
    ));

    let project = TestProject::from_template(template);

    // Run am build
    let output = project.run_am("build");

    // Should succeed
    assert_success(&output);

    // Verify build artifacts exist
    // This will depend on the specific builder being used
}

#[test]
#[ignore]
fn test_dependency_resolution() {
    // Create a mock git repository for the dependency
    let _mock_server = MockGitServer::new("test_math_lib");

    // Create a project that depends on the mock library
    let project = TestProject::new("app_with_dep");

    project.write_file(
        "pac.at",
        r#"
name: "app_with_dep"
version: "0.1.0"

dep("test_math_lib", "0.1.0") {
    at: "source/lib/test_math_lib"
}

app("main") {
    link("test_math_lib") {}
    dir("src") {}
}
"#,
    );

    // Create a simple source file
    project.write_file(
        "src/main.at",
        r#"
use test_math_lib

fn main() -> i32 {
    let result = test_math_lib::add(1, 2);
    return result;
}
"#,
    );

    // Run am pull to download dependencies
    let pull_output = project.run_am("pull");
    assert_success(&pull_output);

    // Verify dependencies were downloaded
    assert!(project.file_exists("deps/test_math_lib/pac.at"));

    // Run am build
    let build_output = project.run_am("build");
    assert_success(&build_output);
}

#[test]
#[ignore]
fn test_transpilation_output() {
    let mut template = TestTemplate::minimal_app();

    // Add Auto source file
    template.source_files.push((
        "src/main.at".to_string(),
        r#"
fn main() -> i32 {
    let x: i32 = 42;
    return x;
}
"#
        .to_string(),
    ));

    template.directories.push("src".to_string());

    let project = TestProject::from_template(template);

    // Run am build
    let output = project.run_am("build");
    assert_success(&output);

    // Check that transpiled C files were generated
    // The exact location depends on the build configuration
    // For example: build/Debug/src/main.c
}

#[test]
#[ignore]
fn test_build_failure_on_syntax_error() {
    let mut template = TestTemplate::minimal_app();

    // Add Auto source file with syntax error
    template.source_files.push((
        "src/main.at".to_string(),
        r#"
fn main( -> i32 {
    // Missing parameter - syntax error
    return 0;
}
"#
        .to_string(),
    ));

    template.directories.push("src".to_string());

    let project = TestProject::from_template(template);

    // Run am build - should fail
    let output = project.run_am("build");

    // Should fail due to syntax error
    assert_failure(&output);

    // Check error message contains useful information
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error") || stderr.contains("Error"),
        "Expected error message, got: {}",
        stderr
    );
}

#[test]
#[ignore]
fn test_multi_target_project() {
    let project = TestProject::new("multi_target");

    // Create a project with multiple targets
    project.write_file(
        "pac.at",
        r#"
name: "multi_target"
version: "0.1.0"

lib("utils") {
    dir("lib/utils") {}
}

lib("math") {
    dir("lib/math") {}
    link("utils") {}
}

app("main") {
    link("math") {}
    link("utils") {}
    dir("src") {}
}
"#,
    );

    // Create directory structure
    project.write_file("lib/utils/utils.at", "fn helper() -> i32 { return 0; }");
    project.write_file("lib/math/math.at", "fn calculate() -> i32 { return 1; }");
    project.write_file("src/main.at", "fn main() -> i32 { return 0; }");

    // Run am scan
    let output = project.run_am("scan");
    assert_success(&output);

    // Run am build
    let output = project.run_am("build");
    assert_success(&output);

    // Verify all targets were built
    // This depends on builder-specific output locations
}
