use auto_val::AutoStr;

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
            builder: "cmake".into(),
            platform: "win11".into(),
            at: "cmake".into(),
        }
    }
}
