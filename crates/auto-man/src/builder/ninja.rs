use super::Builder;
use auto_gen::AutoGen;
use auto_val::AutoPath;
use log::*;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::AutoResult;
use crate::{Pac, Target};

pub struct NinjaBuilder {
    pub gen: AutoGen,
    pub path: AutoPath,
    pub out: Box<dyn Write>,
}

impl NinjaBuilder {
    pub fn new(path: AutoPath) -> Self {
        let gen = Self::new_gen(&path);
        Self {
            gen,
            path,
            out: Box::new(std::io::sink()),
        }
    }

    fn new_gen(path: &AutoPath) -> AutoGen {
        AutoGen::new().out(path.clone()).note('$').rename(true)
    }

    fn reset_ninja_file(&mut self) -> AutoResult<()> {
        // check if path exists
        if Path::new(self.path.path()).is_file() {
            // remove file
            std::fs::remove_file(self.path.path())?;
        }
        let parent = self.path.parent();
        if !parent.is_dir() {
            std::fs::create_dir_all(parent.path())?;
        }
        let out = File::create(self.path.path())?;
        self.out = Box::new(out);
        Ok(())
    }
}

impl Builder for NinjaBuilder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()> {
        self.setup(pac)?;
        for t in &pac.targets {
            self.target(t)?;
            if !t.deps.is_empty() {
                for dep in t.deps.iter() {
                    self.target(dep)?;
                }
            }
        }
        self.finish(pac)?;
        Ok(())
    }

    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()> {
        // load srcs in sub dirs into targets
        pac.collect_srcs()?;

        // setup Ninja file header
        self.reset_ninja_file()?;

        println!("Got includes: {:?}", pac.all_incs());

        let mut includes = String::from("includes =");
        for inc in pac.all_incs() {
            includes.push_str(&format!(" -I{}", inc));
        }

        let port = &pac.port;

        let (cc, link) = match port.platform.as_str() {
            "win32" => ("cl.exe /c $in /Fo:$out", "link.exe /OUT:$out $in"),
            "linux" => ("clang -c $in -o $out", "clang -o $out $in"),
            _ => ("clang", "clang"),
        };

        // write header rules
        self.out.write(
            format!(
                r#"# ninja build file

# includes
{}

# rules
rule cc
    command = {} $includes
    description = Compiling $in

rule link
    command = {}
    description = Linking $out

"#,
                includes, cc, link
            )
            .as_bytes(),
        )?;

        Ok(())
    }

    fn target(&mut self, target: &Target) -> AutoResult<()> {
        println!("[ninja] building target {}", target.name);
        let out = self.out.as_mut();

        out.write(format!("# build objects for target {}\n", target.name).as_bytes())?;

        let mut objs = Vec::new();

        let obj_postfix = match target.port.platform.as_str() {
            "win32" => ".obj",
            "linux" => ".o",
            _ => ".o",
        };

        for src in target.srcs.iter() {
            if src.ends_with(".c") {
                let ofile = AutoPath::new(src).filename().replace(".c", obj_postfix);
                let path = if src.starts_with("/") {
                    src
                } else {
                    &format!("{}", src).into()
                };
                out.write(format!("build {}: cc ../{}\n", ofile, path).as_bytes())?;
                objs.push(ofile);
            }
        }

        let target_name = match target.port.platform.as_str() {
            "win32" => format!("{}.exe", target.name).into(),
            "linux" => target.name.clone(),
            _ => target.name.clone(),
        };

        out.write(b"# target executable\n")?;
        out.write(format!("build {}: link {}\n", target_name, objs.join(" ")).as_bytes())?;
        out.write(b"\n")?;

        // out.write(format!("build {}.o: cc ../{}.c\n", target.name, target.name).as_bytes())?;

        let is_header_only = target.srcs.is_empty() && !target.incs.is_empty();
        // warn!("Target [{}] srcs: {:?}", target.name, target.srcs);

        // out.write(format!("{}({} ", cmd, target.name).as_bytes())?;
        if is_header_only {
            // out.write(b"INTERFACE")?;
        } else {
            // print_list(&target.srcs, out)?;
        }
        // out.write(b")\n\n")?;

        let incs = target
            .incs
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if !incs.is_empty() {
            // out.write(format!("target_include_directories({} {} ", target.name, attr).as_bytes())?;
            // print_list(&target.incs, out)?;
            // out.write(b")\n")?;
        }

        // defines
        if !target.defines.is_empty() {
            // out.write(format!("target_compile_definitions({} {} ", target.name, attr).as_bytes())?;
            // print_vec(&target.defines, out)?;
            // out.write(b")\n")?;
        }

        if target.links.len() > 0 {}

        // out.write(b"\n")?;

        Ok(())
    }

    fn finish(&mut self, _pac: &Pac) -> AutoResult<()> {
        let out = self.out.as_mut();
        out.write(b"\n")?;
        out.flush()?;
        // let out be finished and close the file
        self.out = Box::new(std::io::sink());
        // self.out = Box::new(std::io::sink());
        //
        let build_path = self.path.parent();

        println!(
            "[ninja] calling ninja to build executable in {}",
            build_path
        );

        // run ninja to build
        let out = std::process::Command::new("ninja")
            .args(["-C", build_path.to_astr().as_str()])
            .output()?;
        // cmd.current_dir(build_path.path());
        if out.status.success() {
            println!("Build successful");
        } else {
            println!("Build failed");
            println!("{}", String::from_utf8(out.stderr).unwrap());
        }
        println!("End of build");
        Ok(())
    }

    fn clean(&mut self) -> AutoResult<()> {
        // remove NinjaLists.txt
        std::fs::remove_file(&self.path.path())?;
        // remove build directory
        std::fs::remove_dir_all("build")?;

        // Ninja
        let files = glob::glob("NinjaLists*")?;
        for file in files {
            if let Ok(file) = file {
                info!("deleting file {}", file.display());
                std::fs::remove_file(file)?;
            }
        }

        Ok(())
    }

    fn run(&mut self, pac: &Pac) -> AutoResult<()> {
        // run target
        let target = self.path.parent().join(&pac.name);
        println!("[ninja] running exe: {}", target);
        let status = std::process::Command::new(target.to_string()).status()?;
        if !status.success() {
            return Err(format!("Failed to run Ninja").into());
        }
        Ok(())
    }
}
