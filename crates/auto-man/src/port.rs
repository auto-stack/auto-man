use std::env;

use auto_val::AutoStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tool {
    pub name: AutoStr,
    pub compiler: AutoStr,
    pub assembler: AutoStr,
    pub linker: AutoStr,
    pub archiver: AutoStr,
}

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
                tool: Tool {
                    name: "msvc".into(),
                    compiler: "cl.exe".into(),
                    assembler: "ml64.exe".into(),
                    linker: "link.exe".into(),
                    archiver: "lib.exe".into(),
                },
            },
            _ => Self {
                name: "linux_ninja".into(),
                builder: "ninja".into(),
                platform: "linux".into(),
                at: "build".into(),
                tool: Tool {
                    name: "gcc".into(),
                    compiler: "gcc".into(),
                    assembler: "as".into(),
                    linker: "ld".into(),
                    archiver: "ar".into(),
                },
            },
        }
    }
}
