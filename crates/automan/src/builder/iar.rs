use auto_val::{AutoStr, AutoPath};
use crate::AutoResult;
use crate::pac::Pac;
use crate::target::Target;
use super::Builder;
use log::*;
use crate::asset::IARTemplate;
use auto_gen::*;

pub struct IARBuilder {
    pub gen: AutoGen,
}

impl IARBuilder {
    pub fn new(path: AutoPath) -> Self {
        let gen = AutoGen::new().out(path).note('@');
        Self { gen }
    }
}

impl Builder for IARBuilder {
    fn build(&mut self, pac: &Pac) -> AutoResult<()> {
        println!("Building with IAR");
        self.setup(pac)?;
        self.finish(pac)
    }

    fn setup(&mut self, pac: &Pac) -> AutoResult<()> {
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

