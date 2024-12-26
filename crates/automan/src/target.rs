use autoval::Node;

pub struct Target {
    pub kind: String,
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
            kind: node.name.clone(),
            name: name,
            config: node,
        }
    }
}
