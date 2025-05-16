use crate::dir::{check_incs, scan_specific_srcs};
use crate::group::Group;
use crate::Dir;
use crate::{AutoResult, Port};
use auto_atom::Atom;
use auto_lang::trans::c::transpile_c;
use auto_lang::util::*;
use auto_val::{Array, NodeBody, Obj};
use auto_val::{AutoPath, AutoStr, Node, Value};
use log::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::path::Path;
use tabled::Tabled;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TargetKind {
    App,    // 应用程序
    Lib,    // 静态代码库
    Bag,    // 普通代码包，作为拼图的一部分，而不少单独的编译目标
    Dep,    // 依赖库
    Device, // 设备资源
}

impl TargetKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "app" => TargetKind::App,
            "lib" => TargetKind::Lib,
            "bag" => TargetKind::Bag,
            "dep" => TargetKind::Dep,
            "device" => TargetKind::Device,
            _ => panic!("Invalid target kind: {}", s),
        }
    }
}

impl fmt::Display for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetKind::App => write!(f, "App"),
            TargetKind::Lib => write!(f, "Lib"),
            TargetKind::Dep => write!(f, "Dep"),
            TargetKind::Device => write!(f, "Device"),
            TargetKind::Bag => write!(f, "Bag"),
        }
    }
}

#[derive(Clone)]
pub struct Target {
    pub name: AutoStr,
    pub version: AutoStr,
    pub kind: TargetKind,
    pub at: AutoStr, // root dir of the target, including its name
    pub rename: AutoStr,
    pub is_scan: bool,
    pub recurse: bool,
    pub show_headers: bool,
    pub dirs: HashMap<AutoStr, Dir>,
    // Auto Source files
    pub autos: Vec<AutoStr>,
    // C Source files
    pub srcs: HashSet<AutoStr>,
    // C header files/folders
    pub incs: HashSet<AutoStr>,
    pub selects: HashSet<AutoStr>,
    pub skips: HashSet<AutoStr>,
    pub links: Vec<Node>,
    pub is_resolved: bool,
    pub is_main: bool,
    pub props: Obj,
    pub deps: Vec<Target>,
    pub dep_names: Vec<AutoStr>,
    pub rel: AutoStr,
    pub defines: Vec<AutoStr>,
    pub port: Port,
}

pub fn set_to_vec<T: Ord>(set: HashSet<T>) -> Vec<T> {
    let mut vec: Vec<T> = set.into_iter().collect();
    vec.sort();
    vec
}

trait VecExt<T> {
    fn to_set(self) -> HashSet<T>;
}

impl<T: Hash + Eq> VecExt<T> for Vec<T> {
    fn to_set(self) -> HashSet<T> {
        self.into_iter().collect()
    }
}

pub fn get_root_dir(name: &AutoStr, kind: &TargetKind, pac: &AutoStr) -> AutoStr {
    // 现在只支持最简单的当前目录
    // 如果target的名称和pac名称相同，说明是主target，使用当前目录
    if name == pac {
        AutoStr::from(".")
    } else {
        // 否则，使用libs/apps/deps中的目录
        let target_dir = match kind {
            TargetKind::App => "apps",
            TargetKind::Lib => "libs",
            TargetKind::Bag => "bags",
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
        };
        AutoStr::from(format!("{}/{}", target_dir, name))
    }
}

fn dir_filter(name: &AutoStr, selects: &HashSet<AutoStr>, skips: &HashSet<AutoStr>) -> bool {
    if !selects.is_empty() && !selects.contains(name) {
        return false;
    }
    if !skips.is_empty() && skips.contains(name) {
        return false;
    }
    return true;
}

impl Target {
    pub fn new(name: impl Into<AutoStr>, kind: TargetKind) -> Self {
        Self {
            name: name.into(),
            version: AutoStr::new(),
            kind,
            at: AutoStr::new(),
            rename: AutoStr::default(),
            is_scan: true,
            recurse: false,
            show_headers: false,
            dirs: HashMap::new(),
            autos: Vec::new(),
            srcs: HashSet::new(),
            incs: HashSet::new(),
            selects: HashSet::new(),
            skips: HashSet::new(),
            links: Vec::new(),
            is_resolved: false,
            is_main: false,
            props: Obj::new(),
            deps: Vec::new(),
            dep_names: Vec::new(),
            rel: AutoStr::new(),
            defines: Vec::new(),
            port: Port::default(),
        }
    }

    pub fn from(node: Node, pac: AutoStr) -> Self {
        // name
        let mut name = AutoStr::new();
        let mut version = AutoStr::new();
        if !node.args.is_empty() {
            name = node.args.args[0].to_astr();
            if node.args.args.len() > 1 {
                version = node.args.args[1].to_astr();
            }
        }

        // kind
        let kind = TargetKind::from_str(&node.name);

        // is_scan
        let is_scan = if node.has_prop("scan") {
            node.props.get_bool_of("scan")
        } else {
            true
        };

        // recurse
        let recurse = if node.has_prop("recurse") {
            node.props.get_bool_of("recurse")
        } else {
            false
        };

        // at
        let mut at = if node.has_prop("at") {
            node.props.get_str_of("at")
        } else {
            get_root_dir(&name, &kind, &pac)
        };

        // parent
        let parent = if node.has_prop("parent") {
            node.props.get_str_of("parent")
        } else {
            AutoStr::default()
        };

        if !parent.is_empty() {
            at = AutoPath::new(parent.clone()).join(name.clone()).to_astr();
        }

        // rename
        let mut rename = AutoPath::new(at.clone()).filename();
        if rename.is_empty() {
            rename = name.clone();
        }

        // selects
        let mut selects = node.props.get_array_of_str("selects").to_set();
        // defaults
        let defaults = node.props.get_array_of_str("defaults").to_set();
        if !defaults.is_empty() {
            selects.extend(defaults);
        }
        // check for skips
        let skips = node.props.get_array_of_str("skips").to_set();

        // dirs
        let dirs = node.props.get_array_of_str("dirs");
        // dirs.push(".".into());
        let mut dirs: Vec<Dir> = dirs
            .into_iter()
            .filter(|d| dir_filter(d, &selects, &skips))
            .map(|d| Dir::from_str(d, at.clone(), kind.clone()))
            .collect();

        // check for dir nodes
        let dir_nodes = node.nodes(&"dir");
        let dirs_from_nodes: Vec<Dir> = dir_nodes
            .into_iter()
            .filter(|d| dir_filter(&d.main_arg().to_astr(), &selects, &skips))
            .map(|n| Dir::from_node(n, at.clone(), kind.clone()))
            .collect();
        dirs.extend(dirs_from_nodes);

        for d in &mut dirs {
            d.set_lpath(rename.clone());
        }
        // convert vec to hashmap
        let dirs = dirs
            .into_iter()
            .map(|d| (d.name.clone(), d))
            .collect::<HashMap<AutoStr, Dir>>();

        // check for srcs
        let srcs = node.props.get_array_of_str("srcs").to_set();
        // check for incs
        let incs = node.props.get_array_of_str("incs").to_set();
        // check for links
        let links = node.get_kids("link");
        // check for props (dep target only)
        let props = node.props.clone();

        let is_main = name == pac;

        // deps
        let mut deps = Vec::new();
        // devices
        node.nodes
            .iter()
            .filter(|n| n.name == "device")
            .for_each(|n| {
                //deps.push(Dep::from_node(n, at.clone(), kind.clone()));
                warn!("Got dep: {}", n.main_arg());
            });

        Self {
            name,
            version,
            kind,
            at,
            rename,
            is_scan,
            recurse,
            show_headers: false,
            dirs,
            autos: Vec::new(),
            srcs,
            incs,
            selects,
            skips,
            links,
            is_resolved: false,
            is_main,
            props,
            deps,
            dep_names: Vec::new(),
            rel: AutoStr::new(),
            defines: Vec::new(),
            port: Port::default(),
        }
    }

    pub fn location(&self) -> AutoPath {
        AutoPath::new(self.at.clone())
    }

    pub fn local_name(&self) -> AutoStr {
        self.rename.clone()
    }

    pub fn config_name(&self) -> AutoStr {
        match self.kind {
            TargetKind::Device => "device.at".into(),
            _ => "pac.at".into(),
        }
    }

    pub fn set_parent(&mut self, parent: impl Into<AutoStr>) {
        self.at = AutoPath::new(parent.into()).join(&self.rename).to_astr();
        // update root of each sub dirs
        for dir in self.dirs.iter_mut() {
            dir.1.update_root(self.at.clone());
        }
    }

    pub fn parent(&self) -> AutoStr {
        AutoPath::new(self.at.clone()).parent().to_astr()
    }

    pub fn all_incs(&self) -> Vec<AutoStr> {
        let mut all_incs = Vec::new();
        all_incs.extend(self.incs.clone());
        for target in &self.deps {
            all_incs.extend(target.incs.clone());
        }
        all_incs.sort();
        all_incs
    }

    pub fn to_nodebody(&self) -> NodeBody {
        let node_type = match self.kind {
            TargetKind::App => "app",
            TargetKind::Lib => "lib",
            TargetKind::Bag => "bag",
            TargetKind::Dep => "dep",
            TargetKind::Device => "device",
        };
        let mut nb = NodeBody::new();
        nb.add_prop("kind", node_type);
        nb.add_prop("relative_path", self.rel.clone());
        nb.add_prop("at", self.at.clone());
        nb.add_prop("name", self.local_name().clone());
        nb.add_prop("id", self.local_name().clone());
        nb.add_prop("root", self.at.clone());
        nb.add_prop("srcs", set_to_vec(self.srcs.clone()));
        nb.add_prop("incs", set_to_vec(self.incs.clone()));
        nb.add_prop("all_incs", self.all_incs());
        nb.add_prop("skips", set_to_vec(self.skips.clone()));
        for (p, v) in self.props.iter() {
            match v {
                Value::Str(s) => {
                    nb.add_prop(p.clone(), s.clone());
                }
                Value::Node(n) => {
                    if n.name == "file" {
                        // for file node, add target's root to the file's path
                        let path = n.main_arg();
                        let path = Path::new(self.at.as_str()).join(path.as_str());
                        nb.add_prop(p.clone(), path.unified());
                    } else {
                        nb.add_prop(p.clone(), n.to_astr());
                    }
                }
                _ => {
                    nb.add_prop(p.clone(), v.clone());
                }
            }
        }
        let mut dirs = Array::new();
        for dir in self.dirs.values() {
            // node.add_kid(dir.to_node());
            let array_val: Vec<Value> = dir.srcs.iter().map(|s| Value::Str(s.clone())).collect();
            let mut obj = Obj::new();
            obj.set("name", Value::Str(dir.name.clone()));
            obj.set("srcs", array_val);
            obj.set("lpath", dir.lpath.clone());
            obj.set("rpath", dir.rpath.clone());
            dirs.push(obj);
        }
        // array of objects
        nb.add_prop("dirs", dirs);

        // deps
        for dep in &self.deps {
            let n = dep.to_node();
            nb.add_kid(n);
        }

        if !self.deps.iter().any(|t| t.kind == TargetKind::Device) {
            nb.add_prop("devices", Value::empty_array());
        }

        if !self.deps.iter().any(|t| t.kind == TargetKind::Dep) {
            nb.add_prop("deps", Value::empty_array());
        }

        if !self.deps.iter().any(|t| t.kind == TargetKind::Bag) {
            nb.add_prop("bags", Value::empty_array());
        }

        if !self.deps.iter().any(|t| t.kind == TargetKind::Lib) {
            nb.add_prop("libs", Value::empty_array());
        }

        // groups for IAR
        if self.is_major() && self.port.builder == "iar" {
            nb.add_prop("groups", self.groups().to_xml());
        }
        nb.add_prop("defines", Value::str_array(self.defines.clone()));
        nb
    }

    pub fn is_major(&self) -> bool {
        self.kind == TargetKind::App || self.kind == TargetKind::Lib
    }

    pub fn to_atom(&self) -> Atom {
        let mut atom = Atom::node(self.to_nodebody());
        atom.name = self.name.clone();
        atom
    }

    pub fn to_node(&self) -> Node {
        let node_type = match self.kind {
            TargetKind::App => "app",
            TargetKind::Lib => "lib",
            TargetKind::Bag => "bag",
            TargetKind::Dep => "dep",
            TargetKind::Device => "device",
        };
        let mut node = Node::new(node_type);
        node.args.add_pos(self.name.clone());
        node.body = self.to_nodebody();
        // node.props.set("name", self.local_name().clone());
        // node.props.set("id", self.local_name().clone());
        // node.props.set("root", self.at.clone());
        // node.props.set("at", self.at.clone());
        // node.props.set("srcs", set_to_vec(self.srcs.clone()));
        // node.props.set("incs", set_to_vec(self.incs.clone()));
        // node.props.set("skips", set_to_vec(self.skips.clone()));
        // for (p, v) in self.props.iter() {
        //     match v {
        //         Value::Str(s) => {
        //             node.props.set(p.clone(), s.clone());
        //         }
        //         Value::Node(n) => {
        //             if n.name == "file" {
        //                 // for file node, add target's root to the file's path
        //                 let path = n.main_arg();
        //                 let path = Path::new(self.at.as_str()).join(path.as_str());
        //                 node.props.set(p.clone(), path.unified());
        //             } else {
        //                 node.props.set(p.clone(), n.to_astr());
        //             }
        //         }
        //         _ => {
        //             node.props.set(p.clone(), v.clone());
        //         }
        //     }
        // }
        // let mut dirs = Array::new();
        // for dir in self.dirs.values() {
        //     // node.add_kid(dir.to_node());
        //     let array_val: Vec<Value> = dir.srcs.iter().map(|s| Value::Str(s.clone())).collect();
        //     let mut obj = Obj::new();
        //     obj.set("name", Value::Str(dir.name.clone()));
        //     obj.set("srcs", array_val);
        //     obj.set("lpath", dir.lpath.clone());
        //     obj.set("rpath", dir.rpath.clone());
        //     dirs.push(obj);
        // }
        // // array of objects
        // node.set_prop("dirs", dirs);
        node
    }

    pub fn get_dir(&self, name: impl Into<AutoStr>) -> Option<&Dir> {
        let name = name.into();
        self.dirs.get(&name)
    }

    pub fn has_auto(&self) -> bool {
        !self.autos.is_empty()
    }

    pub fn transpile_auto(&mut self) -> AutoResult<()> {
        for path in self.autos.iter() {
            info!("Transpiling {}", path.as_str());
            // read the file
            let content = std::fs::read_to_string(path.as_str())?;
            // transpile the content to c code
            let c_code = transpile_c(&self.name, &content)?;
            // write the c code to the file
            let c_path = path.as_str().replace(".at", ".c");
            std::fs::write(Path::new(c_path.as_str()), c_code.source)?;
            // write the header file
            let h_path = path.as_str().replace(".at", ".h");
            std::fs::write(Path::new(h_path.as_str()), c_code.header)?;
            // add the file to the srcs
            self.srcs.insert(AutoStr::from(c_path));
        }
        Ok(())
    }

    pub fn exists(&self) -> bool {
        if self.is_main {
            return true;
        }
        for dir in self.dirs.values() {
            if dir.name != "." && Path::new(dir.to_string().as_str()).is_dir() {
                return true;
            }
        }
        false
    }

    pub fn downloaded(&self) -> bool {
        let loc = self.location();
        return loc.is_dir();
    }

    pub fn merge(&mut self, other: &Target) {
        self.props.merge(&other.props);
        self.autos.extend(other.autos.clone());
        // TODO: merge dirs
        for (k, v) in other.dirs.iter() {
            let mut dir = v.clone();
            // check new root dir
            let path = Path::new(self.at.as_str()).join(dir.at.as_str());
            if path.is_dir() {
                dir.append_root(self.at.clone());
            }
            self.dirs.insert(k.clone(), dir);
        }
        self.srcs.extend(other.srcs.clone());
        self.incs.extend(other.incs.clone());
        self.links.extend(other.links.clone());
        self.skips.extend(other.skips.clone());
    }

    pub fn set_rel(&mut self, rel: AutoStr) {
        self.rel = rel;
    }

    pub fn set_port(&mut self, port: Port) {
        self.port = port;
    }

    pub fn set_defines(&mut self, defines: Vec<AutoStr>) {
        self.defines = defines;
    }

    /// If the target has links, copy link's target info
    pub fn check_links(&mut self) -> AutoResult<Vec<Node>> {
        // 2. If the targets links to a dep target, recursively resolve the linked target
        let links = std::mem::take(&mut self.links);
        Ok(links)
    }

    pub fn apply_link(&mut self, link: &Node) {}

    // fn check_root(&mut self) -> AutoResult<()> {
    //     let root = self.get_root_dir();
    //     info!("{} root: {}", self.name, root.as_str());
    //     self.root = root;
    //     Ok(())
    // }

    fn check_dirs(&mut self) -> AutoResult<()> {
        // 1. check prop `dirs` for existing dirs
        for dir in self.dirs.values() {
            let path = dir.to_string();
            let path = Path::new(path.as_str());
            if !path.is_dir() {
                return Err(format!("checking dir: {} is not a directory", path.display()).into());
            }
        }
        Ok(())
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        if !self.props.has("defines") {
            let defines = Vec::<AutoStr>::new();
            // defines.push("DEBUG".into());
            // defines.push("USE_KERNEL".into());
            self.props.set("defines", defines);
        }

        // 0. check for properties that needs to be scanned
        for (_, v) in self.props.iter() {
            if let Value::Node(n) = v {
                if n.name == "file" {
                    // TODO: for file node, check it's existence
                }
            }
        }
        // info!("scanning sources for {}", self.name);
        // TODO: why do we need to clone dirs?
        // 1. check dirs for existence
        self.check_dirs()?;

        let mut succ_srcs = HashSet::new();
        let mut succ_incs = HashSet::new();

        if self.is_scan {
            let mut target_dir = Dir::from_str(".".into(), self.at.clone(), self.kind.clone());
            target_dir.show_headers = self.show_headers;
            target_dir.scan()?;
            succ_srcs.extend(target_dir.srcs.clone());
            succ_incs.extend(target_dir.incs.clone());
        }

        // 2. check for target level specified srcs/incs
        let srcs = self
            .srcs
            .iter()
            .map(|s| s.clone())
            .collect::<Vec<AutoStr>>();
        let scanned_srcs = scan_specific_srcs(&srcs, &self.at)?;
        succ_srcs.extend(scanned_srcs);

        let incs = self
            .incs
            .iter()
            .map(|s| s.clone())
            .collect::<Vec<AutoStr>>();
        let scanned_incs = check_incs(&incs, &self.at)?;
        succ_incs.extend(scanned_incs);

        // 3. for each dir, scan for source files, include dirs
        let mut dirs = std::mem::take(&mut self.dirs);
        for dir in dirs.values_mut() {
            // info!("scanning dir: {}", dir.to_string());
            dir.show_headers = self.show_headers;
            dir.scan()?;
            // succ_srcs.extend(dir.srcs.clone());
            succ_incs.extend(dir.incs.clone());
        }
        self.dirs = dirs;

        self.srcs = succ_srcs.into_iter().collect();
        self.incs = succ_incs.into_iter().collect();
        // dirs are moved out and scanned, with all infomation merged into the target
        // so we don't need to store dirs anymore
        // self.dirs is empty now

        // 4. scan deps of this target

        for dep in &mut self.deps {
            dep.scan()?;
        }

        Ok(())
    }

    pub fn clean(&self) -> AutoResult<()> {
        if !(self.kind == TargetKind::Dep || self.kind == TargetKind::Device) {
            return Err(format!(
                "Target {} is not a dependency or device, no need to clean",
                self.name
            )
            .into());
        }
        // check if dir is a git directory
        let dir = AutoPath::new(self.at.clone());
        if !dir.is_dir() {
            return Err(format!("Target location {} is not a dir", self.at).into());
        }
        let gitdir = dir.join(".git");
        if !gitdir.is_dir() {
            return Err(format!("Target location {} is not a git dir", self.at).into());
        }
        // check if there is any modifications to this git repository
        let cmd = std::process::Command::new("git")
            .arg("status")
            .arg("-s")
            .current_dir(&dir.path())
            .output()
            .map_err(|e| format!("Failed to run git status: {}", e))?;
        if !cmd.status.success() {
            return Err(format!(
                "Failed to run git status: {}",
                String::from_utf8_lossy(&cmd.stderr)
            )
            .into());
        }
        if !cmd.stdout.is_empty() {
            println!(
                "\n--- git status ---\n\n{}",
                String::from_utf8_lossy(&cmd.stdout)
            );
            return Err(format!("Target location {} has uncommitted changes", self.at).into());
        }
        println!("target {} is ok to clean!", self.name);
        std::fs::remove_dir_all(&dir.path())?;
        Ok(())
    }

    pub fn print_info(&self) -> AutoResult<()> {
        println!("Target: {}", self.name);
        println!("Kind: {}", self.kind);
        println!("Root: {}", self.at);
        println!(
            "Dirs: {}",
            self.dirs
                .values()
                .map(|d| d.name.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );

        Ok(())
    }

    fn group_target(&self, g: &mut Group, t: &Target) {
        // set files
        for src in &t.srcs {
            let fpath = format!("$PROJ_DIR${}\\{}", &self.rel, src);
            g.files.push(crate::group::File { name: fpath.into() })
        }
        // set dirs
        for (_n, d) in &t.dirs {
            let dg = g.mut_kid(&d.name);
            self.group_dir(dg, d);
        }
        if self.name != t.name {
            // set deps
            for dep in &t.deps {
                let dg = g.mut_kid(&dep.name);
                self.group_target(dg, dep);
            }
        }
    }

    fn group_dir(&self, dg: &mut Group, d: &Dir) {
        for s in &d.srcs {
            let fpath = format!("$PROJ_DIR${}\\{}", &self.rel, s);
            dg.files.push(crate::group::File { name: fpath.into() })
        }
        for s in &d.dirs {
            let sg = dg.mut_kid(&s.name);
            self.group_dir(sg, s);
        }
    }

    fn groups(&self) -> Group {
        let mut group = Group::new(self.name.as_str());
        let tg = group.mut_kid_path(&self.location());
        self.group_target(tg, self);
        for t in &self.deps {
            let loc = t.location();
            let g = group.mut_kid_path(&loc);
            self.group_target(g, t);
        }
        group
    }
}

#[derive(Tabled)]
pub struct TargetStatus {
    pub name: AutoStr,
    pub kind: AutoStr,
    pub at: AutoStr,
    pub dirs: AutoStr,
}

impl TargetStatus {
    pub fn from(target: &Target) -> Self {
        Self {
            name: target.name.clone(),
            kind: target.kind.to_string().into(),
            at: target.at.clone(),
            dirs: target
                .dirs
                .values()
                .map(|d| d.name.to_string())
                .collect::<Vec<String>>()
                .join(", ")
                .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pac;
    use auto_lang::config::AutoConfig;

    #[test]
    fn test_dirs() {
        let dir = Dir::from_str("main".into(), ".".into(), TargetKind::App);
        assert_eq!(dir.to_string(), "main");
    }

    #[test]
    fn test_dir_from_node_with_main() {
        let code = r#"
        app("main") {
            dir("src") {
                src("main.at")
            }
            dir("util") {
                src("util.at")
            }
        }
        "#;
        let config = AutoConfig::new(code).unwrap();
        let app_node = config.root.nodes[0].clone();
        let target = Target::from(app_node, "main".into());
        let at = target.at.clone();
        assert_eq!(at, ".");

        let src_dir = target.get_dir("src");
        if let Some(dir) = src_dir {
            assert_eq!(dir.to_string(), "src");
            assert_eq!(dir.at, "src");
            assert_eq!(dir.name, "src");
        } else {
            panic!("src_dir is None");
        }

        let util_dir = target.get_dir("util");
        if let Some(dir) = util_dir {
            assert_eq!(dir.to_string(), "util");
            assert_eq!(dir.at, "util");
            assert_eq!(dir.name, "util");
        } else {
            panic!("util_dir is None");
        }
    }

    #[test]
    fn test_dir_from_node_with_demo() {
        let code = r#"
        name: "math"

        app("demo") {
        }
        "#;
        let config = AutoConfig::new(code).unwrap();
        let pac = Pac::new(config);
        let target = pac.get_target("demo").unwrap();
        let root = target.at.clone();
        assert_eq!(root, "apps/demo");
    }

    #[test]
    fn test_target() {
        let code = r#"
        app("main") {}
        "#;
        let config = AutoConfig::new(code).unwrap();
        let node = config.root.nodes[0].clone();
        let target = Target::from(node, "main".into());
        let root_dir = target.at.clone();
        assert_eq!(root_dir, ".");
        assert_eq!(target.name, "main");
        assert_eq!(target.kind, TargetKind::App);

        // target.scan().unwrap();
        // assert_eq!(target.autos, vec!["main.at"]);
        // assert_eq!(target.srcs, vec!["main.c"]);
        // assert_eq!(target.incs, vec!["main.h"]);
    }

    #[test]
    fn test_current_dir() {
        use auto_lang::util::*;
        let root = "apps/demo";
        let cur = ".";
        let path = Path::new(root).join(cur);
        let path = path.unified();
        assert_eq!(path, "apps/demo");
    }

    #[test]
    fn test_filter_dirs() {
        let selects: HashSet<AutoStr> = vec!["a".into(), "c".into()].to_set();
        let skips: HashSet<AutoStr> = vec![].to_set();
        let name = "a".into();
        let succ = dir_filter(&name, &selects, &skips);
        assert_eq!(succ, true);

        let name = "b".into();
        assert_eq!(dir_filter(&name, &selects, &skips), false);
    }
}
