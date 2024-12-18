use std::path::Path;
use std::fs::{create_dir_all, File};
use std::io::Write;
use crate::AutoError;
use crate::fs::copy_dir_all;
use rust_embed::RustEmbed;
use crate::asset::Templates;
use crate::util::{split_first, split_last};
use autolang::AutoConfig;
use crate::Pac;

pub struct Builder {
}



// API
impl Builder {
    pub fn new() -> Self {
        Self { }
    }

    pub fn create(&self, name: &str) -> Result<(), AutoError> {
        // 1. Check if project already exists
        let path = Path::new(name);
        if path.is_file() {
            return Err(format!("A file named with {} already exists", name).into());
        }
        if path.is_dir() {
            // TODO: Let the user choose to replace the directory
            return Err(format!("A directory named with {} already exists", name).into());
        }
        // 2. Copy default template to destination
        self.copy_template("default", name)?;
        Ok(())
    }

    pub fn pull(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn build(&self, path: &String) -> Result<(), AutoError> {
        // 1. Find pac.at
        let config = Path::new(path).join("pac.at");
        if !config.is_file() {
            return Err(format!("No pac.at file found in {}", path).into());
        }
        // 2. Parse pac.at
        let config = AutoConfig::from_file(config.as_path())?;
        println!("Config: {}", config.obj);
        // 3. Find build targets with dependency ordering
        let pac = Pac::new(config);
        pac.resolve()?;
        // 4. Generate build script (CMake)
        // 5. Invode build script
        pac.build()?;
        Ok(())
    }

    pub fn run(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn clean(&self) -> Result<(), AutoError> {
        Ok(())
    }
}


// Implementations
impl Builder {
    fn copy_template(&self, template: &str, path: &str) -> Result<(), AutoError> {
        println!("Copying template from {}", template);
        Templates::copy(template, path)?;
        Ok(())
    }
}
