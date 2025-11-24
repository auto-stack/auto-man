use std::env;

use auto_val::AutoStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Port {
    pub name: AutoStr,
    pub builder: AutoStr,
    pub platform: AutoStr,
    pub at: AutoStr,
}

impl Default for Port {
    fn default() -> Self {
        match env::consts::OS {
            "windows" => Self {
                name: "windows_ninja".into(),
                builder: "ninja".into(),
                platform: "windows".into(),
                at: "build".into(),
            },
            _ => Self {
                name: "linux_ninja".into(),
                builder: "ninja".into(),
                platform: "linux".into(),
                at: "build".into(),
            },
        }
    }
}
