use autolang::config::AutoConfig;
use crate::AutoError;
use crate::{Target, TargetKind};

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

    pub fn exe_path(&self) -> String {
        // TODO: check Debug/Release, check exe name
        let mode = "Debug";
        // find first exe in targets
        for target in &self.targets {
            match target.kind {
                TargetKind::Exe => {
                    return format!("build/{}/{}", mode, target.name);
                }
                _ => {}
            }
        }
        format!("build/{}/{}", mode, self.name)
    }
}
