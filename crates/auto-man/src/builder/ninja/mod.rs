// Ninja builder 子模块
//
// 这个模块实现了一个灵活、可配置的 Ninja build 系统生成器

// 导出所有子模块
pub mod config;
pub mod mapper;
pub mod templates;
pub mod resolver;
pub mod builder;

// 重新导出常用类型
pub use config::{
    CompilerConfig,
    CompilerKind,
    CompilerLocation,
    ExecutableType,
    FlagFormat,
    FlagMappings,
};

pub use mapper::FlagMapper;

pub use templates::CommandTemplates;

pub use resolver::CompilerResolver;

// 导出 NinjaBuilder
pub use builder::NinjaBuilder;
