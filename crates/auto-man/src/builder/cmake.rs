use super::Builder;
use auto_val::AutoStr;
use log::*;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::AutoResult;
use crate::{Pac, Target, TargetKind};

pub struct CMakeBuilder {
    pub cmake_path: String,
    pub out: Box<dyn Write>,
}

impl CMakeBuilder {
    pub fn new(path: &str) -> Self {
        Self {
            cmake_path: path.to_string(),
            out: Box::new(std::io::sink()),
        }
    }

    fn clear_cmake_file(&mut self) -> AutoResult<()> {
        // check if path exists
        if Path::new(&self.cmake_path).is_file() {
            // remove file
            std::fs::remove_file(&self.cmake_path)?;
        }
        let out = File::create(&self.cmake_path)?;
        self.out = Box::new(out);
        Ok(())
    }
}

fn print_vec(list: &Vec<AutoStr>, out: &mut dyn Write) -> AutoResult<()> {
    // sort the list
    let mut sorted_list = list.clone();
    sorted_list.sort();

    if sorted_list.len() == 1 {
        out.write(format!("{}", sorted_list[0].as_str()).as_bytes())?;
    } else {
        out.write(b"\n")?;
        for item in sorted_list.iter() {
            out.write(format!("    {}\n", item.as_str()).as_bytes())?;
        }
    }
    Ok(())
}

fn print_list(list: &HashSet<AutoStr>, out: &mut dyn Write) -> AutoResult<()> {
    // sort the list
    let mut sorted_list = list.iter().collect::<Vec<_>>();
    sorted_list.sort();

    if sorted_list.len() == 1 {
        out.write(format!("{}", sorted_list[0].as_str()).as_bytes())?;
    } else {
        out.write(b"\n")?;
        for item in sorted_list.iter() {
            out.write(format!("    {}\n", item.as_str()).as_bytes())?;
        }
    }
    Ok(())
}

impl Builder for CMakeBuilder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()> {
        self.setup(pac)?;
        let mut targets_done = HashSet::new();
        for t in &pac.targets {
            if targets_done.contains(&t.rename) {
                continue;
            } else {
                self.target(t, pac)?;
                targets_done.insert(t.rename.clone());
            }
            if !t.deps.is_empty() {
                for dep in t.deps.iter() {
                    if targets_done.contains(&dep.rename) {
                        continue;
                    } else {
                        self.target(dep, pac)?;
                        targets_done.insert(dep.rename.clone());
                    }
                }
            }
        }
        self.finish(pac)?;
        Ok(())
    }

    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()> {
        // load srcs in sub dirs into targets
        pac.collect_srcs()?;

        // setup cmake file header
        self.clear_cmake_file()?;
        self.out.write(b"cmake_minimum_required(VERSION 3.22)\n")?;
        self.out
            .write(format!("project({} LANGUAGES C CXX ASM)\n", pac.name).as_bytes())?;
        self.out.write(b"\n")?;
        Ok(())
    }

    fn finish(&mut self, _pac: &Pac) -> AutoResult<()> {
        let out = self.out.as_mut();
        out.flush()?;
        self.out = Box::new(std::io::sink());

        // run cmake build to generate the win32 project
        let mut cmd = std::process::Command::new("cmake");
        cmd.arg("-A win32").arg("-B build");
        let status = cmd.status()?;
        if !status.success() {
            return Err(format!("Failed to run cmake").into());
        }
        // run cmake build to generate the executable
        let status = std::process::Command::new("cmake")
            .arg("--build")
            .arg("build")
            .status()?;
        if !status.success() {
            return Err(format!("Failed to run cmake").into());
        }
        println!("End of build");
        Ok(())
    }

    fn target(&mut self, target: &Target, _pac: &Pac) -> AutoResult<()> {
        println!("building target {}", target.name);
        let out = self.out.as_mut();

        let cmd = match target.kind {
            TargetKind::App => "add_executable",
            TargetKind::Bag => "add_library",
            TargetKind::Lib => "add_library",
            TargetKind::Dep => "add_library",
            TargetKind::Device => "add_library",
            TargetKind::Test => "add_executable", // TODO: add test
        };

        let is_header_only = target.srcs.is_empty() && !target.incs.is_empty();
        // warn!("Target [{}] srcs: {:?}", target.name, target.srcs);

        out.write(format!("{}({} ", cmd, target.name).as_bytes())?;
        if is_header_only {
            out.write(b"INTERFACE")?;
        } else {
            print_list(&target.srcs, out)?;
        }
        out.write(b")\n\n")?;

        let incs = target
            .incs
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !incs.is_empty() {
            let attr = if is_header_only {
                "INTERFACE"
            } else {
                "PUBLIC"
            };
            out.write(format!("target_include_directories({} {} ", target.name, attr).as_bytes())?;
            print_list(&target.incs, out)?;
            out.write(b")\n")?;
        }

        // defines
        if !target.defines.is_empty() {
            let attr = if is_header_only {
                "INTERFACE"
            } else {
                "PUBLIC"
            };
            out.write(format!("target_compile_definitions({} {} ", target.name, attr).as_bytes())?;
            print_vec(&target.defines, out)?;
            out.write(b")\n")?;
        }

        if target.links.len() > 0 {
            for link in target.links.iter() {
                out.write(
                    format!("target_link_libraries({} {})\n", target.name, link.id()).as_bytes(),
                )?;
            }
        }

        out.write(b"\n")?;

        Ok(())
    }

    fn clean(&mut self) -> AutoResult<()> {
        // remove CMakeLists.txt
        std::fs::remove_file(&self.cmake_path)?;
        // remove build directory
        std::fs::remove_dir_all("build")?;

        // CMake
        let files = glob::glob("CMakeLists*")?;
        for file in files {
            if let Ok(file) = file {
                info!("deleting file {}", file.display());
                std::fs::remove_file(file)?;
            }
        }

        Ok(())
    }

    fn run(&mut self, pac: &Pac, args: Vec<String>) -> AutoResult<()> {
        // run target
        let target = pac.exe_path();
        let status = std::process::Command::new(target).args(args).status()?;
        if !status.success() {
            return Err(format!("Failed to run cmake").into());
        }
        Ok(())
    }
}
