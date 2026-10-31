pub mod ninja;
pub use ninja::*;
mod tool;
pub use tool::*;
mod cargo;
pub use cargo::*;
mod vue;
pub use vue::*;

use crate::AutoResult;
use crate::{Pac, Target};
use auto_val::{AutoPath, AutoStr};
use std::collections::HashMap;

/// Build tools inherit the caller's console and environment. On Windows their
/// compiler descendants (including MSVC auxiliaries) belong to the same owned
/// Job and are closed after the original root status has been collected. Unix
/// keeps the original foreground process group / Ctrl+C behavior.
fn run_build_command<I, S>(
    argv: I,
    cwd: Option<&std::path::Path>,
) -> std::io::Result<std::process::ExitStatus>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString>,
{
    use auto_tool_execution::{ExecutionSpec, StdioPolicy};
    let mut spec = ExecutionSpec::new(argv);
    spec.cwd = cwd.map(std::path::Path::to_path_buf);
    spec.stdin = StdioPolicy::Inherit;
    spec.stdout = StdioPolicy::Inherit;
    spec.stderr = StdioPolicy::Inherit;
    #[cfg(windows)]
    {
        spec.tree_scope = auto_tool_execution::TreeScope::ProcessTree;
    }
    let mut child = auto_tool_execution::spawn(&spec).map_err(|error| match error {
        auto_tool_execution::ExecutionError::Spawn { source, .. } => source,
        other => std::io::Error::other(other.to_string()),
    })?;
    let status = child.wait()?;
    #[cfg(windows)]
    child.terminate()?;
    Ok(status)
}

#[cfg(test)]
mod build_command_tests {
    #[test]
    fn build_command_preserves_nonzero_exit_status() {
        #[cfg(windows)]
        let argv = vec!["cmd", "/C", "exit", "23"];
        #[cfg(not(windows))]
        let argv = vec!["sh", "-c", "exit 23"];
        let status = super::run_build_command(argv, None).unwrap();
        assert_eq!(status.code(), Some(23));
        assert!(!status.success());
    }

    #[test]
    fn build_command_preserves_missing_program_error() {
        let error = super::run_build_command(["__plan890_missing_compiler__"], None).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}

pub trait Builder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn finish(&mut self, pac: &Pac) -> AutoResult<()>;
    fn target(&mut self, target: &Target, pac: &Pac) -> AutoResult<()>;
    fn clean(&mut self) -> AutoResult<()>;
    fn run(&mut self, pac: &Pac, args: Vec<String>) -> AutoResult<()>;

    /// Enable memory output mode for testing
    fn enable_memory_output(&mut self) -> AutoResult<()>;

    /// Get captured memory output
    fn get_memory_output(&self) -> HashMap<String, Vec<u8>>;
}

pub enum BuilderKind {
    Ninja(AutoPath), // Ninja with path to build folder
    Cargo(AutoPath), // Cargo with path to Cargo.toml directory
    Vue(AutoPath),   // Vue with path to project directory (containing package.json)
}

impl BuilderKind {
    /// Create a BuilderKind from a string identifier
    pub fn from_str(builder_name: &str, path: AutoPath) -> Option<Self> {
        match builder_name {
            "ninja" => {
                let path = path.join("build.ninja");
                Some(BuilderKind::Ninja(path))
            }
            "cargo" => {
                let path = path.join("Cargo.toml");
                Some(BuilderKind::Cargo(path))
            }
            "vue" => {
                // Vue project: path is the dist directory (where package.json is)
                Some(BuilderKind::Vue(path))
            }
            _ => None,
        }
    }

    /// Create a builder instance from this BuilderKind
    pub fn create_builder(&self) -> Box<dyn Builder> {
        match self {
            BuilderKind::Ninja(path) => Box::new(NinjaBuilder::new(path.clone())),
            BuilderKind::Cargo(path) => Box::new(CargoBuilder::new(path.clone())),
            BuilderKind::Vue(path) => Box::new(VueBuilder::new(path.clone())),
        }
    }
}

pub fn new_builder(kind: BuilderKind) -> Box<dyn Builder> {
    kind.create_builder()
}

/// Create a builder from a string builder type and path
pub fn make_builder(builder: &AutoStr, path: AutoPath) -> Option<Box<dyn Builder>> {
    BuilderKind::from_str(builder.as_str(), path).map(|kind| kind.create_builder())
}
