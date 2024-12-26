use autolang::config::AutoConfig;
use crate::AutoError;
use crate::Target;
use crate::builder::Builder;
use crate::builder::new_builder;
use crate::builder::BuilderKind;

pub struct Pac {
    pub name: String,
    pub targets: Vec<Target>,
}

impl Pac {
    pub fn new(config: AutoConfig) -> Self {
        let mut targets = vec![];
        let name = config.name();
        for n in config.root.nodes.into_iter() {
            let target = Target::from_node(n);
            targets.push(target);
        }
        Self {
            name,
            targets,
        }
    }

    pub fn resolve(&self) -> Result<(), AutoError> {
        println!("Resolving dependencies");
        Ok(())
    }

}
