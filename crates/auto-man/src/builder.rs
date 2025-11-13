mod cmake;
pub use cmake::*;
mod iar;
pub use iar::*;
mod ghs;
pub use ghs::*;
mod ninja;
pub use ninja::*;

use crate::AutoResult;
use crate::{Pac, Target};
use auto_val::AutoPath;

pub trait Builder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn finish(&mut self, pac: &Pac) -> AutoResult<()>;
    fn target(&mut self, target: &Target) -> AutoResult<()>;
    fn clean(&mut self) -> AutoResult<()>;
    fn run(&mut self, pac: &Pac) -> AutoResult<()>;
}

pub enum BuilderKind {
    CMake(String),   // Cmake with path to CMakeLists.txt
    IAR(AutoPath),   // IAR with path to project folder
    GHS(AutoPath),   // GHS with path to project folder
    Ninja(AutoPath), // Ninja with path to build folder
}

pub fn new_builder(kind: BuilderKind) -> Box<dyn Builder> {
    match kind {
        BuilderKind::CMake(path) => Box::new(CMakeBuilder::new(&path)),
        BuilderKind::IAR(path) => Box::new(IARBuilder::new(path)),
        BuilderKind::GHS(path) => Box::new(GHSBuilder::new(path)),
        BuilderKind::Ninja(path) => Box::new(NinjaBuilder::new(path)),
    }
}
