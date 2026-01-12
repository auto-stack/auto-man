//! Common test utilities and helpers for AutoMan integration tests
//!
//! This module provides reusable test fixtures, temporary directory management,
//! and mock utilities for testing AutoMan's build process.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;
use tempfile::TempDir;

/// Test project builder for creating temporary AutoMan projects
pub struct TestProject {
    /// Temporary directory containing the test project
    pub temp_dir: TempDir,
    /// Path to the project root
    pub project_path: PathBuf,
}

impl TestProject {
    /// Create a new empty test project
    pub fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let project_path = temp_dir.path().join(name);

        fs::create_dir_all(&project_path).expect("Failed to create project dir");

        TestProject {
            temp_dir,
            project_path,
        }
    }

    /// Create a test project from a template
    pub fn from_template(template: TestTemplate) -> Self {
        let project = Self::new(&template.name);

        // Create pac.at file
        let pac_content = template.pac_at_content();
        project.write_file("pac.at", &pac_content);

        // Create source files if specified
        for (path, content) in template.source_files {
            project.write_file(&path, &content);
        }

        // Create directory structure
        for dir in &template.directories {
            let dir_path = project.project_path.join(dir);
            fs::create_dir_all(&dir_path).expect("Failed to create directory");
        }

        project
    }

    /// Write content to a file in the test project
    pub fn write_file(&self, relative_path: &str, content: &str) {
        let file_path = self.project_path.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory");
        }

        let mut file = File::create(&file_path).expect("Failed to create file");
        file.write_all(content.as_bytes())
            .expect("Failed to write file content");
    }

    /// Read content from a file in the test project
    pub fn read_file(&self, relative_path: &str) -> String {
        let file_path = self.project_path.join(relative_path);
        fs::read_to_string(&file_path).expect("Failed to read file")
    }

    /// Check if a file exists in the test project
    pub fn file_exists(&self, relative_path: &str) -> bool {
        let file_path = self.project_path.join(relative_path);
        file_path.exists()
    }

    /// Get the absolute path to a file in the test project
    pub fn path(&self, relative_path: &str) -> PathBuf {
        self.project_path.join(relative_path)
    }

    /// Run am command in this project directory
    pub fn run_am(&self, args: &str) -> std::process::Output {
        let am_exe = std::env::var("AM_TEST_EXE")
            .unwrap_or_else(|_| "./am".to_string());

        let output = std::process::Command::new(am_exe)
            .current_dir(&self.project_path)
            .args(args.split_whitespace())
            .output();

        output.expect("Failed to execute am command")
    }
}

/// Test project templates for common scenarios
pub struct TestTemplate {
    pub name: String,
    pub target_type: TargetType,
    pub source_files: Vec<(String, String)>,
    pub directories: Vec<String>,
}

pub enum TargetType {
    App,
    Lib,
    Bag,
}

impl TestTemplate {
    /// Create a minimal app template
    pub fn minimal_app() -> Self {
        Self {
            name: "test_app".to_string(),
            target_type: TargetType::App,
            source_files: vec![],
            directories: vec![],
        }
    }

    /// Create a simple app with dependencies
    pub fn app_with_deps() -> Self {
        Self {
            name: "app_with_deps".to_string(),
            target_type: TargetType::App,
            source_files: vec![
                ("src/main.at".to_string(), r#"
use math

fn main() -> i32 {
    let result = math::add(1, 2);
    return result;
}
"#.to_string()),
            ],
            directories: vec!["src".to_string()],
        }
    }

    /// Create a library template
    pub fn lib() -> Self {
        Self {
            name: "test_lib".to_string(),
            target_type: TargetType::Lib,
            source_files: vec![],
            directories: vec![],
        }
    }

    /// Generate pac.at content for this template
    fn pac_at_content(&self) -> String {
        match self.target_type {
            TargetType::App => {
                format!(r#"
name: "{}"
version: "0.1.0"

app("main") {{
    dir("src") {{}}
}}
"#, self.name)
            }
            TargetType::Lib => {
                format!(r#"
name: "{}"
version: "0.1.0"

lib("mylib") {{
    dir("src") {{}}
}}
"#, self.name)
            }
            TargetType::Bag => {
                format!(r#"
name: "{}"
version: "0.1.0"

bag("codebag") {{
    dir("src") {{}}
}}
"#, self.name)
            }
        }
    }
}

/// Mock git server for testing dependency operations
pub struct MockGitServer {
    pub temp_dir: TempDir,
    pub repo_url: String,
}

impl MockGitServer {
    /// Create a new mock git server with a test repository
    pub fn new(repo_name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_path = temp_dir.path().join(repo_name);

        // Initialize git repository
        std::process::Command::new("git")
            .args(["init", repo_path.to_str().unwrap()])
            .output()
            .expect("Failed to init git repo");

        // Configure git
        std::process::Command::new("git")
            .args(["-C", repo_path.to_str().unwrap(), "config", "user.email", "test@test.com"])
            .output()
            .expect("Failed to configure git");

        std::process::Command::new("git")
            .args(["-C", repo_path.to_str().unwrap(), "config", "user.name", "Test User"])
            .output()
            .expect("Failed to configure git");

        // Create a basic pac.at file
        let pac_at = repo_path.join("pac.at");
        let mut file = File::create(&pac_at).expect("Failed to create pac.at");
        writeln!(file, r#"
name: "{}"
version: "0.1.0"

lib("{}") {{
    dir("src") {{}}
}}
"#, repo_name, repo_name).expect("Failed to write pac.at");

        // Create source directory
        let src_dir = repo_path.join("src");
        fs::create_dir_all(&src_dir).expect("Failed to create src dir");

        // Initial commit
        std::process::Command::new("git")
            .args(["-C", repo_path.to_str().unwrap(), "add", "."])
            .output()
            .expect("Failed to add files");

        std::process::Command::new("git")
            .args(["-C", repo_path.to_str().unwrap(), "commit", "-m", "Initial commit"])
            .output()
            .expect("Failed to commit");

        let repo_url = repo_path.to_str().unwrap().to_string();

        MockGitServer {
            temp_dir,
            repo_url,
        }
    }
}

/// Assert that a command output indicates success
pub fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "Command failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Assert that a command output indicates failure
pub fn assert_failure(output: &std::process::Output) {
    assert!(
        !output.status.success(),
        "Command unexpectedly succeeded\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Generate a unique test identifier based on timestamp
pub fn unique_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("test_{}", timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_project_creation() {
        let project = TestProject::new("test_proj");
        assert!(project.project_path.exists());
        assert!(project.temp_dir.path().exists());
    }

    #[test]
    fn test_test_project_file_operations() {
        let project = TestProject::new("test_proj");
        project.write_file("test.txt", "Hello, World!");
        assert!(project.file_exists("test.txt"));
        let content = project.read_file("test.txt");
        assert_eq!(content, "Hello, World!");
    }
}
