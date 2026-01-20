use std::env;

use crate::Tool;
use auto_val::AutoStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Port {
    pub name: AutoStr,
    pub builder: AutoStr,
    pub platform: AutoStr,
    pub at: AutoStr,
    pub tool: Tool,
    /// 引用的编译器配置名称 (可选)
    pub compiler_name: Option<AutoStr>,
}

impl Default for Port {
    fn default() -> Self {
        match env::consts::OS {
            "windows" => Self {
                name: "windows_ninja".into(),
                builder: "ninja".into(),
                platform: "windows".into(),
                at: "build".into(),
                tool: Tool::from_kind("msvc".into(), crate::ToolKind::Msvc),
                compiler_name: None,
            },
            _ => Self {
                name: "linux_ninja".into(),
                builder: "ninja".into(),
                platform: "linux".into(),
                at: "build".into(),
                tool: Tool::from_kind("gcc".into(), crate::ToolKind::Gcc),
                compiler_name: None,
            },
        }
    }
}
