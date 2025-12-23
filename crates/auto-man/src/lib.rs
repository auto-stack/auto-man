pub mod asset;
mod automan;
mod builder;
mod dir;
pub mod fs;
pub mod git;
pub mod group;
mod index;
mod pac;
mod port;
pub mod pull;
mod target;
pub mod up;
pub mod util;

pub use automan::*;
pub use builder::*;
pub use dir::*;
pub use index::*;
pub use pac::*;
pub use port::*;
pub use target::*;

pub use auto_val::AutoError;
pub use auto_val::AutoResult;

pub const AUTOMAN_VERSION: &str = "v0.1.2";
