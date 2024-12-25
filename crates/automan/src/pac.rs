use autolang::config::AutoConfig;
use crate::AutoError;
use crate::Target;

pub struct Pac {
    pub name: String,
    pub config: AutoConfig,
}

impl Pac {
    pub fn new(config: AutoConfig) -> Self {
        Self {
            name: config.name(),
            config,
        }
    }

    pub fn resolve(&self) -> Result<(), AutoError> {
        println!("Resolving dependencies");
        Ok(())
    }

    pub fn build(&self) -> Result<(), AutoError> {
        println!("Building project");
        println!("Targets: {}", self.config.list_target_names().join(", "));
        for n in &self.config.root.nodes {
            let target = Target::from_node(n);
            println!("Building target: {}", target.kind);
        }
        Ok(())
    }
}
