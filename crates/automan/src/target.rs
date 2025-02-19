use autoval::{AutoStr, Node, Value, ValueKey};
use crate::AutoResult;
use std::path::Path;
use std::fmt;
use tabled::Tabled;
use autolang::transpiler::transpile_c;

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

#[derive(Tabled)]
pub struct Target {
    pub name: AutoStr,
    pub kind: TargetKind,

    #[tabled(skip)]
    pub dirs: Vec<AutoStr>,

    // Auto Source files
    #[tabled(skip)]
    pub autos: Vec<AutoStr>,
    // C Source files
    #[tabled(skip)]
    pub srcs: Vec<AutoStr>,
    // C header files
    #[tabled(skip)]
    pub incs: Vec<AutoStr>,

    #[tabled(skip)]
    pub links: Vec<AutoStr>,

    #[tabled(skip)]
    pub is_resolved: bool,

    #[tabled(skip)]
    pub is_main: bool,

}


impl Target {
    pub fn from_node(node: Node, pac_name: AutoStr) -> Self {
        let mut name = AutoStr::new();
        if !node.args.is_empty() {
            name = node.args.args[0].auto_str()
        }

        let kind = TargetKind::from_str(&node.name);

        // check for links
        let links = node.props.get_array_of("links");
        // to auto str
        let links = links.iter().map(|v| AutoStr::from(v.repr())).collect();

        let is_main = name == pac_name;

        Self {
            kind,
            name,
            dirs: Vec::new(),
            autos: Vec::new(),
            srcs: Vec::new(),
            incs: Vec::new(),
            links,
            is_resolved: false,
            is_main,
        }
    }

    pub fn has_auto(&self) -> bool {
        !self.autos.is_empty()
    }

    pub fn transpile(&mut self) -> AutoResult<()> {
        for path in self.autos.iter() {
            println!("Transpiling {}", path.as_str());
            // read the file
            let content = std::fs::read_to_string(path.as_str())?;
            // transpile the content to c code
            let c_content = transpile_c(&content)?;
            // write the c code to the file
            let c_path = path.as_str().replace(".at", ".c");

            std::fs::write(Path::new(c_path.as_str()), c_content)?;
            // add the file to the srcs
            self.srcs.push(AutoStr::from(c_path));
        }
        Ok(())
    }

    pub fn check_dirs(&mut self) -> AutoResult<()> {
        // 现在只支持最简单的当前目录
        let mut dirs = Vec::new();
        // 如果target的名称和pac名称相同，说明是主target，使用当前目录
        if self.is_main {
            dirs.push(AutoStr::from("."));
        } else { // 否则，使用libs/apps/deps中的目录
            let target_dir = match self.kind {
                TargetKind::App => "apps",
                TargetKind::Lib => "libs",
                TargetKind::Dep => "deps",
            };
            dirs.push(AutoStr::from(format!("{}/{}", target_dir, self.name)));
        }
        self.dirs = dirs;
        Ok(())
    }

    pub fn exists(&self) -> bool {
        if self.is_main {
            return true;
        }
        for dir in self.dirs.iter() {
            if dir != "." && Path::new(dir.as_str()).is_dir() {
                return true;
            }
        }
        false
    }

    /**
     * If the target has links to dependency targets, check whether the links are valid.
     */
    pub fn check_links(&mut self) -> AutoResult<()> {
        // 2. If the targets links to a dep target, recursively resolve the linked target
        if !self.links.is_empty() {
            for link in self.links.iter() {
                let link = link.as_str();
                // TODO:
            }
        }
        Ok(())
    }

    pub fn scan_sources(&mut self) -> AutoResult<()> {
        // TODO: why do we need to clone dirs?
        // 1. check dirs
        self.check_dirs()?;

        let dirs = self.dirs.clone();
        println!("Scanning dirs {:?}", dirs);
        let mut succ_dirs = Vec::new();
        for dir in dirs {
            // if dir scan fails, 
            match self.scan_dir(&dir) {
                Ok(_) => {
                    succ_dirs.push(dir);
                }
                Err(e) => {
                    println!("{}", e);
                }
            }
        }
        self.dirs = succ_dirs;
        // TODO: reduce include files into include directories
        let mut incs = Vec::new();
        for inc in self.incs.iter() {
            let dir = Path::new(inc.as_str()).parent().unwrap();
            incs.push(AutoStr::from(dir.to_str().unwrap()));
        }
        self.incs = incs;
        Ok(())
    }

    // Scan the target directory for source/include files
    pub fn scan_dir(&mut self, dir: &str) -> AutoResult<()> {
        let dir = Path::new(dir);
        if !dir.is_dir() {
            return Err(format!("{} is not a directory", dir.display()).into());
        }
        let files = dir.read_dir()?;
        for file in files {
            let file = file?;
            // check for Auto Source files
            let path = file.path();
            if !path.is_file() {
                continue;
            }
            println!("{}", path.display());

            let ext = path.extension();
            let file_name = path.file_stem();
            let path_str = AutoStr::from(path.to_str().unwrap());
            // replace \ with /
            let path_str = path_str.replace("\\", "/");
            if let Some(ext) = ext {
                match ext.to_str().unwrap() {
                    "at" => {
                        if let Some(file_name) = file_name {
                            if let Some(name) = file_name.to_str() {
                                if !name.eq("pac") {
                                    self.autos.push(path_str);
                                }
                            }
                        }
                    }
                    "c" => {
                        self.srcs.push(path_str);
                    }
                    "h" => {
                        self.incs.push(path_str);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autolang::config::AutoConfig;

    #[test]
    fn test_target() {
        let code = r#"
        exe("main") {}
        "#;
        let config = AutoConfig::from_code(code);
        let node = config.unwrap().root.nodes[0].clone();
        let mut target = Target::from_node(node);
        assert_eq!(target.name, "main");
        assert_eq!(target.dirs, vec![".", "main"]);
        assert_eq!(target.kind, TargetKind::App);

        // target.scan().unwrap();
        // assert_eq!(target.autos, vec!["main.at"]);
        // assert_eq!(target.srcs, vec!["main.c"]);
        // assert_eq!(target.incs, vec!["main.h"]);
    }


}

