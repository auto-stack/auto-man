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

/// Collect generated files from build output directory and project root
fn collect_generated_files(case_path: &Path) -> HashMap<String, Vec<u8>> {
    let mut generated = HashMap::new();

    // Collect files from build directory
    let build_dir = case_path.join("build");
    if build_dir.exists() {
        if let Ok(entries) = fs::read_dir(&build_dir) {
            for entry in entries.flatten() {
                collect_files_recursive(&entry.path(), &mut generated, &build_dir);
            }
        }
    }

    // Collect specific generated files from project root
    // These are files that AutoMan generates in the project root (e.g., CMakeLists.txt)
    let root_files = ["CMakeLists.txt", "*.ninja"];
    for pattern in &root_files {
        if pattern.contains('*') {
            // Use glob for wildcards
            if let Ok(paths) = glob::glob(&case_path.join(pattern).to_string_lossy()) {
                for path in paths.flatten() {
                    if let Some(file_name) = path.file_name() {
                        let name = file_name.to_string_lossy().to_string();
                        if let Ok(content) = fs::read(&path) {
                            generated.insert(name, content);
                        }
                    }
                }
            }
        } else {
            // Direct file check
            let file_path = case_path.join(pattern);
            if file_path.is_file() {
                if let Ok(content) = fs::read(&file_path) {
                    generated.insert(pattern.to_string(), content);
                }
            }
        }
    }

    generated
}

/// Recursively collect files from a directory
fn collect_files_recursive(path: &Path, files: &mut HashMap<String, Vec<u8>>, base_dir: &Path) {
    if path.is_file() {
        // Get relative path from base_dir
        if let Ok(rel_path) = path.strip_prefix(base_dir) {
            let file_name = rel_path
                .to_string_lossy()
                .to_string()
                .replace('\\', "/"); // Normalize path separators

            if let Ok(content) = fs::read(path) {
                files.insert(file_name, content);
            }
        }
    } else if path.is_dir() {
        // Recursively collect from subdirectories
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_files_recursive(&entry.path(), files, base_dir);
            }
        }
    }
}

/// Save generated files as .wrong files for debugging
fn save_wrong_files(case_path: &Path, builder_name: &str, generated: &HashMap<String, Vec<u8>>) {
    let wrong_dir = case_path.join("wrong").join(builder_name);

    // Remove old wrong directory if it exists
    if wrong_dir.exists() {
        let _ = fs::remove_dir_all(&wrong_dir);
    }

    // Create wrong directory
    if let Err(e) = fs::create_dir_all(&wrong_dir) {
        eprintln!("Failed to create wrong directory: {}", e);
        return;
    }

    // Save all generated files
    for (filename, content) in generated {
        let file_path = wrong_dir.join(filename);
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&file_path, content) {
            eprintln!("Failed to write wrong file {:?}: {}", file_path, e);
        } else {
            println!("    Saved wrong file: {}", filename);
        }
    }

    println!("    Wrong files saved to: {:?}", wrong_dir);
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

    // Load expected files
    let expected_files = load_expected_files(&case.path, builder_name);

    if expected_files.is_empty() {
        return Err(format!("No expected files found for builder {}", builder_name));
    }

    println!("    Found {} expected file(s)", expected_files.len());

    // Clean up previous build
    let build_dir = case.path.join("build");
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir)
            .map_err(|e| format!("Failed to clean build directory: {}", e))?;
    }

    // Clean up CMakeLists.txt and build.ninja if they exist
    for file in ["CMakeLists.txt", "build.ninja"] {
        let file_path = case.path.join(file);
        if file_path.exists() {
            fs::remove_file(&file_path)
                .map_err(|e| format!("Failed to clean {}: {}", file, e))?;
        }
    }

    // Note: We don't create state.at here, letting AutoMan use its default port
    // This simplifies the test setup and ensures portability

    // Change to test case directory
    let original_dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;

    std::env::set_current_dir(&case.path)
        .map_err(|e| format!("Failed to change to test directory: {}", e))?;

    // Run auto-man to generate build files
    let result = (|| -> Result<(), String> {
        // Parse pac.at
        use auto_lang::config::AutoConfig;
        use auto_lang::Universe;
        use auto_val::Obj;

        let config_path = case.path.join("pac.at");
        let _config = AutoConfig::read(&config_path)
            .map_err(|e| format!("Failed to read pac.at: {}", e))?;

        // Set port environment variable
        let mut env = Universe::new();
        env.set_global("port", builder_name.to_string().into());

        // Re-load config with port environment
        let config = AutoConfig::from_file(&config_path, &Obj::new(), env)
            .map_err(|e| format!("Failed to parse pac.at with port: {}", e))?;

        // Create Pac
        let mut pac = auto_man::Pac::new(config);

        // Set port (this triggers builder initialization)
        pac.set_port(builder_name.into())
            .map_err(|e| format!("Failed to set port: {}", e))?;

        // Transpile Auto files to C
        println!("    Transpiling Auto files...");
        pac.transpile_autot()
            .map_err(|e| format!("Transpile failed: {}", e))?;

        // Generate build files (don't actually compile)
        println!("    Generating build files...");
        pac.build()
            .map_err(|e| format!("Build generation failed: {}", e))?;

        Ok(())
    })();

    // Restore original directory
    std::env::set_current_dir(&original_dir)
        .map_err(|e| format!("Failed to restore directory: {}", e))?;

    // Check if auto-man execution succeeded
    if let Err(e) = result {
        return Err(format!("Auto-man execution failed: {}", e));
    }

    // Collect generated files
    println!("    Collecting generated files...");
    let generated_files = collect_generated_files(&case.path);

    if generated_files.is_empty() {
        return Err("No files were generated".to_string());
    }

    println!("    Generated {} file(s)", generated_files.len());

    // Compare with expected
    println!("    Comparing outputs...");
    match compare_outputs(&generated_files, &expected_files) {
        Ok(_) => {
            println!("    ✓ All outputs match expected files");
            Ok(())
        }
        Err(e) => {
            // Save wrong files for debugging
            save_wrong_files(&case.path, builder_name, &generated_files);
            Err(format!("Output comparison failed: {}", e))
        }
    }
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
