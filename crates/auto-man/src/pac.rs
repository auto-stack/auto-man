use crate::builder::new_builder;
use crate::builder::Builder;
use crate::builder::BuilderKind;
use crate::git::check_changes;
use crate::git::check_detached;
use crate::git::pull;
use crate::git::switch_to_master;
use crate::AutoResult;
use crate::Port;
use crate::{Index, PacInfo};
use crate::{Target, TargetKind, TargetStatus};
use auto_atom::Atom;
use auto_lang::config::AutoConfig;
use auto_lang::Universe;
use auto_val::NodeBody;
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
    pub port: Port,
    pub version: AutoStr,
    pub ports: Vec<Node>,
    pub builder: Option<Box<dyn Builder>>,
    pub targets: Vec<Target>,
    pub index: Shared<Index>,
    pub device_index: Shared<Index>,
    pub props: Obj,
    pub build_at: AutoStr,
    pub build_location: AutoStr,
    /// Whether to show header files in `srcs`
    pub show_headers: bool,
    pub defines: Vec<AutoStr>,
    is_update: bool,
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
        let pac_name = config.name();
        let version = config.version();
        let props = config.root.props_clone();

        // ports
        let mut ports = config.root.get_nodes("port");
        if ports.is_empty() {
            let mut port = Node::new("port");
            port.add_arg(Arg::Pos("win32".into()));
            port.set_prop("name", "win32");
            port.set_prop("builder", "cmake");
            port.set_prop("platform", "win11");
            ports.push(port);
        }
        let builder = None;

        // defines
        let mut defines = vec![];
        if config.root.has_prop("defines") {
            defines = config.root.get_prop("defines").to_str_vec();
        }

        // target related properties
        let target_props = vec!["at"];

        // targets, NOTE: ports are not targets
        let mut targets = vec![];
        for mut n in config.root.nodes.into_iter() {
            if n.name != "port" {
                // main target should receive target related args from the pac
                if n.main_arg().to_astr() == pac_name && n.name == "device" {
                    for p in &target_props {
                        if config.args.has(*p) {
                            let value = config.args.get(*p).unwrap();
                            println!("Setting prop: {}: {}", *p, value.clone());
                            // TODO: find a better way to transfer args to dependencies target
                            n.set_prop(*p, value);
                        }
                    }
                }

                let mut target = Target::from(n, pac_name.clone());
                target.set_defines(defines.clone());
                targets.push(target);
            }
        }

        Self {
            name: pac_name,
            version,
            port: Port::default(),
            ports,
            builder,
            targets,
            props,
            index: shared(Index::default()),
            device_index: shared(Index::default()),
            build_at: ".".into(),
            build_location: "cmake".into(),
            show_headers: false,
            defines,
            is_update: false,
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

    pub fn print_target_info(&self, target: &str) -> AutoResult<()> {
        if let Some(target) = self.targets.iter().find(|t| t.name == target) {
            target.print_info()
        } else {
            return Err("Target not found".into());
        }
    }

    // print targets as a table
    pub fn print_targets(&self) {
        info!("--- pac: {} ---\n", self.name);
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

    /// Print atom files to local directory
    pub fn print_atom(&self) -> AutoResult<()> {
        let atom = self.to_atom();
        // info!("ATOM packed for pac: <{}>", self.name);
        // println!("{}", atom);
        let atom_pretty = pretty(atom.to_string().as_str(), 3);
        // info!("ATOM for pac: <{}>", self.name);
        // write prettified atom to file
        // write prettified atom to file
        std::fs::write(".am/pac.atom.at", atom_pretty.as_bytes())?;

        for app in self.build_targets() {
            let name = app.name.clone();

            let app_atom = app.to_atom();
            let app_atom_pretty = pretty(app_atom.to_string().as_str(), 3);

            std::fs::write(format!(".am/{}.atom.at", name), app_atom_pretty.as_bytes())?;
        }

        Ok(())
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

    pub fn libs(&self) -> Vec<&Target> {
        self.targets
            .iter()
            .filter(|t| t.kind == TargetKind::Lib)
            .collect()
    }

    pub fn apps(&self) -> Vec<&Target> {
        self.targets
            .iter()
            .filter(|t| t.kind == TargetKind::App)
            .collect()
    }

    pub fn build_targets_mut(&mut self) -> Vec<&mut Target> {
        self.targets
            .iter_mut()
            .filter(|t| t.kind == TargetKind::App || t.kind == TargetKind::Lib)
            .collect()
    }

    pub fn build_targets(&self) -> Vec<&Target> {
        self.targets
            .iter()
            .filter(|t| t.kind == TargetKind::App || t.kind == TargetKind::Lib)
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

        let port_name = state.root.get_prop("port").to_astr();
        if !port_name.is_empty() {
            self.set_port(port_name)?;
        } else {
            let ports = self.list_port_names();
            let port_name = select_port(None, &ports)?;
            self.set_port(port_name.clone())?;
            self.save_port(port_name)?;
        }

        Ok(())
    }

    pub fn save_port(&mut self, port: AutoStr) -> AutoResult<()> {
        let state_file = self.get_state_file()?;
        let mut state = AutoConfig::read(&state_file.path())?;
        state.root.set_prop("port", port.clone());
        state.save(&state_file)?;
        Ok(())
    }

    pub fn set_port(&mut self, port_name: AutoStr) -> AutoResult<()> {
        // 0. check port
        let port_node = self
            .ports
            .iter()
            .find(|p| p.main_arg().to_astr() == port_name);
        let Some(port_node) = port_node else {
            return Err(format!("Port {} not found", port_name).into());
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
        self.build_location = AutoPath::new(at.clone()).to_astr();
        // merge configs within the port into the pac
        self.merge_port(port_name.clone())?;
        // set builder
        let build_path = AutoPath::new(self.build_location.clone());
        match self.port.builder.as_str() {
            "cmake" => {
                let path = build_path.join("CMakeLists.txt");
                self.builder = Some(new_builder(BuilderKind::CMake(path.to_string())));
            }
            "iar" => {
                self.builder = Some(new_builder(BuilderKind::IAR(build_path)));
            }
            "ghs" => {
                self.builder = Some(new_builder(BuilderKind::GHS(build_path)));
                self.set_show_headers();
            }
            "ninja" => {
                let path = build_path.join("build/build.ninja");
                self.builder = Some(new_builder(BuilderKind::Ninja(build_path.join(path))));
                self.set_show_headers();
            }
            _ => {
                panic!(
                    "Invalid port builder name: {}:{}",
                    self.port.name, self.port.builder
                );
            }
        }
        Ok(())
    }

    pub fn set_show_headers(&mut self) {
        self.show_headers = true;
    }

    pub fn copy_target(&self, id: &AutoStr) -> Option<Target> {
        for t in &self.targets {
            if t.name == *id {
                return Some(t.clone());
            }
        }
        None
    }

    pub fn targets_map(&mut self) -> HashMap<AutoStr, Target> {
        let mut targets_map = HashMap::new();
        let targets = std::mem::take(&mut self.targets);
        for target in targets {
            targets_map.insert(target.name.clone(), target.clone());
        }
        targets_map
    }

    pub fn get_build_names(targets_map: &HashMap<AutoStr, Target>) -> Vec<AutoStr> {
        let mut build_names = Vec::new();
        for target in targets_map.values() {
            if target.kind == TargetKind::App || target.kind == TargetKind::Lib {
                build_names.push(target.name.clone());
            }
        }
        build_names
    }

    fn resolve_target_deps(targets_map: &mut HashMap<AutoStr, Target>) -> bool {
        let mut all_resolved = true;
        // let mut resolved: HashMap<AutoStr, Vec<AutoStr>> = HashMap::new();
        let mut resolved_links: HashMap<AutoStr, Vec<Node>> = HashMap::new();
        for t in targets_map.values() {
            if t.links.is_empty() {
                // resolved.insert(t.name.clone(), vec![]);
                resolved_links.insert(t.name.clone(), vec![]);
            } else {
                // try all links, if all links are resolved, copy their dependencies
                let mut all_deps_resolved = true;
                // let mut link_names = Vec::new();
                let mut links = Vec::new();
                // link_names.extend(t.links.iter().map(|t| t.id()));
                links.extend(t.links.iter().map(|t| t.clone()));
                for l in &t.links {
                    let Some(dep) = targets_map.get(&l.id()) else {
                        continue;
                    };
                    if !dep.is_resolved {
                        all_deps_resolved = false;
                        break;
                    } else {
                        // link_names.extend(dep.links.iter().map(|t| t.id()));
                        links.extend(dep.links.iter().map(|t| t.clone()));
                        // links.extend_from_slice(&dep.links);
                    }
                }
                if all_deps_resolved {
                    // resolved.insert(t.name.clone(), link_names);
                    resolved_links.insert(t.name.clone(), links);
                    // resolved.insert(t.name.clone(), links);
                }
            }
            if t.is_resolved == false {
                all_resolved = false;
            }
        }
        // println!("link_names: [{}] {:?}", resolved.len(), resolved);
        // println!("links: [{}]", resolved_links.len());
        // for (k, links) in &resolved_links {
        //     println!(
        //         "{}:{}",
        //         k,
        //         links.iter().map(|l| l.id()).collect::<Vec<_>>().join(", ")
        //     );
        // }
        // println!(">>>>>>>>>>>>>>>>>>> resolved: {:?}", resolved);
        for (k, links) in resolved_links {
            let mut deps = Vec::new();
            for l in &links {
                let id = l.id();
                // let id = l.clone();
                let Some(dep) = targets_map.get(&id) else {
                    continue;
                };
                deps.push(dep.name.clone());
                // let mut merge = dep.clone();
                // merge.merge_link(&l);
                // deps.push(merge);
            }
            let Some(t) = targets_map.get_mut(&k) else {
                continue;
            };
            t.is_resolved = true;
            t.dep_names = deps;
            t.all_links = links;
            // t.all_deps = deps;
        }
        // for t in targets_map.values() {
        // println!("Target: {}", t.name);
        // println!(
        //     "deps: {}",
        //     t.deps
        //         .iter()
        //         .map(|t| t.name.clone())
        //         .collect::<Vec<AutoStr>>()
        //         .join(", ")
        // );
        // }
        all_resolved

        // let build_names = Self::get_build_names(&targets_map);

        // for n in build_names {
        //     let Some(mut target) = targets_map.remove(&n) else {
        //         continue;
        //     };
        //     for link in &target.links {
        //         let id = link.id();
        //         let Some(dep) = targets_map.get(&id) else {
        //             continue;
        //         };
        //         target.deps.push(dep.clone());
        //     }

        //     targets_map.insert(target.name.clone(), target.clone());
        // }

        // for n in build_names {
        //     // take out the targett from targets_map
        //     let mut target = targets_map.remove(&n);
        //     let Some(mut target) = target else {
        //         error!("Target {} not found", n);
        //         continue;
        //     };

        //     let links = target.links;
        //     let mut link_targets = Vec::new();
        //     // check if a link is missing
        //     for link in &links {
        //         let id = link.id();
        //         let dep = targets_map.get(&id);
        //         match dep {
        //             Some(dep) => {
        //                 let mut dep = dep.clone();
        //                 dep.apply_link(&link);
        //                 link_targets.push(dep.clone());
        //             }
        //             None => {
        //                 error!("Target {} not found", id);
        //             }
        //         }
        //     }
        //     target.links = links;
        //     target.deps = link_targets;
        //     targets_map.insert(target.name.clone(), target.clone());
        // }
    }

    pub fn pull(&mut self) -> AutoResult<()> {
        self.is_update = true;
        self.download()?;
        self.is_update = false;
        Ok(())
    }

    pub fn download(&mut self) -> AutoResult<()> {
        info!("Downloading deps");
        self.download_deps(&AutoStr::new(), &AutoStr::new())?;
        println!("download done!");
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

        // 1. download dependencies, recursively
        self.download()?;

        let rel = self.relative_path();
        for t in &mut self.targets {
            t.set_rel(rel.clone());
            t.set_defines(self.defines.clone());
            t.set_port(self.port.clone());
        }

        self.print_targets();

        // 2. link targets with dependencies
        info!("Linking targets with dependencies");

        // let devices: Vec<Target> = self
        //     .targets
        //     .iter()
        //     .filter(|t| t.kind == TargetKind::Device)
        //     .map(|t| t.clone())
        //     .collect();
        let mut targets_map = self.targets_map();
        let mut major_targets = Vec::new();
        // remained_targets.extend(devices);

        // resolve target's links, expanding indirect dependencies into target.dep_names
        let mut done = false;
        while !done {
            done = Self::resolve_target_deps(&mut targets_map);
        }

        let major_target_names = targets_map
            .values()
            .filter(|t| t.is_major())
            .map(|t| t.name.clone())
            .collect::<Vec<AutoStr>>();

        for n in major_target_names {
            let mut deps = Vec::new();
            let t = targets_map.get(&n).unwrap();
            for l in &t.all_links {
                let Some(dep) = targets_map.get(&l.id()) else {
                    continue;
                };
                let mut merge = dep.clone();
                merge.merge_link(l);
                deps.push(merge);
            }
            // for dep in &t.dep_names {
            //     let Some(dep) = targets_map.get(dep) else {
            //         continue;
            //     };
            //     deps.push(dep.clone());
            // }
            let Some(t) = targets_map.get_mut(&n) else {
                continue;
            };
            t.deps = deps;
        }

        // copy dep's target info into major targets
        targets_map
            .values()
            .filter(|t| {
                t.kind == TargetKind::Device
                    || t.kind == TargetKind::App
                    || t.kind == TargetKind::Lib
            })
            .for_each(|t| major_targets.push(t.clone()));

        self.targets = major_targets;

        // 3. scan targets for its local source files
        info!("Scanning targets for srcs and incs");
        let show_headers = self.show_headers;
        for target in &mut self.targets {
            target.show_headers = show_headers;
            target.scan()?;
        }
        Ok(())
    }

    fn merge_port(&mut self, port: AutoStr) -> AutoResult<()> {
        // reload config with said port
        let port_node = self.ports.iter().find(|p| p.main_arg().to_astr() == port);

        let Some(port_node) = port_node else {
            self.port = Port::default();
            return Ok(());
        };

        // props
        let props = port_node.props_clone();
        self.props.merge(&props);

        // targets
        for n in port_node.nodes.clone() {
            let target = Target::from(n, self.name.clone());
            self.targets.push(target);
        }

        let mut port = Port::default();
        port.name = port_node.main_arg().to_astr();
        port.builder = port_node.get_prop("builder").to_astr();
        port.platform = port_node.get_prop("platform").to_astr();
        port.at = port_node.get_prop("at").to_astr();
        self.port = port;
        Ok(())
    }

    fn update_repo(&self, path: &AutoPath) -> AutoResult<()> {
        warn!("trying to update repo {}", path);

        check_changes(path)?;
        match check_detached(path) {
            Ok(_) => {}
            Err(_) => {
                // switch to master
                switch_to_master(path)?;
            }
        }

        pull(path)
    }

    pub fn contains(&self, name: &AutoStr) -> bool {
        self.targets.iter().any(|t| t.name == *name)
    }

    fn download_deps(&mut self, parent: &AutoStr, version: &AutoStr) -> AutoResult<()> {
        let mut targets = std::mem::take(&mut self.targets);
        let mut new_deps = HashMap::new();
        let mut target_names = HashMap::new();
        for t in targets.iter() {
            target_names.insert(t.name.clone(), true);
        }
        for target in targets.iter_mut() {
            // override target's at location
            if !parent.is_empty() {
                target.set_parent(parent);
            };
            let loc = target.location();

            match target.kind {
                TargetKind::Dep | TargetKind::Device => {
                    // 1. update if exists
                    if target.exists() {
                        continue;
                    }

                    if version == "latest" {
                        target.version = "latest".into();
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
                        // env
                        Universe::default(),
                    )?;
                    let mut pac = Pac::new(dep_config);
                    pac.index = self.index.clone();
                    pac.device_index = self.device_index.clone();
                    pac.is_update = self.is_update;
                    // get the main target of the package
                    let dep_name = target.name.clone();
                    let lib_target = pac.get_target(dep_name);
                    if let Some(lib_target) = lib_target {
                        // merge the lib target into current dep target
                        target.merge(lib_target);
                    }
                    // get other lib targetsm
                    for lib in pac.libs() {
                        if lib.name == target.name {
                            continue;
                        }
                        if !target_names.contains_key(&lib.name) {
                            let mut lib = lib.clone();
                            // check new root dir
                            for (_, v) in lib.dirs.iter_mut() {
                                let path = Path::new(target.at.as_str()).join(v.at.as_str());
                                if path.is_dir() {
                                    v.append_root(target.at.clone());
                                }
                            }

                            new_deps.insert(lib.name.clone(), lib);
                        }
                    }

                    // 4. recursively download dep's deps
                    pac.download_deps(&target.parent(), &target.version.clone())?;
                    // merge dep's dep into current pac if not exists
                    for dep in pac.deps() {
                        // if !new_deps.contains_key(&dep.name)
                        // && !target_names.contains_key(&dep.name)
                        if !target_names.contains_key(&dep.name) {
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
                self.download_repo(&info, &dep.kind, &at)?;
                return Ok(());
            }
            None => {
                return Err(format!("Dependency {} not found", name).into());
            }
        }
    }

    pub fn download_repo(&self, info: &PacInfo, kind: &TargetKind, at: &AutoStr) -> AutoResult<()> {
        // 1. make deps folder if not exists
        let mut deps_dir = match kind {
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
            _ => return Err(format!("Invalid target kind: {}", kind).into()),
        };
        if !at.is_empty() {
            deps_dir = at.as_str();
        }
        // 2. use git to download the repo
        let url = format!("{}", info.repo);
        let path = deps_dir;
        info!("- {}: ({})", path, info.repo);
        if Path::new(&path).exists() {
            if self.is_update {
                let p = AutoPath::new(path);
                self.update_repo(&p)?;
            }
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
                .arg(path)
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
            if version == "latest" {
                version = "master".into();
            } else {
                if !version.starts_with("v") {
                    version = format!("v{}", version.as_str()).into();
                }
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
        // 1. remove build directory
        // TODO: what about other builders that are not choosed yet?
        let builder = std::mem::take(&mut self.builder);
        if let Some(mut builder) = builder {
            builder.clean()?;
            self.builder = Some(builder);
        }
        // 2. remove downloaded deps and devices

        // NOTE: if content is modified (yet to be committed), a warning should be displayed and the directory should be left
        // for t in self.downloaded() {
        // t.clean()?;
        // }
        Ok(())
    }

    pub(crate) fn collect_srcs(&mut self) -> AutoResult<()> {
        for t in self.targets.iter_mut() {
            t.collect_srcs()?;
        }
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

    pub fn all_incs(&self) -> Vec<AutoStr> {
        let mut all_incs = vec![];
        for target in &self.targets {
            all_incs.extend(target.incs.clone());
        }
        all_incs.sort();
        all_incs
    }

    pub fn to_nodebody(&self) -> NodeBody {
        let mut nb = NodeBody::new();
        // name prop is the first positional arg
        nb.add_prop("id", self.name.clone());
        nb.add_prop("name", self.name.clone());
        nb.add_prop("version", self.version.clone());
        let mut all_incs = self.all_incs();
        for target in &self.targets {
            nb.add_kid(target.to_node());
        }
        all_incs.sort();
        nb.add_prop("all_incs", Value::Array(Array::from_vec(all_incs)));

        if !self.has_target(TargetKind::Dep) {
            nb.add_prop("deps", Value::empty_array());
        }

        if !self.has_target(TargetKind::Device) {
            nb.add_prop("devices", Value::empty_array());
        }

        if !self.has_target(TargetKind::Lib) {
            nb.add_prop("libs", Value::empty_array());
        }

        if !self.has_target(TargetKind::App) {
            nb.add_prop("apps", Value::empty_array());
        }

        // root.set_prop("files", self.files_list());
        // root.set_prop("groups", self.group_list());
        nb.add_prop("files", Array::new());
        // nb.add_prop("groups", self.groups().to_xml());

        let props = self.props.clone();
        for (k, v) in props.iter() {
            nb.add_prop(k.to_string().as_str(), v.clone());
        }

        // set builder folder
        nb.add_prop("relative_path", self.relative_path());
        nb
    }

    pub fn to_node(&self) -> Node {
        let mut root = Node::new("root");
        // name prop is the first positional arg
        root.add_arg(Arg::Pos(Value::Str(self.name.clone())));
        root.set_prop("version", self.version.clone());
        let mut all_incs = self.all_incs();
        for target in &self.targets {
            root.add_kid(target.to_node());
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
        // root.set_prop("groups", self.groups().to_xml());

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
        "/..".repeat(depth).into()
    }

    // fn group_target(&self, g: &mut Group, t: &Target) {
    //     // set files
    //     for src in &t.srcs {
    //         let fpath = format!("$PROJ_DIR${}\\{}", &self.relative_path(), src);
    //         g.files.push(crate::group::File { name: fpath.into() })
    //     }
    //     // set dirs
    //     for (_n, d) in &t.dirs {
    //         let dg = g.mut_kid(&d.name);
    //         self.group_dir(dg, d);
    //     }
    //     // set deps
    //     for dep in &t.deps {
    //         let dg = g.mut_kid(&dep.name);
    //         self.group_target(dg, dep);
    //     }
    // }

    // fn groups(&self) -> Group {
    //     let mut group = Group::new("root");
    //     for t in &self.targets {
    //         let loc = t.location();
    //         let g = group.mut_kid_path(&loc);
    //         self.group_target(g, t);
    //     }
    //     group
    // }

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
        let mut atom = Atom::node_body(self.to_nodebody());
        atom.name = self.name.clone();
        atom
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

        app("main") {}
        "#;
        let config = AutoConfig::new(code);
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
        let config = AutoConfig::new(code);
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
