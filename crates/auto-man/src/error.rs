//! AutoMan-specific error types
//!
//! This module defines domain-specific error types for AutoMan operations,
//! providing better error context and handling than the generic AutoError.

use auto_val::{AutoPath, AutoStr};
use std::fmt;

/// AutoMan-specific error types
#[derive(Debug)]
pub enum AutoManError {
    /// Configuration-related errors
    ConfigError {
        path: AutoPath,
        message: String,
    },

    /// Dependency-related errors
    DependencyError {
        name: AutoStr,
        version: AutoStr,
        reason: String,
    },

    /// Build-related errors
    BuildError {
        target: AutoStr,
        stage: BuildStage,
        message: String,
    },

    /// Git operation errors
    GitError {
        operation: String,
        repo: String,
        details: String,
    },

    /// Transpilation errors
    TranspileError {
        file: AutoPath,
        line: usize,
        message: String,
    },
}

/// Stages of the build process where errors can occur
#[derive(Debug, Clone, PartialEq)]
pub enum BuildStage {
    /// Scanning phase - discovering dependencies and sources
    Scan,
    /// Transpilation phase - converting Auto to C
    Transpile,
    /// Build generation phase - creating build files
    Generate,
    /// Compilation phase - running the compiler
    Compile,
}

impl fmt::Display for AutoManError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AutoManError::ConfigError { path, message } => {
                write!(f, "Configuration error in '{}': {}", path, message)
            }
            AutoManError::DependencyError { name, version, reason } => {
                write!(f, "Dependency error for '{}:{}': {}", name, version, reason)
            }
            AutoManError::BuildError { target, stage, message } => {
                write!(
                    f,
                    "Build error for target '{}' during {:?}: {}",
                    target, stage, message
                )
            }
            AutoManError::GitError { operation, repo, details } => {
                write!(f, "Git error during '{}' on '{}': {}", operation, repo, details)
            }
            AutoManError::TranspileError { file, line, message } => {
                write!(
                    f,
                    "Transpile error in '{}' at line {}: {}",
                    file, line, message
                )
            }
        }
    }
}

impl std::error::Error for AutoManError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AutoManError::ConfigError {
            path: AutoPath::new("test.at"),
            message: "Invalid syntax".to_string(),
        };
        assert!(err.to_string().contains("Configuration error"));
        assert!(err.to_string().contains("test.at"));
    }

    #[test]
    fn test_dependency_error() {
        let err = AutoManError::DependencyError {
            name: "mylib".into(),
            version: "1.0.0".into(),
            reason: "Not found in index".to_string(),
        };
        assert!(err.to_string().contains("mylib:1.0.0"));
        assert!(err.to_string().contains("Not found in index"));
    }

    #[test]
    fn test_build_error() {
        let err = AutoManError::BuildError {
            target: "myapp".into(),
            stage: BuildStage::Compile,
            message: "Compilation failed".to_string(),
        };
        assert!(err.to_string().contains("myapp"));
        assert!(err.to_string().contains("Compile"));
    }
}
