use autoval::Node;

pub enum TargetKind {
    Exe,
    StaticLib,
    SharedLib,
    Obj,
}

impl TargetKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "exe" => TargetKind::Exe,
            "lib" => TargetKind::StaticLib,
            "dll" => TargetKind::SharedLib,
            "obj" => TargetKind::Obj,
            _ => panic!("Invalid target kind: {}", s),
        }
    }
}

pub struct Target {
    pub kind: TargetKind,
    pub name: String,
    pub config: Node,
}


impl Target {
    pub fn from_node(node: Node) -> Self {
        let mut name = "".to_string();
        if !node.args.is_empty() {
            name = node.args.args[0].to_string();
        }

        Self {
            kind: TargetKind::from_str(&node.name),
            name: name,
            config: node,
        }
    }
}
