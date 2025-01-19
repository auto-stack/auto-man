use autoval::{Node, AutoStr};
use crate::AutoResult;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum TargetKind {
    Obj, // 目标文件
    App, // 应用程序
    Lib, // 静态代码库
    Dylib, // 动态链接库
}

impl TargetKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "obj" => TargetKind::Obj,
            "app" => TargetKind::App,
            "lib" => TargetKind::Lib,
            "dylib" => TargetKind::Dylib,
            _ => panic!("Invalid target kind: {}", s),
        }
    }
}

pub struct Target {
    pub kind: TargetKind,
    pub name: AutoStr,
    pub dirs: Vec<AutoStr>,

    // Auto Source files
    pub autos: Vec<AutoStr>,
    // C Source files
    pub srcs: Vec<AutoStr>,
    // C header files
    pub incs: Vec<AutoStr>,
}


impl Target {
    pub fn from_node(node: Node) -> Self {
        let mut name = AutoStr::new();
        if !node.args.is_empty() {
            name = node.args.args[0].auto_str()
        }

        // TODO: get dir from node props
        let mut dirs = Vec::new();
        if name == "main" {
            dirs.push(AutoStr::from("."));
        } 
        dirs.push(name.clone());

        Self {
            kind: TargetKind::from_str(&node.name),
            name,
            dirs,
            autos: Vec::new(),
            srcs: Vec::new(),
            incs: Vec::new(),
        }
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        // TODO: why do we need to clone dirs?
        let dirs = self.dirs.clone();
        for dir in dirs {
            self.scan_dir(&dir)?;
        }
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
            let path_str = AutoStr::from(path.to_str().unwrap());
            if let Some(ext) = ext {
                match ext.to_str().unwrap() {
                    "at" => {
                        self.autos.push(path_str);
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
        assert_eq!(target.kind, TargetKind::Exe);

        target.scan().unwrap();
        assert_eq!(target.autos, vec!["main.at"]);
        assert_eq!(target.srcs, vec!["main.c"]);
        assert_eq!(target.incs, vec!["main.h"]);

    }
}

