use autolang::AutoConfig;
use crate::AutoError;

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
        Ok(())
    }
}
