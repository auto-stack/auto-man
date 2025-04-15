use crate::builder::new_builder;
use crate::builder::Builder;
use crate::builder::BuilderKind;
use crate::AutoResult;
use crate::{Index, PacInfo};
use crate::{Target, TargetKind, TargetStatus};
use auto_atom::Atom;
use auto_lang::config::AutoConfig;
use auto_val::{pretty, Arg, Array, AutoPath, AutoStr, Node, Obj, Value, ValueKey};
use auto_val::{shared, Shared};
use log::*;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use tabled::{
    settings::{object::Rows, style::Style, themes::Colorization, Color},
    Table,
};

pub struct Pac {
    pub name: AutoStr,
    pub port: AutoStr,
    pub version: AutoStr,
    pub ports: Vec<Node>,
    pub builder: Option<Box<dyn Builder>>,
    pub targets: Vec<Target>,
    pub index: Shared<Index>,
    pub device_index: Shared<Index>,
    pub props: Obj,
    pub build_at: AutoStr,
    pub build_location: AutoStr,
}

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
            index: shared(Index::default()),
            device_index: shared(Index::default()),
            build_at: ".".into(),
            build_location: "cmake".into(),
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
        table
            .with(Style::modern_rounded())
            .with(Colorization::exact([Color::FG_BRIGHT_BLUE], Rows::first()));

        println!("{}", table);
    }

    pub fn has_device_prop(&self, prop: impl Into<AutoStr>) -> bool {
        let mut has_prop = false;
        // TODO: too much clone for key
        let key = ValueKey::Str(prop.into());
        if self.props.has(key.clone()) {
            return true;
        }
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
        self.targets
            .iter()
            .filter(|t| t.kind == TargetKind::Dep)
            .collect()
    }

    pub fn list_port_names(&self) -> Vec<AutoStr> {
        self.ports.iter().map(|p| p.main_arg().to_astr()).collect()
    }

    pub fn set_port(&mut self, port: AutoStr) -> AutoResult<()> {
        println!("Setting port: {}", port);
        // 0. check port
        let port_node = self.ports.iter().find(|p| p.main_arg().to_astr() == port);
        let Some(port_node) = port_node else {
            return Err(format!("Port {} not found", port).into());
        };
        let mut at = if port_node.has_prop("at") {
            port_node.get_prop_of("at").to_astr()
        } else {
            ".".into()
        };
        if at.ends_with("/") {
            at = at[0..at.len() - 1].into()
        }
        self.build_at = at.clone();
        self.build_location = AutoPath::new(at.clone()).join(port.clone()).to_astr();
        // merge configs within the port into the pac
        self.merge_port(port.clone())?;
        // set builder
        match port.as_str() {
            "cmake" => {
                let path = AutoPath::new(at).join("CMakeLists.txt");
                self.builder = Some(new_builder(BuilderKind::CMake(path.to_string())));
            }
            "iar" => {
                let path = AutoPath::new(at).join("iar");
                self.builder = Some(new_builder(BuilderKind::IAR(path)));
            }
            _ => {
                panic!("Invalid port name: {}", port);
            }
        }
        self.port = port.clone();
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
        self.download_deps(&AutoStr::new())?;

        self.print_targets();
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
        let port_node = self
            .ports
            .iter()
            .find(|p| p.main_arg().to_astr() == port)
            .unwrap();
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

    fn update_repo(&self, path: &AutoPath) -> AutoResult<()> {
        let output = std::process::Command::new("git")
            .arg("pull")
            .arg(path.to_string())
            .output()?;
        if output.status.success() {
            info!("updated dependency {}", path);
        }
        Ok(())
    }

    pub fn contains(&self, name: &AutoStr) -> bool {
        self.targets.iter().any(|t| t.name == *name)
    }

    pub fn download_deps(&mut self, at: &AutoStr) -> AutoResult<()> {
        let mut targets = std::mem::take(&mut self.targets);
        let mut new_deps = HashMap::new();
        let mut target_names = HashMap::new();
        for t in targets.iter() {
            target_names.insert(t.name.clone(), true);
        }

        println!("Downloading deps with at {}", at);

        for target in targets.iter_mut() {
            // override target's at location
            if !at.is_empty() {
                target.set_at(at);
            };
            let loc = target.location();
            println!("target root; {}", target.root);

            match target.kind {
                TargetKind::Dep | TargetKind::Device => {
                    // 1. update if exists
                    if target.exists() {
                        // git update
                        self.update_repo(&loc)?;
                        continue;
                    }

                    // 2. download the dep target
                    self.download_dep(&target)?;

                    // 3. read pac.at from dep
                    // get args from dep node props
                    // read pac.at from dep and parse the lib target
                    info!("reading pac with override props: {:?}", target.props);
                    let dep_config = AutoConfig::from_file(
                        loc.join(target.config_name()).path(),
                        // override dep props
                        &target.props,
                    )?;
                    let mut pac = Pac::new(dep_config);
                    pac.index = self.index.clone();
                    pac.device_index = self.device_index.clone();
                    // get the lib target
                    let dep_name = target.name.clone();
                    let lib_target = pac.get_target(dep_name);
                    if let Some(lib_target) = lib_target {
                        // merge the lib target into current dep target
                        target.merge(lib_target);
                    }

                    // 4. recursively download dep's deps
                    pac.download_deps(&target.at)?;
                    // merge dep's dep into current pac if not exists
                    for dep in pac.deps() {
                        if !new_deps.contains_key(&dep.name)
                            && !target_names.contains_key(&dep.name)
                        {
                            println!("adding dep {} from pac {}", dep.name, pac.name);
                            new_deps.insert(dep.name.clone(), dep.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        self.targets = targets;
        for (_, dep) in new_deps {
            if !self.targets.iter().any(|t| t.name == dep.name) {
                self.targets.push(dep);
            }
        }
        Ok(())
    }

    pub fn download_dep(&mut self, dep: &Target) -> AutoResult<()> {
        // 查询索引
        let name = dep.name.clone();
        let at = dep.at.clone();
        let rename = dep.props.get_str_or("rename", name.clone());

        println!("looking up dependency from index: {}", name);
        let index = match dep.kind {
            TargetKind::Dep => self.index.borrow(),
            TargetKind::Device => self.device_index.borrow(),
            _ => return Err(format!("Invalid target kind to download: {}", dep.kind).into()),
        };
        let info = index.lookup(&name);
        match info {
            Some(info) => {
                let mut info = info.clone();
                // TODO: check if dep version is in index
                info.version = dep.version.clone();
                self.download_repo(&info, &dep.kind, &at, &rename)?;
                return Ok(());
            }
            None => {
                return Err(format!("Dependency {} not found", name).into());
            }
        }
    }

    pub fn download_repo(
        &self,
        info: &PacInfo,
        kind: &TargetKind,
        at: &AutoStr,
        rename: &AutoStr,
    ) -> AutoResult<()> {
        println!("Checking dependency from {}", info.repo);
        // 1. make deps folder if not exists
        let mut deps_dir = match kind {
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
            _ => return Err(format!("Invalid target kind: {}", kind).into()),
        };
        if !at.is_empty() {
            deps_dir = at.as_str();
        }
        if !std::path::Path::new(deps_dir).exists() {
            std::fs::create_dir_all(deps_dir)?;
        }
        // 2. use git to download the repo
        let url = format!("{}", info.repo);
        let path = format!("{}/{}", deps_dir, rename);
        println!("Downloading dep rep {} to {}", info.name, path);
        if Path::new(&path).exists() {
            // TODO: update the repo if necessary
            // TODO: or provide a command to update dependencies
            // let output = std::process::Command::new("git")
            //     .arg("-C")
            //     .arg(path.clone())
            //     .arg("pull")
            //     .arg("origin") // TODO: remote and branch, how to config?
            //     .arg("master")
            //     .arg("--tags")
            //     .output()?;
            // if output.status.success() {
            //     println!("Updated dependency {} from {}", info.name, info.repo);
            // } else {
            //     println!("Failed to update dependency {} from {}", info.name, info.repo);
            //     println!("{}", String::from_utf8_lossy(&output.stderr));
            // }
        } else {
            let output = std::process::Command::new("git")
                .arg("clone")
                .arg(url)
                .arg(path.clone())
                .output()?;
            if output.status.success() {
                println!("Downloaded dependency {} from {}", info.name, info.repo);
            } else {
                println!(
                    "Failed to download dependency {} from {}",
                    info.name, info.repo
                );
                println!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        // 3. checkout wanted version
        let mut version = info.version.clone();
        if !version.is_empty() {
            if !version.starts_with("v") {
                version = format!("v{}", version.as_str()).into();
            }
            info!("checking out {} to version {}", info.name, version);
            let mut cmd = std::process::Command::new("git");
            cmd.arg("-C")
                .arg(path)
                .arg("checkout")
                .arg(version.as_str());

            let cmd_str = format!("{:?}", cmd);
            println!("cmd: {}", cmd_str);
            let output = cmd.output()?;
            if output.status.success() {
                println!(
                    "Checked out dependency {} to version {}",
                    info.name, info.version
                );
            } else {
                println!("Failed to checkout {} to version {}", info.name, version);
                println!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
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

    pub(crate) fn update_deps(&mut self) -> AutoResult<()> {
        let mut targets = std::mem::take(&mut self.targets);
        for target in targets.iter_mut() {
            let loc = target.location();
            match target.kind {
                TargetKind::Dep | TargetKind::Device => {
                    if target.exists() {
                        // git update
                        self.update_repo(&loc)?;
                        continue;
                    }
                    self.download_dep(&target)?;
                    // TODO: recursively update dependencies
                }
                _ => {}
            }
        }
        self.targets = targets;
        Ok(())
    }
}

fn path_depth(path: &AutoStr) -> u32 {
    if path.is_empty() || path == "." {
        return 0;
    }
    path.split("/").count() as u32
}

impl Pac {
    fn has_target(&self, kind: TargetKind) -> bool {
        for t in &self.targets {
            if t.kind == kind {
                return true;
            }
        }
        false
    }

    fn has_target_with_name(&self, name: &AutoStr) -> bool {
        let mut hit = false;
        for t in self.targets.iter() {
            if t.name == *name {
                hit = true;
                break;
            }
        }
        return hit;
    }

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

        if !self.has_target(TargetKind::Dep) {
            root.set_prop("deps", Value::Array(Array::new()));
        }

        if !self.has_target(TargetKind::Device) {
            root.set_prop("devices", Value::Array(Array::new()));
        }

        if !self.has_target(TargetKind::Lib) {
            root.set_prop("libs", Value::Array(Array::new()));
        }

        if !self.has_target(TargetKind::App) {
            root.set_prop("apps", Value::Array(Array::new()));
        }

        root.set_prop("files", self.files_list());

        let props = self.props.clone();
        for (k, v) in props.iter() {
            root.set_prop(k.to_string().as_str(), v.clone());
        }

        // set builder folder
        let depth = path_depth(&self.build_at) as usize;
        let source_dir = "/..".repeat(depth + 1);
        root.set_prop("relative_path", source_dir);
        root
    }

    fn files_list(&self) -> Array {
        let mut total_list: HashMap<AutoStr, Vec<Obj>> = HashMap::new();

        for t in &self.targets {
            let loc = t.location();
            // get the first two parts of loc
            let mut head = loc.parent().filename();
            if head == t.name {
                head = ".".into();
            }
            if head.is_empty() {
                head = "main".into();
            }
            let mut target_dirs = Array::new();
            for (_, d) in t.dirs.iter() {
                let mut target_srcs = Obj::new();
                let array: Vec<Value> = d.srcs.iter().map(|x| Value::Str(x.clone())).collect();
                let array = Value::array(array);
                target_srcs.set("name", d.name.clone());
                target_srcs.set("srcs", array);
                target_dirs.push(target_srcs);
            }
            let mut target_obj = Obj::new();
            let array: Vec<Value> = t.srcs.iter().map(|x| Value::Str(x.clone())).collect();
            let array = Value::array(array);
            target_obj.set("srcs", array);
            target_obj.set("name", t.name.clone());
            target_obj.set("dirs", target_dirs);
            if total_list.contains_key(&head) {
                let array = total_list.get_mut(&head);
                if let Some(array) = array {
                    array.push(target_obj);
                }
            } else {
                let mut array = vec![];
                array.push(target_obj);
                total_list.insert(head.clone(), array);
            }
        }

        // convert hashmap to Array
        let mut array = Array::new();
        for (k, v) in total_list {
            let mut obj = Obj::new();
            obj.set("name", k);
            obj.set("targets", v);
            array.push(obj);
        }
        array
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

    #[test]
    fn test_autostr_key() {
        use auto_val::AutoStr;
        use std::collections::HashMap;
        let mut map = HashMap::new();
        let key = AutoStr::from("a");
        map.insert(key, "A");

        let key2: AutoStr = "a".into();
        assert!(map.contains_key(&key2));
    }

    #[test]
    fn test_path_depth() {
        use super::*;
        let path = "project/lse1480".into();
        assert_eq!(path_depth(&path), 2);

        let path = "project".into();
        assert_eq!(path_depth(&path), 1);

        let path = ".".into();
        assert_eq!(path_depth(&path), 0);
    }
}
