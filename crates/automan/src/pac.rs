use auto_lang::config::AutoConfig;
use std::path::Path;
use auto_val::{AutoStr, Obj, Node, pretty, Arg, Value, AutoPath, Array, ValueKey};
use crate::AutoResult;
use crate::{Target, TargetKind, TargetStatus};
use log::*;
use std::fmt;
use std::collections::HashMap;
use crate::builder::Builder;
use crate::builder::BuilderKind;
use crate::builder::new_builder;
use tabled::{
    Table,
    settings::{
        style::Style,
        object::Rows,
        themes::Colorization,
        Color,
    },
};
use auto_atom::Atom;

pub struct PacInfo {
    pub name: AutoStr,
    pub version: AutoStr,
    pub repo: AutoStr,
}

pub struct Index {
    map: HashMap<AutoStr, PacInfo>,
}

impl Index {
    pub fn load(path: AutoStr) -> AutoResult<Self> {
        let path = Path::new(path.as_str());
        let mut map = HashMap::new();
        if !path.exists() {
            return Err(format!("index file {} not found", path.display()).into());
        }
        let code = std::fs::read_to_string(path)?;
        let config = AutoConfig::from_code(&code, &Obj::new())?;
        // config is a bunch of index nodes
        let root = config.root;
        root.nodes.iter().filter(|n| n.name == "pac" || n.name == "device").for_each(|n| {
            let name = n.id();
            let version = n.get_prop("version").to_astr();
            let repo = n.get_prop("repo").to_astr();
            map.insert(name.clone(), PacInfo { name, repo, version });
        });

        Ok(Index { map })
    }

    pub fn lookup(&self, name: &AutoStr) -> Option<&PacInfo> {
        self.map.get(name)
    }
}

pub struct Pac {
    pub name: AutoStr,
    pub port: AutoStr,
    pub version: AutoStr,
    pub ports: Vec<Node>,
    pub builder: Option<Box<dyn Builder>>,
    pub targets: Vec<Target>,
    pub index: Index,
    pub device_index: Index,
    pub props: Obj,
}

// TODO: read from default locations by OS and Env
const INDEX_PATH: &str = "D:/am/index.at";
const DEVICE_INDEX_PATH: &str = "D:/am/devices.at";

impl Pac {
    pub fn new(config: AutoConfig) -> Self {
        let mut targets = vec![];
        let pac_name = config.name();
        let version = config.version();
        let props = config.root.props.clone();

        // ports
        let mut ports = config.root.get_nodes("port");
        if ports.is_empty() {
            let mut port = Node::new("port");
            port.add_arg(Arg::Pos("cmake".into()));
            ports.push(port);
        }
        let builder = None;
        // targets, NOTE: ports are not targets
        for n in config.root.nodes.into_iter() {
            if n.name != "port" {
                let target = Target::from(n, pac_name.clone());
                targets.push(target);
            }
        }

        Self {
            name: pac_name,
            version,
            port: "cmake".into(),
            ports,
            builder,
            targets,
            props,
            index: Index::load(INDEX_PATH.into()).unwrap(),
            device_index: Index::load(DEVICE_INDEX_PATH.into()).unwrap(),
        }
    }

    pub fn transpile_autot(&mut self) -> AutoResult<()> {
        for target in &mut self.targets {
            if target.has_auto() {
                target.transpile_auto()?;
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

    pub fn has_device_prop(&self, prop: impl Into<AutoStr>) -> bool {
        let mut has_prop = false;
        // TODO: too much clone for key
        let key = ValueKey::Str(prop.into());
        for target in &self.targets {
            if target.kind == TargetKind::Device {
                if target.props.has(key.clone()) {
                    has_prop = true;
                    break;
                }
            }
        }
        has_prop
    }

    pub fn print_atom(&self) {
        let atom = self.to_atom();
        // info!("ATOM packed for pac: <{}>", self.name);
        // println!("{}", atom);
        let atom_p = pretty(atom.to_string().as_str(), 3);
        info!("ATOM for pac: <{}>", self.name);
        println!("{}", atom_p);
    }

    pub fn get_target(&self, name: impl Into<AutoStr>) -> Option<&Target> {
        let name = name.into();
        self.targets.iter().find(|t| t.name == name)
    }

    pub fn deps(&self) -> Vec<&Target> {
        self.targets.iter().filter(|t| t.kind == TargetKind::Dep).collect()
    }

    pub fn list_port_names(&self) -> Vec<AutoStr> {
        self.ports.iter().map(|p| p.main_arg().to_astr()).collect()
    }

    pub fn set_port(&mut self, port: AutoStr) -> AutoResult<()> {
        // 0. check port
        if self.ports.iter().find(|p| p.main_arg().to_astr() == port).is_none() {
            return Err(format!("Port {} not found", port).into());
        }
        // merge configs within the port into the pac
        self.merge_port(port.clone())?;
        // set builder
        match port.as_str() {
            "cmake" => {
                self.builder = Some(new_builder(BuilderKind::CMake("CMakeLists.txt".to_string())));
            }
            "iar" => {
                self.builder = Some(new_builder(BuilderKind::IAR(AutoPath::new("iar"))));
            }
            _ => {
                panic!("Invalid port name: {}", port);
            }
        }
        Ok(())
    }

    pub fn resolve(&mut self) -> AutoResult<()> {
        // 0. check basic props
        if !self.props.has("defines") {
            let defines = Vec::<AutoStr>::new();
            // defines.push("DEBUG".into());
            // defines.push("USE_KERNEL".into());
            self.props.set("defines", defines);
        }
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

    fn merge_port(&mut self, port: AutoStr) -> AutoResult<()> {
        let port_node = self.ports.iter().find(|p| p.main_arg().to_astr() == port).unwrap();
        // props
        let props = port_node.props.clone();
        self.props.merge(&props);

        // targets
        for n in port_node.nodes.clone() {
            let target = Target::from(n, self.name.clone());
            self.targets.push(target);
        }
        Ok(())
    }

    fn update_repo(&self, path: &str) -> AutoResult<()> {
        let output = std::process::Command::new("git")
            .arg("pull")
            .arg(path)
            .output()?;
        if output.status.success() {
            info!("updated dependency {}", path);
        }
        Ok(())
    }

    pub fn contains(&self, name: &AutoStr) -> bool {
        self.targets.iter().any(|t| t.name == *name)
    }

    pub fn download_deps(&mut self) -> AutoResult<()> {
        let mut targets = std::mem::take(&mut self.targets);
        let mut new_deps = HashMap::new();
        for target in targets.iter_mut() {
            match target.kind {
                TargetKind::Dep => {
                    if target.exists() {
                        // git update
                        let path = format!("deps/{}", target.name);
                        self.update_repo(&path)?;
                        
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
                    let mut pac = Pac::new(dep_config);
                    // get the lib target
                    let dep_name = target.name.clone();
                    let lib_target = pac.get_target(dep_name);
                    if let Some(lib_target) = lib_target {
                        // merge the lib target into current dep target
                        target.merge(lib_target);
                    }
                    pac.download_deps()?;
                    // merge dep's dep into current pac if not exists
                    for dep in pac.deps() {
                        if !new_deps.contains_key(&dep.name) {
                            new_deps.insert(dep.name.clone(), dep.clone());
                        }
                    }
                }
                TargetKind::Device => {
                    if target.exists() {
                        // git update
                        let path = format!("devices/{}", target.name);
                        self.update_repo(&path)?;
                        continue;
                    }
                    self.download_device(&target)?;
                    // get args from dep node props
                    // read pac.at from dep and parse the lib target
                    info!("reading device with override props: {:?}", target.props);
                    let dep_config = AutoConfig::from_file(
                        Path::new(format!("devices/{}/device.at", target.name).as_str()),
                        // override dep props
                        &target.props)?;
                    let pac = Pac::new(dep_config);
                    // TODO: recursively download dep targets from the dep
                    // get the lib target
                    let dep_name = target.name.clone();
                    let dev_target = pac.get_target(dep_name);
                    if let Some(dev_target) = dev_target {
                        // merge the dev target into current dev target
                        info!("depending dev root: {}", dev_target.root);
                        target.merge(dev_target);
                        info!("device root updated: {}", target.root);
                    }
                }
                _ => {}
            }
        }
        self.targets = targets;
        for (_, dep) in new_deps {
            self.targets.push(dep);
        }
        Ok(())
    }

    fn download_device(&mut self, device: &Target) -> AutoResult<()> {
        info!("[download] device: {}", device.name);
        // 查询索引
        let name = device.name.clone();
        let info = self.device_index.lookup(&name);
        match info {
            Some(info) => {
                self.download_repo(&info.name, &info.repo, &device.kind)?;
                return Ok(());
            }
            None => {
                return Err(format!("Device {} not found", name).into());
            }
        }
    }

    // 下载dep的源码包
    pub fn download_dep(&mut self, dep: &Target) -> AutoResult<()> {
        // 查询索引
        let name = dep.name.clone();
        println!("looking up dependency from index: {}", name);
        let info = self.index.lookup(&name);
        match info {
            Some(info) => {
                self.download_repo(&info.name, &info.repo, &dep.kind)?;
                return Ok(());
            }
            None => {
                return Err(format!("Dependency {} not found", name).into());
            }
        }
    }

    pub fn download_repo(&self, name: &AutoStr, repo: &AutoStr, kind: &TargetKind) -> AutoResult<()> {
        println!("Downloading dependency from {}", repo);
        // 1. make deps folder if not exists
        let deps_dir = match kind {
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
            _ => return Err(format!("Invalid target kind: {}", kind).into()),
        };
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

    pub fn build(&mut self) -> AutoResult<()> {
        let builder = std::mem::take(&mut self.builder);
        if let Some(mut builder) = builder {
            builder.build(self)?;
            self.builder = Some(builder);
        }
        Ok(())
    }

    pub fn run(&mut self) -> AutoResult<()> {
        let builder = std::mem::take(&mut self.builder);
        if let Some(mut builder) = builder {
            builder.run(self)?;
            self.builder = Some(builder);
        } else {
            let mut builder = new_builder(BuilderKind::CMake("CMakeLists.txt".to_string()));
            builder.run(self)?;
        }
        Ok(())
    }

    pub fn clean(&mut self) -> AutoResult<()> {
        let builder = std::mem::take(&mut self.builder);
        if let Some(mut builder) = builder {
            builder.clean()?;
            self.builder = Some(builder);
        }
        Ok(())
    }

}
impl Pac {
    pub fn to_node(&self) -> Node {
        let mut root = Node::new("root");
        // name prop is the first positional arg
        root.add_arg(Arg::Pos(Value::Str(self.name.clone())));
        root.set_prop("version", self.version.clone());
        let mut all_incs = vec![];
        for target in &self.targets {
            root.add_kid(target.to_node());

            // incs
            all_incs.extend(target.incs.clone());
        }
        all_incs.sort();
        root.set_prop("all_incs", Value::Array(Array::from_vec(all_incs)));

        let props = self.props.clone();
        for (k, v) in props.iter() {
            root.set_prop(k.to_string().as_str(), v.clone());
        }
        root
    }

    pub fn to_atom(&self) -> Atom {
        Atom::node(self.to_node())
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
    use auto_val::Obj;
    #[test]
    fn test_pac() {
        let code = r#"
        name: "test"
        version: "0.0.1"

        app("main") {}
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
        let atom = pac.to_atom();
        println!("{}", atom);
    }
}
