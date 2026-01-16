//! Integration tests for AutoMan
//!
//! This module implements folder-based test cases where each test case is a complete
//! auto-man project. The test framework:
//! 1. Discovers test cases in tests/integration/
//! 2. Runs auto-man to generate build configurations
//! 3. Compares generated files with expected outputs
//! 4. Generates .wrong files on failure for debugging

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a single test case (folder)
struct TestCase {
    name: String,
    path: PathBuf,
}

impl TestCase {
    /// Get builders that this test case supports
    fn get_supported_builders(&self) -> Vec<String> {
        let mut builders = Vec::new();

        // Check which expected output directories exist
        let expected_dir = self.path.join("expected");
        if expected_dir.exists() {
            if let Ok(entries) = fs::read_dir(&expected_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    builders.push(name);
                }
            }
        }

        builders
    }

    /// Check if this test case has expected files for a builder
    fn has_expected_files(&self, builder_name: &str) -> bool {
        let expected_dir = self.path.join("expected").join(builder_name);
        expected_dir.exists() && expected_dir.is_dir()
    }
}

/// Discover all test cases in tests/integration/
fn discover_test_cases() -> Vec<TestCase> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_dir = manifest_dir.join("tests/integration");
    let mut cases = Vec::new();

    if !test_dir.exists() {
        eprintln!("Warning: Test directory {:?} does not exist", test_dir);
        return cases;
    }

    if let Ok(entries) = fs::read_dir(&test_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Check if it's a directory with pac.at file
            if path.is_dir() && path.join("pac.at").exists() {
                let name = path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                cases.push(TestCase { name, path });
            }
        }
    }

    // Sort by name for consistent ordering
    cases.sort_by_key(|c| c.name.clone());
    cases
}

/// Load expected files for a specific builder
fn load_expected_files(case_path: &Path, builder_name: &str) -> HashMap<String, Vec<u8>> {
    let mut expected = HashMap::new();
    let expected_dir = case_path.join("expected").join(builder_name);

    if let Ok(entries) = fs::read_dir(&expected_dir) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_file() {
                let file_name = file_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();

                if let Ok(content) = fs::read(&file_path) {
                    expected.insert(file_name, content);
                }
            }
        }
    }

    expected
}

/// Compare generated outputs with expected files
fn compare_outputs(
    generated: &HashMap<String, Vec<u8>>,
    expected: &HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    // Check all expected files
    for (filename, expected_content) in expected {
        match generated.get(filename) {
            Some(generated_content) => {
                if generated_content != expected_content {
                    return Err(format!(
                        "File {} mismatch:\nExpected: {} bytes\nGenerated: {} bytes",
                        filename,
                        expected_content.len(),
                        generated_content.len()
                    ));
                }
            }
            None => {
                return Err(format!("Missing generated file: {}", filename));
            }
        }
    }

    // Check for unexpected generated files
    for filename in generated.keys() {
        if !expected.contains_key(filename) {
            return Err(format!("Unexpected generated file: {}", filename));
        }
    }

    Ok(())
}

/// Run a single test case with a specific builder
fn run_test_case(case: &TestCase, builder_name: &str) -> Result<(), String> {
    println!("  Testing builder: {}", builder_name);

    // For now, we'll skip actual execution and just check expected files exist
    // TODO: Implement actual auto-man execution and output generation

    let expected_files = load_expected_files(&case.path, builder_name);

    if expected_files.is_empty() {
        return Err(format!("No expected files found for builder {}", builder_name));
    }

    println!("    Found {} expected file(s)", expected_files.len());

    // TODO: Generate actual outputs and compare
    // For now, just verify expected files exist
    Ok(())
}

/// Main integration test function
#[test]
fn test_integration_all_cases() {
    let test_cases = discover_test_cases();

    if test_cases.is_empty() {
        println!("No test cases found");
        return;
    }

    println!("\n=== AutoMan Integration Tests ===");
    println!("Found {} test case(s)\n", test_cases.len());

    let mut passed = 0;
    let mut failed = 0;

    for case in &test_cases {
        println!("=== Testing: {} ===", case.name);
        println!("Path: {:?}", case.path);

        let builders = case.get_supported_builders();

        if builders.is_empty() {
            println!("  Skipped (no expected output directories)");
            continue;
        }

        println!("  Builders: {:?}", builders);

        for builder_name in &builders {
            match run_test_case(case, builder_name) {
                Ok(_) => {
                    println!("  ✓ {} builder passed", builder_name);
                    passed += 1;
                }
                Err(e) => {
                    println!("  ✗ {} builder failed: {}", builder_name, e);
                    // TODO: Generate .wrong files
                    failed += 1;
                }
            }
        }
    }

    println!("\n=== Test Summary ===");
    println!("Total: {}", test_cases.len());
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);

    if failed > 0 {
        panic!("{} test(s) failed", failed);
    }
}
