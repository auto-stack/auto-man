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
            },
            _ => Self {
                name: "linux_ninja".into(),
                builder: "ninja".into(),
                platform: "linux".into(),
                at: "build".into(),
                tool: Tool::from_kind("gcc".into(), crate::ToolKind::Gcc),
            },
        }
    }
}
