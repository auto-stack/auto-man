use autolang::config::AutoConfig;
use autoval::AutoStr;
use crate::AutoError;
use crate::{Target, TargetKind};
use std::fmt;
pub struct Pac {
    pub name: AutoStr,
    pub version: AutoStr,
    pub targets: Vec<Target>,
}

impl Pac {
    pub fn new(config: AutoConfig) -> Self {
        let mut targets = vec![];
        let name = config.name();
        let version = config.version();
        for n in config.root.nodes.into_iter() {
            let target = Target::from_node(n);
            targets.push(target);
        }
        Self {
            name,
            version,
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

impl fmt::Display for Pac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pac() {
        let code = r#"
        name: "test"
        version: "0.0.1"

        exe("main") {}
        "#;
        let config = AutoConfig::from_code(code);
        match config {
            Ok(config) => {
                let pac = Pac::new(config);
                assert_eq!(pac.name, "test");
                assert_eq!(pac.version, "0.0.1");
                assert_eq!(pac.targets.len(), 1);
                assert_eq!(pac.targets[0].name, "main");
                assert_eq!(pac.targets[0].kind, TargetKind::Exe);
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
    }
}
