use auto_val::{AutoStr, Node};
use crate::AutoResult;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::fmt;
use tabled::Tabled;
use auto_lang::transpiler::transpile_c;
use std::collections::{HashSet, HashMap};
use auto_val::Obj;
use auto_lang::util::*;
use log::*;

#[derive(Debug, PartialEq, Eq)]
pub enum TargetKind {
    App, // 应用程序
    Lib, // 静态代码库
    Dep, // 依赖库
}

impl TargetKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "app" => TargetKind::App,
            "lib" => TargetKind::Lib,
            "dep" => TargetKind::Dep,
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
        }
    }
}


#[derive(Clone)]
pub struct Dir {
    pub name: AutoStr,
    pub root: AutoStr,
    // TODO: add sub dirs
    pub dirs: Vec<Dir>,
    pub srcs: Vec<AutoStr>,
    pub incs: Vec<AutoStr>,
    pub skips: HashSet<AutoStr>,
}

fn scan_dir_flat(path: &Path, skips: &HashSet<AutoStr>) -> AutoResult<(HashSet<AutoStr>, bool)> {
    if !path.is_dir() {
        return Err(format!("scanning flat dir:{} is not a directory", path.display()).into());
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
        info!("scanned {}", path.display());
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
                _ => {
                    continue;
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
        } else { // try to find the path relative to the root dir
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
        } else { // try to find the path relative to the root dir
            let rel_path = Path::new(root.as_str()).join(inc.as_str());
            if rel_path.is_dir() {
                info!("got inc dir: {}", rel_path.unified());
                succ_incs.insert(rel_path.unified());
            }
        }
    }
    Ok(succ_incs)
}

impl Dir {
    pub fn from_str(s: AutoStr, parent: AutoStr) -> Self {
        let name = s;
        let root = parent;
        Self { name, root, dirs: Vec::new(), srcs: Vec::new(), incs: Vec::new(), skips: HashSet::new() }
    }

    pub fn from_node(node: &Node, parent: AutoStr) -> Self {
        let name = AutoStr::from(node.args.args[0].auto_str());
        info!("making Dir {}::{}", parent, name);
        let root = Path::new(parent.as_str()).join(name.as_str()).unified();
        // TODO read srcs, incs, skips from node
        let dirs = Vec::new();
        let srcs = node.props.get_array_of_str("srcs");
        let incs = node.props.get_array_of_str("incs");
        let skips = node.props.get_array_of_str("skips").into_iter().map(|s| AutoStr::from(s)).collect::<HashSet<AutoStr>>();
        info!("dir contents: {}", name);
        info!("root: {}", root);
        info!("srcs: {:?}", srcs);
        info!("incs: {:?}", incs);
        info!("skips: {:?}", skips);
        Self { name, root, dirs, srcs, incs, skips }
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        let root_path = Path::new(self.root.as_str());
        let (files, has_headers) = scan_dir_flat(root_path, &self.skips)?;
        let mut all_srcs = HashSet::new();
        all_srcs.extend(files);

        // TODO: recursively scan sub dirs

        // scan for additional srcs
        let succ_srcs = scan_specific_srcs(&self.srcs, &self.root)?;
        all_srcs.extend(succ_srcs);

        self.srcs = all_srcs.into_iter().collect();


        // scan for additional incs
        let succ_incs = check_incs(&self.incs, &self.root)?;
        self.incs = succ_incs.into_iter().collect();

        // if current dir has header files, add current dir to incs
        if has_headers {
            self.incs.push(self.root.clone());
        }

        Ok(())
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

pub struct TargetConfig {
    pub name: AutoStr,
    pub kind: TargetKind,
    pub root: AutoStr,
    pub dirs: Vec<Dir>,
    pub srcs: Vec<AutoStr>,
    pub incs: Vec<AutoStr>,
    pub skips: Vec<AutoStr>,
    pub links: Vec<AutoStr>,
    pub pac: AutoStr,
    pub props: Obj,
}

impl TargetConfig {

    pub fn get_root_dir(name: &AutoStr, kind: &TargetKind, pac: &AutoStr) -> AutoStr {
        // 现在只支持最简单的当前目录
        // 如果target的名称和pac名称相同，说明是主target，使用当前目录
        if name == pac {
            AutoStr::from(".")
        } else { // 否则，使用libs/apps/deps中的目录
            let target_dir = match kind {
                TargetKind::App => "apps",
                TargetKind::Lib => "libs",
                TargetKind::Dep => "deps",
            };
            AutoStr::from(format!("{}/{}", target_dir, name))
        }
    }

    pub fn from_node(node: Node, pac: AutoStr) -> Self {
        let mut name = AutoStr::new();
        if !node.args.is_empty() {
            name = node.args.args[0].auto_str()
        }

        let kind = TargetKind::from_str(&node.name);

        // root dir
        let root = Self::get_root_dir(&name, &kind, &pac);
        info!("target config: {} root: {}", name, root);
        // check for dirs
        let mut dirs = node.props.get_array_of_str("dirs");
        dirs.push(".".into());
        let mut dirs: Vec<Dir> = dirs.into_iter().map(|d| { Dir::from_str(d, root.clone()) }).collect();
        // check for dir nodes
        let dir_nodes = node.nodes(&"dir");
        let dirs_from_nodes: Vec<Dir> = dir_nodes.iter().map(|n| { Dir::from_node(n, root.clone()) }).collect();
        dirs.extend(dirs_from_nodes);

        // check for srcs
        let srcs = node.props.get_array_of_str("srcs");

        // check for incs
        let incs = node.props.get_array_of_str("incs");

        // check for links
        let links = node.props.get_array_of_str("links");

        // check for skips
        let skips = node.props.get_array_of_str("skips");

        // check for props (dep target only)
        let props = node.props.clone();

        Self {
            kind,
            pac,
            name,
            root,
            dirs,
            srcs,
            incs,
            links,
            skips,
            props,
        }
    }
}

pub struct Target {
    pub name: AutoStr,
    pub kind: TargetKind,
    pub root: AutoStr,
    pub dirs: HashMap<AutoStr, Dir>,
    // Auto Source files
    pub autos: Vec<AutoStr>,
    // C Source files
    pub srcs: HashSet<AutoStr>,
    // C header files/folders
    pub incs: HashSet<AutoStr>,
    pub skips: Vec<AutoStr>,
    pub links: Vec<AutoStr>,
    pub is_resolved: bool,
    pub is_main: bool,
    pub props: Obj,
}

impl Target {
    pub fn from(config: TargetConfig) -> Self {
        let is_main = config.name == config.pac;
        let mut dirs = HashMap::new();
        for dir in config.dirs.into_iter() {
            dirs.insert(dir.name.clone(), dir);
        }
        let mut target = Self {
            name: config.name,
            kind: config.kind,
            root: config.root,
            dirs,
            autos: Vec::new(),
            srcs: config.srcs.into_iter().collect(),
            incs: config.incs.into_iter().collect(),
            skips: config.skips,
            links: config.links,
            is_resolved: false,
            is_main,
            props: config.props,
        };
        target.check_root().unwrap();
        target
    }

    pub fn get_dir(&self, name: impl Into<AutoStr>) -> Option<&Dir> {
        let name = name.into();
        self.dirs.get(&name)
    }

    pub fn has_auto(&self) -> bool {
        !self.autos.is_empty()
    }

    pub fn transpile(&mut self) -> AutoResult<()> {
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

    pub fn get_root_dir(&self) -> AutoStr {
        // 现在只支持最简单的当前目录
        // 如果target的名称和pac名称相同，说明是主target，使用当前目录
        if self.is_main {
            AutoStr::from(".")
        } else { // 否则，使用libs/apps/deps中的目录
            let target_dir = match self.kind {
                TargetKind::App => "apps",
                TargetKind::Lib => "libs",
                TargetKind::Dep => "deps",
            };
            AutoStr::from(format!("{}/{}", target_dir, self.name))
        }
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
                dir.root = path.unified();
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

    fn check_root(&mut self) -> AutoResult<()> {
        let root = self.get_root_dir();
        info!("{} root: {}", self.name, root.as_str());
        self.root = root;
        Ok(())
    }

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
        info!("scanning sources for {}", self.name);
        // TODO: why do we need to clone dirs?
        // 1. check dirs for existence
        self.check_dirs()?;

        // 2. check for target level additional srcs/incs
        let srcs = self.srcs.iter().map(|s| s.clone()).collect::<Vec<AutoStr>>();
        let succ_srcs = scan_specific_srcs(&srcs, &self.root)?;
        self.srcs = succ_srcs.into_iter().collect();

        let incs = self.incs.iter().map(|s| s.clone()).collect::<Vec<AutoStr>>();
        let succ_incs = check_incs(&incs, &self.root)?;
        self.incs = succ_incs.into_iter().collect();

        // 3. for each dir, scan for source files, include dirs
        for dir in self.dirs.values_mut() {
            info!("scanning dir: {}", dir.to_string());
            dir.scan()?;
            self.srcs.extend(dir.srcs.clone());
            info!("extended srcs: {:?}", self.srcs);
            self.incs.extend(dir.incs.clone());
            info!("extended incs: {:?}", self.incs);
        }

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
            dirs: target.dirs.values().map(|d| d.to_string()).collect::<Vec<String>>().join(", ").into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auto_lang::config::AutoConfig;
    use crate::Pac;

    #[test]
    fn test_dirs() {
        let dir = Dir::from_str("main".into(), ".".into());
        assert_eq!(dir.to_string(), "./main");

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
        let target = Target::from(TargetConfig::from_node(app_node, "main".into()));
        let root = target.root.clone();
        assert_eq!(root, ".");

        let src_dir = target.get_dir("src");
        if let Some(dir) = src_dir {
            assert_eq!(dir.to_string(), "./src");
            assert_eq!(dir.root, root);
            assert_eq!(dir.name, "src");
        } else {
            panic!("src_dir is None");
        }

        let util_dir = target.get_dir("util");
        if let Some(dir) = util_dir {
            assert_eq!(dir.to_string(), "./util");
            assert_eq!(dir.root, root);
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
        let target = Target::from(TargetConfig::from_node(node, "main".into()));
        let root_dir = target.get_root_dir();
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

