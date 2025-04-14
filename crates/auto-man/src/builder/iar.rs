use super::Builder;
use crate::asset::IARTemplate;
use crate::pac::Pac;
use crate::target::{Target, TargetKind};
use crate::AutoResult;
use auto_gen::*;
use auto_val::AutoPath;
use log::*;

pub struct IARBuilder {
    pub gen: AutoGen,
    pub path: AutoPath,
}

impl IARBuilder {
    pub fn new(path: AutoPath) -> Self {
        let gen = AutoGen::new().out(path.clone()).note('@');
        Self { gen, path }
    }

    fn check_devices(&self, pac: &mut Pac) -> AutoResult<()> {
        let mut has_device = false;
        for target in &pac.targets {
            if target.kind == TargetKind::Device {
                has_device = true;
                break;
            }
        }
        if !has_device {
            // let mut dummy_device = Target::new("dummy", TargetKind::Device);
            // dummy_device.props.set("icf", "dummy");
            // dummy_device.props.set("ddf", "dummy");
            // dummy_device.props.set("board", "dummy");
            // pac.targets.push(dummy_device);
        }
        return Ok(());
    }
}

impl Builder for IARBuilder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()> {
        println!("Building with IAR");
        self.setup(pac)?;
        self.finish(pac)
    }

    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()> {
        // check devices
        self.check_devices(pac)?;
        // check necessary configs
        if !pac.has_device_prop("icf") {
            error!("icf file location is not set");
            return Err("Please set the icf file location in the device file".into());
        }
        if !pac.has_device_prop("ddf") {
            error!("ddf file location is not set");
            return Err("Please set the ddf file location in the device file".into());
        }
        if !pac.has_device_prop("board") {
            error!("board file is not set");
            return Err("Please set the board file in the device file".into());
        }
        // create iar project directory
        let project_dir = self.gen.out.path();
        println!("project_dir: {}", project_dir.display());
        if project_dir.is_file() {
            error!("{} is a file, not a directory", project_dir.display());
        } else if project_dir.is_dir() {
            // remove all files in the directory
            // std::fs::remove_dir_all(project_dir)?;
            std::fs::create_dir_all(project_dir)?;
        } else {
            std::fs::create_dir_all(project_dir)?;
        }
        // set data and molds for auto-gen
        let mut gen = std::mem::take(&mut self.gen);
        gen = gen.data(pac.to_atom());
        let molds = vec![
            Mold::new("iar.eww", IARTemplate::eww()?),
            Mold::new("iar.ewp", IARTemplate::ewp()?),
            Mold::new("iar.ewt", IARTemplate::ewt()?),
            Mold::new("iar.ewd", IARTemplate::ewd()?),
        ];
        gen = gen.molds(molds);
        self.gen = gen;
        Ok(())
    }

    fn finish(&mut self, pac: &Pac) -> AutoResult<()> {
        self.gen.gen();
        Ok(())
    }

    fn target(&mut self, target: &Target) -> AutoResult<()> {
        Ok(())
    }

    fn clean(&mut self) -> AutoResult<()> {
        Ok(())
    }

    fn run(&mut self, pac: &Pac) -> AutoResult<()> {
        Ok(())
    }
}
