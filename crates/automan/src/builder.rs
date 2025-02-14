use crate::{Pac, Target, TargetKind};
use crate::AutoResult;
use std::io::Write;
use std::path::Path;
use std::fs::File;


pub trait Builder {
    fn build(&mut self, pac: &Pac) -> AutoResult<()>;
    fn setup(&mut self, pac: &Pac) -> AutoResult<()>;
    fn finish(&mut self, pac: &Pac) -> AutoResult<()>;
    fn target(&mut self, target: &Target) -> AutoResult<()>;
    fn clean(&mut self) -> AutoResult<()>;
    fn run(&mut self, pac: &Pac) -> AutoResult<()>;
}

pub enum BuilderKind {
    CMake(String), // Cmake with path to CMakeLists.txt
    MSVC, // MSVC
}

pub fn new_builder(kind: BuilderKind) -> Box<dyn Builder> {
    match kind {
        BuilderKind::CMake(path) => Box::new(CMakeBuilder::new(&path)),
        BuilderKind::MSVC => Box::new(MSVCBuilder::new()),
    }
}

pub struct CMakeBuilder {
    pub cmake_path: String,
    pub out: Box<dyn Write>,
}

impl CMakeBuilder {
    pub fn new(path: &str) -> Self {
        // check if path exists
        if Path::new(path).is_file() {
            // remove file
            std::fs::remove_file(path).unwrap();
        }
        let out = File::create(path).unwrap();
        Self {
            cmake_path: path.to_string(),
            out: Box::new(out),
        }
    }
}

impl Builder for CMakeBuilder {
    fn build(&mut self, pac: &Pac) -> AutoResult<()> {
        println!("Building with CMake");
        self.setup(pac)?;
        for t in &pac.targets {
            self.target(t)?;
        }
        self.finish(pac)?;
        Ok(())
    }

    fn setup(&mut self, pac: &Pac) -> AutoResult<()> {
        self.out.write(b"cmake_minimum_required(VERSION 3.22)\n")?;
        self.out.write(format!("project({} LANGUAGES C CXX ASM)\n", pac.name).as_bytes())?;
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

    fn target(&mut self, target: &Target) -> AutoResult<()> {
        println!("building target {}", target.name);
        let out = self.out.as_mut();
        let sources = target.srcs.join(" ");

        match target.kind {
            TargetKind::App => {
                out.write(format!("add_executable({} {})\n", target.name, sources).as_bytes())?;
            }
            TargetKind::Lib => {
                out.write(format!("add_library({} {})\n", target.name, sources).as_bytes())?;
            }
            TargetKind::Dep => {
                // after download, a dep target is the same as a lib target
                out.write(format!("add_library({} {})\n", target.name, sources).as_bytes())?;
            }
        }
        let incs = target.incs.join(" ");
        if !incs.is_empty() {
            out.write(format!("target_include_directories({} PUBLIC {})\n", target.name, incs).as_bytes())?;
        }

        if target.links.len() > 0 {
            for link in target.links.iter() {
                out.write(format!("target_link_libraries({} {})\n", target.name, link).as_bytes())?;
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
        Ok(())
    }

    fn run(&mut self, pac: &Pac) -> AutoResult<()> {
        // run target
        let target = pac.exe_path();
        let status = std::process::Command::new(target)
            .status()?;
        if !status.success() {
            return Err(format!("Failed to run cmake").into());
        }
        Ok(())
    }
}   

pub struct MSVCBuilder {

}

impl MSVCBuilder {
    pub fn new() -> Self {
        Self {
        }
    }
}

impl Builder for MSVCBuilder {
    fn build(&mut self, pac: &Pac) -> AutoResult<()> {
        println!("Building with MSVC");
        Ok(())
    }

    fn setup(&mut self, pac: &Pac) -> AutoResult<()> {
        println!("project({})", pac.name);
        Ok(())
    }

    fn finish(&mut self, pac: &Pac) -> AutoResult<()> {
        println!("target_link_libraries(main PRIVATE {})", pac.targets.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(" "));
        Ok(())
    }   

    fn target(&mut self, target: &Target) -> AutoResult<()> {
        println!("add_executable({} {})", target.name, target.name);
        Ok(())
    }

    fn clean(&mut self) -> AutoResult<()> {
        Ok(())
    }

    fn run(&mut self, _pac: &Pac) -> AutoResult<()> {
        Ok(())
    }
}