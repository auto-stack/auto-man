use super::Builder;
use auto_gen::AutoGen;
use auto_val::AutoPath;
use dialoguer::Select;
use log::*;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::{AutoResult, TargetKind};
use crate::{Pac, Target};
use encoding_rs::GBK;

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
        let mut targets_done = HashSet::new();
        for t in &pac.targets {
            if !t.deps.is_empty() {
                for dep in t.deps.iter() {
                    if targets_done.contains(&dep.rename) {
                        continue;
                    } else {
                        self.target(dep)?;
                        targets_done.insert(dep.rename.clone());
                    }
                }
            }
            if targets_done.contains(&t.rename) {
                continue;
            } else {
                self.target(t)?;
                targets_done.insert(t.rename.clone());
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
            includes.push_str(&format!(" -I../{}", AutoPath::new(inc)));
        }

        let port = &pac.port;

        let (cc, link, lib) = match port.platform.as_str() {
            "windows" => (
                "cl.exe /c $in /Fo:$out",
                "link.exe /OUT:$out $in",
                "lib.exe /OUT:$out $in",
            ),
            "linux" => (
                "clang -c $in -o $out",
                "clang -o $out $in",
                "ar rcs $out $in",
            ),
            _ => ("clang", "clang", "clang"),
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

rule lib
    command = {}
    description = Building library $out

"#,
                includes, cc, link, lib
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
            "windows" => ".obj",
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

        // deal with the main target of this project
        match target.kind {
            TargetKind::App | TargetKind::Test => {
                let n = if target.name.is_empty() {
                    match target.kind {
                        TargetKind::App => "main".into(),
                        TargetKind::Test => "test".into(),
                        _ => unreachable!(),
                    }
                } else {
                    target.name.clone()
                };

                let target_name = match target.port.platform.as_str() {
                    "windows" => format!("{}.exe", n).into(),
                    "linux" => n.clone(),
                    _ => n.clone(),
                };

                out.write(b"# target executable\n")?;
                out.write(format!("build {}: link {}", target_name, objs.join(" ")).as_bytes())?;

                // add dependency libs
                let deps = target
                    .links
                    .iter()
                    .map(|d| format!("{}.lib", d.main_arg().repr()))
                    .collect::<Vec<String>>()
                    .join(" ");
                if !deps.is_empty() {
                    out.write(b" ")?;
                    out.write(deps.as_bytes())?;
                }
                out.write(b"\n")?;
            }
            TargetKind::Lib => {
                let target_name = match target.port.platform.as_str() {
                    "windows" => format!("{}.lib", target.name).into(),
                    "linux" => format!("lib{}.a", target.name).into(),
                    _ => target.name.clone(),
                };

                out.write(b"# target library\n")?;
                out.write(format!("build {}: lib {}\n", target_name, objs.join(" ")).as_bytes())?;
                out.write(b"\n")?;
            }
            _ => {
                // do nothing
            }
        }
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
            println!("Build failed:");
            println!("=============== Ninja Output: Start ==================");
            if std::env::consts::OS == "windows" {
                let mut decoder = GBK.new_decoder();
                let mut buffer_bytes = [0u8; 2048];
                let mut buffer: &mut str = std::str::from_utf8_mut(&mut buffer_bytes).unwrap();
                _ = decoder.decode_to_str(&out.stdout, &mut buffer, false);
                println!("{}", buffer);
            } else {
                println!("{}", String::from_utf8_lossy(&out.stdout));
            }
            println!("=============== Ninja Output: End ===================");
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

    fn run(&mut self, pac: &Pac, args: Vec<String>) -> AutoResult<()> {
        // look for .exe in build directory
        let mut exes = Vec::new();
        for entry in std::fs::read_dir("build")? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let n = entry.file_name().into_string().unwrap();
                if is_executable::is_executable(&entry.path()) {
                    exes.push(format!("build/{}", n));
                }
            }
        }
        // let user select one to run
        if exes.is_empty() {
            return Err("No executable found".into());
        } else if exes.len() == 1 {
            let target = exes[0].clone();
            println!("[ninja] running exe: {}", target);
            let status = std::process::Command::new(target).args(args).status()?;
            if !status.success() {
                return Err(format!("Failed to run Ninja").into());
            }
        } else {
            // TODO: implement user selection
            let selection = Select::new()
                .with_prompt("Which executable do you want to run?")
                .default(0)
                .items(&exes)
                .interact()?;

            let target = exes[selection].clone();
            println!("[ninja] running exe: {}", target);
            let status = std::process::Command::new(target).args(args).status()?;
            if !status.success() {
                return Err(format!("Failed to run Ninja").into());
            }
        }
        Ok(())
    }
}
