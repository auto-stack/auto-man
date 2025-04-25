use crate::builder::new_builder;
use crate::builder::Builder;
use crate::builder::BuilderKind;
use crate::group::Group;
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
use std::fs::OpenOptions;
use std::io::Write;
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

fn select_port(input: Option<String>, ports: &Vec<AutoStr>) -> AutoResult<AutoStr> {
    use dialoguer::Select;
    let port = if let Some(input) = input {
        input.into()
    } else {
        if ports.len() == 1 {
            ports[0].clone()
        } else {
            let selection = Select::new()
                .with_prompt("Which port do you want to build?")
                .default(0)
                .items(&ports)
                .interact()?;

            ports[selection].clone()
        }
    };
    Ok(port)
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

    pub fn print_atom(&self) -> AutoResult<()> {
        let atom = self.to_atom();
        // info!("ATOM packed for pac: <{}>", self.name);
        // println!("{}", atom);
        let atom_pretty = pretty(atom.to_string().as_str(), 3);
        // info!("ATOM for pac: <{}>", self.name);
        // write prettified atom to file
        // write prettified atom to file
        std::fs::write("pac.atom.at", atom_pretty.as_bytes()).map_err(|e| e.into())
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

    pub fn get_state_file(&self) -> AutoResult<AutoPath> {
        // check state file exists? .am/state.at
        let am_dir = AutoPath::new(".am");

        if !am_dir.is_dir() {
            // Create the directory if it doesn't exist
            std::fs::create_dir_all(&am_dir.path())?;
        }

        let state_file = am_dir.join("state.at");
        if !state_file.is_file() {
            // Create the state file if it doesn't exist
            std::fs::File::create(&state_file.path())?;
        }

        Ok(state_file)
    }

    pub fn try_load_port(&mut self) -> AutoResult<()> {
        // load local state from .am/state.at
        let state_file = self.get_state_file()?;
        let state = AutoConfig::read(&state_file.path())?;

        let port = state.root.props.get_str_of("port");
        if !port.is_empty() {
            self.set_port(port)?;
        } else {
            let ports = self.list_port_names();
            let port = select_port(None, &ports)?;
            self.set_port(port.clone())?;
            self.save_port(port)?;
        }

        Ok(())
    }

    pub fn save_port(&mut self, port: AutoStr) -> AutoResult<()> {
        let state_file = self.get_state_file()?;
        let mut state = AutoConfig::read(&state_file.path())?;
        state.root.props.set("port", port.clone());
        state.save(&state_file)?;
        Ok(())
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
        info!("Downloading deps");
        self.download_deps(&AutoStr::new())?;

        self.print_targets();
        // 2. link targets with dependencies
        info!("Linking targets with dependencies");
        for target in &mut self.targets {
            target.check_links()?;
        }

        // 3. scan targets for its local source files
        info!("Scanning targets for srcs and incs");
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
        let mut cmd = std::process::Command::new("git");
        cmd.arg("pull");
        cmd.current_dir(path.to_string().as_str());
        let output = cmd.output()?;
        if output.status.success() {
            info!("updated dependency {}", path);
            println!("{}", String::from_utf8_lossy(&output.stdout));
        } else {
            error!("failed to update dependency {}", path);
            println!("{}", String::from_utf8_lossy(&output.stderr));
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
        for target in targets.iter_mut() {
            // override target's at location
            if !at.is_empty() {
                target.set_at(at);
            };
            let loc = target.location();

            match target.kind {
                TargetKind::Dep | TargetKind::Device => {
                    // 1. update if exists
                    if target.exists() {
                        // git update
                        // self.update_repo(&loc)?;
                        continue;
                    }

                    // 2. download the dep target
                    self.download_dep(&target)?;

                    // 3. read pac.at from dep
                    // get args from dep node props
                    // read pac.at from dep and parse the lib target
                    let target_path = loc.join(target.config_name());
                    // info!(
                    // "reading pac {} with override props: {:?}",
                    // target_path, target.props
                    // );
                    let dep_config = AutoConfig::from_file(
                        target_path.path(),
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
        info!("- {}: ({})", path, info.repo);
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
            info!("    - checkout version {}", version);
            let mut cmd = std::process::Command::new("git");
            cmd.arg("-C")
                .arg(path)
                .arg("checkout")
                .arg(version.as_str());
            let output = cmd.output()?;
            if !output.status.success() {
                error!("Failed to checkout {} to version {}", info.name, version);
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
        info!("Building port at ./{}", self.build_location);
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
                    println!("updating target: {} at {}", target.name, loc);
                    if target.downloaded() {
                        println!("update downloaded");
                        // git update
                        self.update_repo(&loc)?;
                        continue;
                    }
                    println!("not downloaded");
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

        // root.set_prop("files", self.files_list());
        // root.set_prop("groups", self.group_list());
        root.set_prop("files", Array::new());
        root.set_prop("groups", self.groups().to_xml());

        let props = self.props.clone();
        for (k, v) in props.iter() {
            root.set_prop(k.to_string().as_str(), v.clone());
        }

        // set builder folder
        root.set_prop("relative_path", self.relative_path());
        root
    }

    fn relative_path(&self) -> AutoStr {
        let depth = path_depth(&self.build_at) as usize;
        "/..".repeat(depth + 1).into()
    }

    fn groups(&self) -> Group {
        let mut group = Group::new("root");
        for t in &self.targets {
            let loc = t.location();
            let g = group.mut_kid_path(&loc);
            // set files
            for src in &t.srcs {
                let fpath = format!("$PROJ_DIR${}\\{}", &self.relative_path(), src);
                g.files.push(crate::group::File { name: fpath.into() })
            }
            // set dirs
            for (_n, d) in &t.dirs {
                let dg = g.mut_kid(&d.name);
                for s in &d.srcs {
                    let fpath = format!("$PROJ_DIR${}\\{}", &self.relative_path(), s);
                    dg.files.push(crate::group::File { name: fpath.into() })
                }
            }
        }
        group
    }

    #[allow(dead_code)]
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
            target_obj.set("name", t.local_name());
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

    #[test]
    fn test_dir_groups() {
        use crate::group::*;
        let paths = vec![
            AutoPath::new("App"),
            AutoPath::new("App/motor"),
            AutoPath::new("App/com"),
            AutoPath::new("Bsp/mcal"),
            AutoPath::new("Bsw/xmen/can"),
            AutoPath::new("Bsw/xmen/config"),
        ];
        let group = dir_groups(paths);
        group.print_kids();
        assert_eq!(group.name, "root");
    }
}
