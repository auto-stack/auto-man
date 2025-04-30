use crate::AutoResult;
use crate::TargetKind;
use auto_val::Array;
use auto_val::{AutoPath, AutoStr, Node, PathBufExt};
use log::*;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::{collections::HashSet, fmt};

#[derive(Clone)]
pub struct Dir {
    pub name: AutoStr,
    pub root: AutoStr,
    pub lpath: AutoStr,
    pub scan: bool,
    pub recurse: bool,
    pub headers: bool,
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
            lpath: AutoStr::new(),
            scan: true,
            recurse: false,
            headers: false,
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

        // info!("making Dir {}::{} with root: {}", parent, name, root);
        let scan = if node.has_prop("scan") {
            node.props.get_bool_of("scan")
        } else {
            true
        };

        let recurse = if node.has_prop("recurse") {
            node.props.get_bool_of("recurse")
        } else {
            false
        };

        let headers = if node.has_prop("headers") {
            node.props.get_bool_of("headers")
        } else {
            false
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
        // info!("dir contents: {}", name);
        // info!("root: {}", root);
        // info!("srcs: {:?}", srcs);
        // info!("incs: {:?}", incs);
        // info!("skips: {:?}", skips);
        Self {
            name,
            root,
            lpath: AutoStr::new(),
            scan,
            recurse,
            headers,
            target_kind,
            dirs,
            srcs,
            incs,
            skips,
        }
    }

    pub fn set_lpath(&mut self, parent_lpath: AutoStr) {
        self.lpath = format!("{}/{}", parent_lpath, self.name).into();

        for d in &mut self.dirs {
            d.set_lpath(self.lpath.clone());
        }
    }

    pub fn update_root(&mut self, root: AutoStr) {
        // set dir's root
        let old_root = self.root.clone();
        self.root = AutoPath::new(root).join(old_root).to_astr();
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
        node.props.set("lpath", self.lpath.clone());
        let spath = AutoPath::new(self.lpath.clone());
        node.props.set("spath", spath.tail(2).to_astr());
        node.props.set(
            "skips",
            self.skips.clone().into_iter().collect::<Vec<AutoStr>>(),
        );

        // add sub dirs
        if self.dirs.is_empty() {
            node.props.set("dirs", Array::new());
        }
        for dir in &self.dirs {
            let n = dir.to_node();
            node.add_kid(n);
        }

        node
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        if self.recurse {
            self.scan_recursive()
        } else {
            self.scan_dir()
        }
    }

    pub fn scan_recursive(&mut self) -> AutoResult<()> {
        let root = self.root.clone();
        let root_path = Path::new(root.as_str());
        self.scan = true;
        // check current dir

        // for each subdir
        for entry in fs::read_dir(root_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                info!("got subdir: {}", path.display());
                let name: AutoStr = path.file_name().unwrap().to_str().unwrap().into();
                info!("dir name: {}", name.clone());
                let mut sub_dir = Dir::from_str(name, self.root.clone(), self.target_kind.clone());
                sub_dir.headers = self.headers;
                sub_dir.recurse = self.recurse;
                sub_dir.scan = self.scan;
                sub_dir.set_lpath(self.lpath.clone());
                sub_dir.scan_recursive()?;
                self.dirs.push(sub_dir);
            }
        }

        info!("scanning dir: {}", root);
        self.scan_flat()?;
        Ok(())
    }

    pub fn scan_flat(&mut self) -> AutoResult<()> {
        let root_path = Path::new(self.root.as_str());
        let mut all_incs = HashSet::new();
        let mut all_srcs = HashSet::new();

        let (files, has_headers) =
            scan_dir_flat(root_path, &self.target_kind, &self.skips, self.headers)?;
        all_srcs.extend(files);
        if has_headers {
            all_incs.insert(self.root.clone());
        }

        self.srcs.extend(all_srcs);
        self.incs.extend(all_incs);
        Ok(())
    }

    pub fn scan_dir(&mut self) -> AutoResult<()> {
        // info!("- Scanning dir: {}", self.root.as_str());
        let root_path = Path::new(self.root.as_str());
        let mut all_incs = HashSet::new();
        let mut all_srcs = HashSet::new();

        if self.scan {
            let (files, has_headers) =
                scan_dir_flat(root_path, &self.target_kind, &self.skips, self.headers)?;
            all_srcs.extend(files);
            if has_headers {
                all_incs.insert(self.root.clone());
            }

            // check `src` folder in current dir
            let src_path = root_path.join("src");
            let (files, has_headers) =
                scan_dir_flat(&src_path, &self.target_kind, &self.skips, self.headers)?;
            all_srcs.extend(files);
            if has_headers {
                all_incs.insert(src_path.unified());
            }

            // check `include` folder in current dir
            let inc_path = root_path.join("include");
            let (files, has_headers) =
                scan_dir_flat(&inc_path, &self.target_kind, &self.skips, self.headers)?;
            all_incs.extend(files);
            if has_headers {
                all_incs.insert(inc_path.unified());
            }

            // check `inc` folder in current dir
            let inc_path = root_path.join("inc");
            let (files, has_headers) =
                scan_dir_flat(&inc_path, &self.target_kind, &self.skips, self.headers)?;
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
            // info!("scanning sub dir: {}", dir.root);
            dir.scan()?;
            all_incs.extend(dir.incs.clone());
            all_srcs.extend(dir.srcs.clone());
        }

        self.srcs = all_srcs.into_iter().collect();
        self.incs = all_incs.into_iter().collect();
        Ok(())
    }

    pub fn append_root(&mut self, root: AutoStr) {
        self.root = AutoPath::new(root.as_str())
            .join(self.root.as_str())
            .unified();

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
        let path = AutoPath::new(self.root.as_str());
        write!(f, "{}", path.unified())
    }
}

// TODO: scan recursively?
pub fn check_incs(incs: &Vec<AutoStr>, root: &AutoStr) -> AutoResult<HashSet<AutoStr>> {
    let mut succ_incs = HashSet::new();
    for inc in incs.iter() {
        let path = AutoPath::new(inc.as_str());
        // if the absolute path itself is a dir, add it to the succ_srcs
        if path.is_file() {
            // NOTE: incs should be dirs, not files
            error!("inc: {} should be directories, not files", path.unified());
            continue;
        }
        if path.is_dir() {
            succ_incs.insert(path.unified());
        } else {
            // try to find the path relative to the root dir
            let rel_path = AutoPath::new(root.as_str()).join(inc.as_str());
            if rel_path.is_dir() {
                // info!("got inc dir: {}", rel_path.unified());
                succ_incs.insert(rel_path.unified());
            }
        }
    }
    Ok(succ_incs)
}

pub fn scan_specific_srcs(srcs: &Vec<AutoStr>, root: &AutoStr) -> AutoResult<HashSet<AutoStr>> {
    let mut succ_srcs = HashSet::new();
    for src in srcs {
        let path = AutoPath::new(root.as_str()).join(src.as_str());
        // info!("scanning absolute path src: {}", path);
        // if the path itself is a file, add it to the succ_srcs
        if path.is_file() {
            succ_srcs.insert(path.unified());
        } else {
            // try to find the path relative to the root dir
            let rel_path = AutoPath::new(root.as_str()).join(src.as_str());
            // info!("scanning relative path src: {}", rel_path);
            if rel_path.is_file() {
                succ_srcs.insert(rel_path.unified());
            }
        }
    }
    Ok(succ_srcs)
}

pub fn scan_dir_flat(
    path: &Path,
    kind: &TargetKind,
    skips: &HashSet<AutoStr>,
    use_header: bool,
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
        // info!("..   got {}", path.display());
        let file_name = path.file_name();
        match file_name {
            Some(file_name) => {
                let file_name = AutoStr::from(file_name.to_str().unwrap());
                if skips.contains(&file_name) {
                    info!("- skipping {}", file_name.as_str());
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
                    if use_header {
                        files_found.insert(path_str);
                    }
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
