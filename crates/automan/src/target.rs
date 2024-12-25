use autoval::Node;

pub struct Target<'a> {
    pub kind: String,
    pub name: String,
    pub config: &'a Node,
}


impl<'a> Target<'a> {
    pub fn from_node(node: &'a Node) -> Self {
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
