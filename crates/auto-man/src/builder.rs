mod cmake;
mod iar;

pub use cmake::*;
pub use iar::*;

use crate::AutoResult;
use crate::{Pac, Target, TargetKind};
use auto_val::{AutoPath, AutoStr};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub trait Builder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn finish(&mut self, pac: &Pac) -> AutoResult<()>;
    fn target(&mut self, target: &Target) -> AutoResult<()>;
    fn clean(&mut self) -> AutoResult<()>;
    fn run(&mut self, pac: &Pac) -> AutoResult<()>;
}

pub enum BuilderKind {
    CMake(String), // Cmake with path to CMakeLists.txt
    IAR(AutoPath), // IAR with path to project folder
}

pub fn new_builder(kind: BuilderKind) -> Box<dyn Builder> {
    match kind {
        BuilderKind::CMake(path) => Box::new(CMakeBuilder::new(&path)),
        BuilderKind::IAR(path) => Box::new(IARBuilder::new(path)),
    }
}
