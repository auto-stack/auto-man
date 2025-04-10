use crate::AutoResult;
use auto_lang::trans::c::transpile_c;
use auto_lang::util::*;
use auto_val::Obj;
use auto_val::{AutoPath, AutoStr, Node, Value};
use log::*;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::Path;
use tabled::Tabled;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TargetKind {
    App,    // 应用程序
    Lib,    // 静态代码库
    Dep,    // 依赖库
    Device, // 设备资源
}

impl TargetKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "app" => TargetKind::App,
            "lib" => TargetKind::Lib,
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
        }
    }
}

fn scan_dir_flat(
    path: &Path,
    kind: &TargetKind,
    skips: &HashSet<AutoStr>,
) -> AutoResult<(HashSet<AutoStr>, bool)> {
    if !path.is_dir() {
        return Ok((HashSet::new(), false));
    }

    let mut files_found = HashSet::new();
    let mut headers_found = false;
    let files = std::fs::read_dir(path)?;
    for file in files {
        let file = file?;
        // check for Auto Source files
        let path = file.path();
        if !path.is_file() {
            continue;
        }
        info!("..   got {}", path.display());
        let file_name = path.file_name();
        match file_name {
            Some(file_name) => {
                let file_name = AutoStr::from(file_name.to_str().unwrap());
                if skips.contains(&file_name) {
                    info!("skipping {}", file_name.as_str());
                    continue;
                }
            }
            None => {
                return Err(format!("{} is not a file", path.display()).into());
            }
        }

        let file_name = path.file_name().unwrap();

        let ext = path.extension();
        let path_str = path.unified();
        if let Some(ext) = ext {
            match ext.to_str().unwrap() {
                "at" => {
                    if file_name == "pac.at" {
                        continue;
                    }
                    files_found.insert(path_str);
                }
                "c" => {
                    files_found.insert(path_str);
                }
                "h" => {
                    headers_found = true;
                }
                "s" => {
                    files_found.insert(path_str);
                }
                "S" => {
                    files_found.insert(path_str);
                }
                _ => {
                    if kind == &TargetKind::Device {
                        if file_name != "pac.at" {
                            files_found.insert(path_str);
                        }
                    } else {
                        continue;
                    }
                }
            }
        }
    }
    Ok((files_found, headers_found))
}

fn scan_specific_srcs(srcs: &Vec<AutoStr>, root: &AutoStr) -> AutoResult<HashSet<AutoStr>> {
    let mut succ_srcs = HashSet::new();
    for src in srcs {
        let path = Path::new(root.as_str()).join(src.as_str());
        info!("scanning absolute path src: {}", path.display());
        // if the path itself is a file, add it to the succ_srcs
        if path.is_file() {
            succ_srcs.insert(path.unified());
        } else {
            // try to find the path relative to the root dir
            let rel_path = Path::new(root.as_str()).join(src.as_str());
            info!("scanning relative path src: {}", rel_path.display());
            if rel_path.is_file() {
                succ_srcs.insert(rel_path.unified());
            }
        }
    }
    Ok(succ_srcs)
}

// TODO: scan recursively?
fn check_incs(incs: &Vec<AutoStr>, root: &AutoStr) -> AutoResult<HashSet<AutoStr>> {
    let mut succ_incs = HashSet::new();
    for inc in incs.iter() {
        info!("checking inc: {}/{}", root.as_str(), inc.as_str());
        let path = Path::new(inc.as_str());
        // if the absolute path itself is a dir, add it to the succ_srcs
        if path.is_file() {
            // NOTE: incs should be dirs, not files
            error!("inc: {} should be directories, not files", path.unified());
            continue;
        }
        if path.is_dir() {
            info!("got inc dir: {}", path.unified());
            succ_incs.insert(path.unified());
        } else {
            // try to find the path relative to the root dir
            let rel_path = Path::new(root.as_str()).join(inc.as_str());
            if rel_path.is_dir() {
                info!("got inc dir: {}", rel_path.unified());
                succ_incs.insert(rel_path.unified());
            }
        }
    }
    Ok(succ_incs)
}

#[derive(Clone)]
pub struct Dir {
    pub name: AutoStr,
    pub root: AutoStr,
    pub scan: bool,
    pub target_kind: TargetKind,
    // TODO: add sub dirs
    pub dirs: Vec<Dir>,
    pub srcs: Vec<AutoStr>,
    pub incs: Vec<AutoStr>,
    pub skips: HashSet<AutoStr>,
}

impl Dir {
    pub fn from_str(name: AutoStr, parent: AutoStr, target_kind: TargetKind) -> Self {
        let name = name;
        let root = Path::new(parent.as_str()).join(name.as_str()).unified();
        info!("making Dir {}::{} with root: {}", parent, name, root);
        Self {
            name,
            root,
            scan: true,
            target_kind,
            dirs: Vec::new(),
            srcs: Vec::new(),
            incs: Vec::new(),
            skips: HashSet::new(),
        }
    }

    pub fn from_node(node: &Node, parent: AutoStr, target_kind: TargetKind) -> Self {
        let name = AutoStr::from(node.args.args[0].to_astr());
        let parent_path = Path::new(parent.as_str());
        let root = if node.has_prop("root") {
            parent_path
                .join(node.props.get_str_of("root").as_str())
                .unified()
        } else {
            parent_path.join(name.as_str()).unified()
        };

        info!("making Dir {}::{} with root: {}", parent, name, root);
        let scan = if node.has_prop("scan") {
            node.props.get_bool_of("scan")
        } else {
            true
        };
        // TODO read srcs, incs, skips from node
        let mut dirs = node
            .props
            .get_array_of_str("dirs")
            .into_iter()
            .map(|s| Dir::from_str(s, root.clone(), target_kind.clone()))
            .collect::<Vec<Dir>>();
        // read sub dirs by dir() node
        let dir_nodes = node.nodes(&"dir");
        for dir_node in dir_nodes {
            let dir = Dir::from_node(dir_node, root.clone(), target_kind.clone());
            dirs.push(dir);
        }

        let srcs = node.props.get_array_of_str("srcs");
        let incs = node.props.get_array_of_str("incs");
        let skips = node
            .props
            .get_array_of_str("skips")
            .into_iter()
            .map(|s| AutoStr::from(s))
            .collect::<HashSet<AutoStr>>();
        info!("dir contents: {}", name);
        info!("root: {}", root);
        info!("srcs: {:?}", srcs);
        info!("incs: {:?}", incs);
        info!("skips: {:?}", skips);
        Self {
            name,
            root,
            scan,
            target_kind,
            dirs,
            srcs,
            incs,
            skips,
        }
    }

    pub fn update_root(&mut self, root: AutoStr) {
        println!("Updating root to {}", root);
        // set dir's root
        let old_root = self.root.clone();
        self.root = AutoPath::new(root).join(old_root).to_astr();
        println!("Self.root: {}", self.root);
        // recursively set root of subdirs
        for dir in self.dirs.iter_mut() {
            dir.update_root(self.root.clone());
        }
    }

    pub fn to_node(&self) -> Node {
        let mut node = Node::new("dir");
        node.args.add_pos(self.name.clone());
        node.props.set("root", self.root.clone());
        node.props.set("srcs", self.srcs.clone());
        node.props.set("incs", self.incs.clone());
        node.props.set(
            "skips",
            self.skips.clone().into_iter().collect::<Vec<AutoStr>>(),
        );

        node
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        let root_path = Path::new(self.root.as_str());
        let mut all_incs = HashSet::new();
        let mut all_srcs = HashSet::new();

        if self.scan {
            let (files, has_headers) = scan_dir_flat(root_path, &self.target_kind, &self.skips)?;
            all_srcs.extend(files);
            if has_headers {
                all_incs.insert(self.root.clone());
            }

            // check `src` folder in current dir
            let src_path = root_path.join("src");
            let (files, has_headers) = scan_dir_flat(&src_path, &self.target_kind, &self.skips)?;
            all_srcs.extend(files);
            if has_headers {
                all_incs.insert(src_path.unified());
            }

            // check `include` folder in current dir
            let inc_path = root_path.join("include");
            let (files, has_headers) = scan_dir_flat(&inc_path, &self.target_kind, &self.skips)?;
            all_incs.extend(files);
            if has_headers {
                all_incs.insert(inc_path.unified());
            }

            // check `inc` folder in current dir
            let inc_path = root_path.join("inc");
            let (files, has_headers) = scan_dir_flat(&inc_path, &self.target_kind, &self.skips)?;
            all_incs.extend(files);
            if has_headers {
                all_incs.insert(inc_path.unified());
            }
        }

        // check specified srcs
        let succ_srcs = scan_specific_srcs(&self.srcs, &self.root)?;
        all_srcs.extend(succ_srcs);

        // check specified incs
        let succ_incs = check_incs(&self.incs, &self.root)?;
        all_incs.extend(succ_incs);

        // TODO: recursively scan sub dirs
        for dir in self.dirs.iter_mut() {
            info!("scanning sub dir: {}", dir.root);
            dir.scan()?;
            all_incs.extend(dir.incs.clone());
            all_srcs.extend(dir.srcs.clone());
        }

        self.srcs = all_srcs.into_iter().collect();
        self.incs = all_incs.into_iter().collect();
        Ok(())
    }

    pub fn append_root(&mut self, root: AutoStr) {
        self.root = Path::new(root.as_str()).join(self.root.as_str()).unified();

        for dir in self.dirs.iter_mut() {
            dir.append_root(root.clone());
        }
    }
}

impl PartialEq for Dir {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.root == other.root
    }
}

impl Eq for Dir {}

impl Hash for Dir {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.root.hash(state);
    }
}

impl fmt::Debug for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl fmt::Display for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = Path::new(self.root.as_str());
        write!(f, "{}", path.unified())
    }
}

// pub struct TargetConfig {
//     pub name: AutoStr,
//     pub kind: TargetKind,
//     pub root: AutoStr,
//     pub scan: bool,
//     pub dirs: Vec<Dir>,
//     pub srcs: Vec<AutoStr>,
//     pub incs: Vec<AutoStr>,
//     pub skips: Vec<AutoStr>,
//     pub links: Vec<AutoStr>,
//     pub pac: AutoStr,
//     pub props: Obj,
// }

// impl TargetConfig {

//     pub fn get_root_dir(name: &AutoStr, kind: &TargetKind, pac: &AutoStr) -> AutoStr {
//         // 现在只支持最简单的当前目录
//         // 如果target的名称和pac名称相同，说明是主target，使用当前目录
//         if name == pac {
//             AutoStr::from(".")
//         } else { // 否则，使用libs/apps/deps中的目录
//             let target_dir = match kind {
//                 TargetKind::App => "apps",
//                 TargetKind::Lib => "libs",
//                 TargetKind::Dep => "deps",
//             };
//             AutoStr::from(format!("{}/{}", target_dir, name))
//         }
//     }

//     pub fn from_node(node: Node, pac: AutoStr) -> Self {
//         let mut name = AutoStr::new();
//         if !node.args.is_empty() {
//             name = node.args.args[0].to_astr()
//         }

//         let kind = TargetKind::from_str(&node.name);

//         // scan
//         let scan = if node.has_prop("scan") {
//             node.props.get_bool_of("scan")
//         } else {
//             true
//         };

//         // root dir
//         let root = Self::get_root_dir(&name, &kind, &pac);
//         info!("target config: {} root: {}", name, root);
//         // check for dirs
//         let mut dirs = node.props.get_array_of_str("dirs");
//         dirs.push(".".into());
//         let mut dirs: Vec<Dir> = dirs.into_iter().map(|d| { Dir::from_str(d, root.clone()) }).collect();
//         // check for dir nodes
//         let dir_nodes = node.nodes(&"dir");
//         let dirs_from_nodes: Vec<Dir> = dir_nodes.iter().map(|n| { Dir::from_node(n, root.clone()) }).collect();
//         dirs.extend(dirs_from_nodes);

//         // check for srcs
//         let srcs = node.props.get_array_of_str("srcs");

//         // check for incs
//         let incs = node.props.get_array_of_str("incs");

//         // check for links
//         let links = node.props.get_array_of_str("links");

//         // check for skips
//         let skips = node.props.get_array_of_str("skips");

//         // check for props (dep target only)
//         let props = node.props.clone();

//         Self {
//             kind,
//             pac,
//             name,
//             root,
//             scan,
//             dirs,
//             srcs,
//             incs,
//             links,
//             skips,
//             props,
//         }
//     }
// }

#[derive(Clone)]
pub struct Target {
    pub name: AutoStr,
    pub version: AutoStr,
    pub kind: TargetKind,
    pub root: AutoStr, // root dir of the target, including its name
    pub at: AutoStr,
    pub is_scan: bool,
    pub dirs: HashMap<AutoStr, Dir>,
    // Auto Source files
    pub autos: Vec<AutoStr>,
    // C Source files
    pub srcs: HashSet<AutoStr>,
    // C header files/folders
    pub incs: HashSet<AutoStr>,
    pub skips: HashSet<AutoStr>,
    pub links: Vec<AutoStr>,
    pub is_resolved: bool,
    pub is_main: bool,
    pub props: Obj,
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
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
        };
        AutoStr::from(format!("{}/{}", target_dir, name))
    }
}

impl Target {
    pub fn new(name: impl Into<AutoStr>, kind: TargetKind) -> Self {
        Self {
            name: name.into(),
            version: AutoStr::new(),
            kind,
            root: AutoStr::new(),
            at: AutoStr::default(),
            is_scan: true,
            dirs: HashMap::new(),
            autos: Vec::new(),
            srcs: HashSet::new(),
            incs: HashSet::new(),
            skips: HashSet::new(),
            links: Vec::new(),
            is_resolved: false,
            is_main: false,
            props: Obj::new(),
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

        // root
        let mut root = if node.has_prop("root") {
            node.props.get_str_of("root")
        } else {
            get_root_dir(&name, &kind, &pac)
        };

        // at
        let at = if node.has_prop("at") {
            node.props.get_str_of("at")
        } else {
            AutoStr::default()
        };

        if !at.is_empty() {
            root = AutoPath::new(at.clone()).join(name.clone()).to_astr();
        }

        // dirs
        let dirs = node.props.get_array_of_str("dirs");
        // dirs.push(".".into());
        let mut dirs: Vec<Dir> = dirs
            .into_iter()
            .map(|d| Dir::from_str(d, root.clone(), kind.clone()))
            .collect();
        // check for dir nodes
        let dir_nodes = node.nodes(&"dir");
        let dirs_from_nodes: Vec<Dir> = dir_nodes
            .iter()
            .map(|n| Dir::from_node(n, root.clone(), kind.clone()))
            .collect();
        dirs.extend(dirs_from_nodes);
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
        let links = node.props.get_array_of_str("links");
        // check for skips
        let skips = node.props.get_array_of_str("skips").to_set();
        // check for props (dep target only)
        let props = node.props.clone();

        let is_main = name == pac;

        Self {
            name,
            version,
            kind,
            root,
            at,
            is_scan,
            dirs,
            autos: Vec::new(),
            srcs,
            incs,
            skips,
            links,
            is_resolved: false,
            is_main,
            props,
        }
    }

    fn default_location(&self) -> AutoPath {
        let default_dir = match self.kind {
            TargetKind::App => "apps",
            TargetKind::Lib => "libs",
            TargetKind::Dep => "deps",
            TargetKind::Device => "devices",
        };
        AutoPath::new(default_dir).join(self.name.clone())
    }

    pub fn location(&self) -> AutoPath {
        // if `at` is specified, root will be overridden
        if self.at.is_empty() {
            self.default_location()
        } else {
            AutoPath::new(self.at.clone()).join(&self.name)
        }
    }

    pub fn config_name(&self) -> AutoStr {
        match self.kind {
            TargetKind::Device => "device.at".into(),
            _ => "pac.at".into(),
        }
    }

    pub fn set_at(&mut self, at: impl Into<AutoStr>) {
        let at = at.into();
        self.at = at.clone();
        if !self.at.is_empty() {
            self.root = self.location().to_astr();
            println!("set root due to at: {}", self.root);
            // update root of each sub dirs
            for dir in self.dirs.iter_mut() {
                dir.1.update_root(self.root.clone());
            }
        }
    }

    pub fn to_node(&self) -> Node {
        let node_type = match self.kind {
            TargetKind::App => "app",
            TargetKind::Lib => "lib",
            TargetKind::Dep => "dep",
            TargetKind::Device => "device",
        };
        let mut node = Node::new(node_type);
        node.args.add_pos(self.name.clone());
        node.props.set("name", self.name.clone());
        node.props.set("root", self.root.clone());
        node.props.set("srcs", set_to_vec(self.srcs.clone()));
        node.props.set("incs", set_to_vec(self.incs.clone()));
        node.props.set("skips", set_to_vec(self.skips.clone()));
        for (p, v) in self.props.iter() {
            match v {
                Value::Str(s) => {
                    node.props.set(p.clone(), s.clone());
                }
                Value::Node(n) => {
                    if n.name == "file" {
                        // for file node, add target's root to the file's path
                        let path = n.main_arg();
                        let path = Path::new(self.root.as_str()).join(path.as_str());
                        node.props.set(p.clone(), path.unified());
                    } else {
                        node.props.set(p.clone(), n.to_astr());
                    }
                }
                _ => {
                    node.props.set(p.clone(), v.clone());
                }
            }
        }
        for dir in self.dirs.values() {
            node.add_kid(dir.to_node());
        }
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

    pub fn merge(&mut self, other: &Target) {
        self.props.merge(&other.props);
        self.autos.extend(other.autos.clone());
        // TODO: merge dirs
        for (k, v) in other.dirs.iter() {
            let mut dir = v.clone();
            // check new root dir
            let path = Path::new(self.root.as_str()).join(dir.root.as_str());
            if path.is_dir() {
                let cur_root = dir.root.clone();
                // dir.root = path.unified();
                dir.append_root(self.root.clone());
                let new_root = dir.root.clone();
                info!(
                    "..   updating dir {} root {} -> {}",
                    dir.name, cur_root, new_root
                );
            }
            self.dirs.insert(k.clone(), dir);
        }
        self.srcs.extend(other.srcs.clone());
        self.incs.extend(other.incs.clone());
        self.links.extend(other.links.clone());
        self.skips.extend(other.skips.clone());
    }

    /**
     * If the target has links to dependency targets, check whether the links are valid.
     */
    pub fn check_links(&mut self) -> AutoResult<()> {
        // 2. If the targets links to a dep target, recursively resolve the linked target
        if !self.links.is_empty() {
            for link in self.links.iter() {
                let _ = link.as_str();
                // TODO:
            }
        }
        Ok(())
    }

    // fn check_root(&mut self) -> AutoResult<()> {
    //     let root = self.get_root_dir();
    //     info!("{} root: {}", self.name, root.as_str());
    //     self.root = root;
    //     Ok(())
    // }

    fn check_dirs(&mut self) -> AutoResult<()> {
        println!("checkdirs: path of target root: {}", self.root);
        println!("checkdirs: path of target at: {}", self.at);
        // 1. check prop `dirs` for existing dirs
        for dir in self.dirs.values() {
            let path = dir.to_string();
            println!("checkdirs: path of dir: {}", path);
            let path = Path::new(path.as_str());
            if !path.is_dir() {
                return Err(format!("checking dir: {} is not a directory", path.display()).into());
            }
        }
        Ok(())
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        // 0. check for properties that needs to be scanned
        for (_, v) in self.props.iter() {
            if let Value::Node(n) = v {
                if n.name == "file" {
                    // TODO: for file node, check it's existence
                }
            }
        }
        info!("scanning sources for {}", self.name);
        // TODO: why do we need to clone dirs?
        // 1. check dirs for existence
        self.check_dirs()?;

        let mut succ_srcs = HashSet::new();
        let mut succ_incs = HashSet::new();

        if self.is_scan {
            let mut target_dir = Dir::from_str(".".into(), self.root.clone(), self.kind.clone());
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
        let scanned_srcs = scan_specific_srcs(&srcs, &self.root)?;
        succ_srcs.extend(scanned_srcs);

        let incs = self
            .incs
            .iter()
            .map(|s| s.clone())
            .collect::<Vec<AutoStr>>();
        let scanned_incs = check_incs(&incs, &self.root)?;
        succ_incs.extend(scanned_incs);

        // 3. for each dir, scan for source files, include dirs
        let mut dirs = std::mem::take(&mut self.dirs);
        for dir in dirs.values_mut() {
            info!("scanning dir: {}", dir.to_string());
            dir.scan()?;
            succ_srcs.extend(dir.srcs.clone());
            succ_incs.extend(dir.incs.clone());
        }

        self.srcs = succ_srcs.into_iter().collect();
        self.incs = succ_incs.into_iter().collect();
        // dirs are moved out and scanned, with all infomation merged into the target
        // so we don't need to store dirs anymore
        // self.dirs is empty now

        Ok(())
    }
}

#[derive(Tabled)]
pub struct TargetStatus {
    pub name: AutoStr,
    pub kind: AutoStr,
    pub root: AutoStr,
    pub dirs: AutoStr,
}

impl TargetStatus {
    pub fn from(target: &Target) -> Self {
        Self {
            name: target.name.clone(),
            kind: target.kind.to_string().into(),
            root: target.root.clone(),
            dirs: target
                .dirs
                .values()
                .map(|d| d.to_string())
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
        let config = AutoConfig::from_code(code, &Obj::new()).unwrap();
        let app_node = config.root.nodes[0].clone();
        let target = Target::from(app_node, "main".into());
        let root = target.root.clone();
        assert_eq!(root, ".");

        let src_dir = target.get_dir("src");
        if let Some(dir) = src_dir {
            assert_eq!(dir.to_string(), "src");
            assert_eq!(dir.root, "src");
            assert_eq!(dir.name, "src");
        } else {
            panic!("src_dir is None");
        }

        let util_dir = target.get_dir("util");
        if let Some(dir) = util_dir {
            assert_eq!(dir.to_string(), "util");
            assert_eq!(dir.root, "util");
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
        let config = AutoConfig::from_code(code, &Obj::new()).unwrap();
        let pac = Pac::new(config);
        let target = pac.get_target("demo").unwrap();
        let root = target.root.clone();
        assert_eq!(root, "apps/demo");
    }

    #[test]
    fn test_target() {
        let code = r#"
        app("main") {}
        "#;
        let config = AutoConfig::from_code(code, &Obj::new()).unwrap();
        let node = config.root.nodes[0].clone();
        let target = Target::from(node, "main".into());
        let root_dir = target.root.clone();
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
        println!("{}", path);
        assert_eq!(path, "apps/demo");
    }
}
