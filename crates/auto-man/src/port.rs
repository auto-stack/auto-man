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
        Self {
            name: "win32".into(),
            builder: "ninja".into(),
            platform: "win11".into(),
            at: "build".into(),
        }
    }
}
