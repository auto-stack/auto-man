use autolang::config::AutoConfig;
use autoval::AutoStr;
use std::path::Path;
use crate::AutoResult;
use crate::{Target, TargetKind, TargetStatus, TargetConfig};
use log::*;
use std::fmt;
use std::collections::HashMap;
use tabled::{
    Table,
    settings::{
        style::Style,
        object::Rows,
        themes::Colorization,
        Color,
    },
};

pub struct PacInfo {
    pub name: AutoStr,
    pub version: AutoStr,
    pub repo: AutoStr,
}

pub struct Index {
    map: HashMap<AutoStr, PacInfo>,
}

impl Index {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        map.insert("utils".into(), PacInfo {
            name: "utils".into(),
            version: "0.0.1".into(),
            repo: "ssh://git@gitlab.boevxa.lan:2424/9100004/utils.git".into(),
        });
        map.insert("FreeRTOS".into(), PacInfo {
            name: "FreeRTOS".into(),
            version: "0.0.1".into(),
            // repo: "ssh://git@gitlab.boevxa.lan:2424/9100004/FreeRTOS.git".into(),
            repo: "D:/am/bare/FreeRTOS".into(),
        });
        map.insert("osal".into(), PacInfo {
            name: "osal".into(),
            version: "0.0.1".into(),
            repo: "D:/am/bare/osal".into(),
        });
        map.insert("kernel".into(), PacInfo {
            name: "kernel".into(),
            version: "0.0.1".into(),
            repo: "D:/am/bare/kernel".into(),
        });
        map.insert("common".into(), PacInfo {
            name: "common".into(),
            version: "0.0.1".into(),
            repo: "D:/am/bare/common".into(),
        });
        Self {
            map,
        }
    }

    pub fn lookup(&self, name: &AutoStr) -> Option<&PacInfo> {
        self.map.get(name)
    }
}

pub struct Pac {
    pub name: AutoStr,
    pub version: AutoStr,
    pub targets: Vec<Target>,
    pub index: Index,
}

impl Pac {
    pub fn new(config: AutoConfig) -> Self {
        let mut targets = vec![];
        let pac_name = config.name();
        let version = config.version();
        for n in config.root.nodes.into_iter() {
            let target = Target::from(TargetConfig::from_node(n, pac_name.clone()));
            targets.push(target);
        }
        Self {
            name: pac_name,
            version,
            targets,
            index: Index::new(),
        }
    }

    pub fn transpile(&mut self) -> AutoResult<()> {
        for target in &mut self.targets {
            if target.has_auto() {
                target.transpile()?;
            }
        }
        Ok(())
    }

    // print targets as a table
    pub fn print_targets(&self) {
        info!("\n--- pac: {} ---\n", self.name);
        let mut statuses = vec![];
        for target in &self.targets {
            statuses.push(TargetStatus::from(target));
        }

        let mut table = Table::new(&statuses);
        table.with(Style::modern_rounded())
            .with(Colorization::exact([Color::FG_BRIGHT_BLUE], Rows::first()));

        println!("{}", table);
    }

    pub fn get_target(&self, name: impl Into<AutoStr>) -> Option<&Target> {
        let name = name.into();
        self.targets.iter().find(|t| t.name == name)
    }

    pub fn resolve(&mut self) -> AutoResult<()> {
        // 1. resolve dependencies: download dep targets recursively if needed
        info!("downloading dependencies");
        self.download_deps()?;

        // 2. link targets with dependencies
        info!("linking targets with dependencies");
        for target in &mut self.targets {
            target.check_links()?;
        }

        // 3. scan targets for its local source files
        info!("scanning targets for local source files");
        for target in &mut self.targets {
            target.scan()?;
        }
        Ok(())
    }

    pub fn download_deps(&mut self) -> AutoResult<()> {
        let mut targets = std::mem::take(&mut self.targets);
        for target in targets.iter_mut() {
            if matches!(target.kind, TargetKind::Dep) {
                if target.exists() {
                    continue;
                }
                self.download_dep(&target)?;
                // get args from dep node props
                // read pac.at from dep and parse the lib target
                info!("reading pac with override props: {:?}", target.props);
                let dep_config = AutoConfig::from_file(
                    Path::new(format!("deps/{}/pac.at", target.name).as_str()),
                    // override dep props
                    &target.props)?;
                let pac = Pac::new(dep_config);
                // TODO: recursively download dep targets from the dep
                // get the lib target
                let dep_name = target.name.clone();
                let lib_target = pac.get_target(dep_name);
                if let Some(lib_target) = lib_target {
                    // merge the lib target into current dep target
                    info!("target root: {}", target.root);
                    target.merge(lib_target);
                    info!("target root updated: {}", target.root);
                }
            }
        }
        self.targets = targets;
        Ok(())
    }

    // 下载dep的源码包
    pub fn download_dep(&mut self, dep: &Target) -> AutoResult<()> {
        // 查询索引
        let name = dep.name.clone();
        println!("looking up dependency from index: {}", name);
        let info = self.index.lookup(&name);
        match info {
            Some(info) => {
                self.download_repo(&info.name, &info.repo)?;
                return Ok(());
            }
            None => {
                return Err(format!("Dependency {} not found", name).into());
            }
        }
    }

    pub fn download_repo(&self, name: &AutoStr, repo: &AutoStr) -> AutoResult<()> {
        println!("Downloading dependency from {}", repo);
        // 1. make deps folder if not exists
        let deps_dir = "deps";
        if !std::path::Path::new(deps_dir).exists() {
            std::fs::create_dir_all(deps_dir)?;
        }
        // 2. use git to download the repo
        let url = format!("{}", repo);
        let path = format!("{}/{}", deps_dir, name);
        let output = std::process::Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(path.clone())
            .output()?;
        if output.status.success() {
            println!("Downloaded dependency {} from {}", name, repo);
        }
        // 3. TODO: checkout wanted version
        Ok(())
    }

    pub fn exe_path(&self) -> String {
        // TODO: check Debug/Release, check exe name
        let mode = "Debug";
        // find first exe in targets
        for target in &self.targets {
            match target.kind {
                TargetKind::App => {
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
    use autoval::Obj;
    #[test]
    fn test_pac() {
        let code = r#"
        name: "test"
        version: "0.0.1"

        exe("main") {}
        "#;
        let config = AutoConfig::from_code(code, &Obj::new());
        match config {
            Ok(config) => {
                let pac = Pac::new(config);
                assert_eq!(pac.name, "test");
                assert_eq!(pac.version, "0.0.1");
                assert_eq!(pac.targets.len(), 1);
                assert_eq!(pac.targets[0].name, "main");
                assert_eq!(pac.targets[0].kind, TargetKind::App);
            }
            Err(e) => {
                assert!(false, "{}", e);
            }
        }
    }

    #[test]
    fn test_tabled() {
        let code = r#"
        name: "test"
        app("main") {}
        lib("lib") {}
        "#;
        let config = AutoConfig::from_code(code, &Obj::new());
        let pac = Pac::new(config.unwrap());
        pac.print_targets();
    }
}
