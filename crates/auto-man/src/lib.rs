pub mod asset;
mod automan;
mod cache;
mod builder;
mod dir;
pub mod error;
pub mod fs;
pub mod git;
pub mod group;
mod index;
mod lock;
mod pac;
mod port;
pub mod pull;
mod target;
pub mod up;
pub mod util;
pub mod version;

pub use automan::*;
pub use builder::*;
pub use cache::*;
pub use dir::*;
pub use error::*;
pub use index::*;
pub use lock::*;
pub use pac::*;
pub use port::*;
pub use target::*;
pub use version::*;

pub use auto_val::AutoError;
pub use auto_val::AutoResult;

pub const AUTOMAN_VERSION: &str = "v0.1.2";
