use std::path::Path;
use crate::AutoError;
use crate::asset::Templates;
use autolang::config::AutoConfig;
use crate::Pac;
use crate::builder::Builder;
use crate::builder::BuilderKind;
use crate::builder::new_builder;
pub struct Automan {
    pac: Pac,
    builder: Box<dyn Builder>,
}

// Static API
impl Automan {
    pub fn create_app(name: &str) -> Result<(), AutoError> {
        Self::create_by_template(name, "app")
    }

    pub fn create_capp(name: &str) -> Result<(), AutoError> {
        Self::create_by_template(name, "capp")
    }

    pub fn create_lib(name: &str) -> Result<(), AutoError> {
        Self::create_by_template(name, "lib")
    }

    pub fn create_clib(name: &str) -> Result<(), AutoError> {
        Self::create_by_template(name, "clib")
    }

    pub fn create_by_template(name: &str, template: &str) -> Result<(), AutoError> {
        // 1. Check if project already exists
        let path = Path::new(name);
        if path.is_file() {
            return Err(format!("A file named with {} already exists", name).into());
        }
        if path.is_dir() {
            // TODO: Let the user choose to replace the directory
            return Err(format!("A directory named with {} already exists", name).into());
        }
        // 2. Copy template to destination
        Templates::copy(template, name)?;
        Ok(())
    }
}

// API
impl Automan {
    pub fn parse_pac(path: &str) -> Result<Pac, AutoError> {
        // 1. Find pac.at
        let config = Path::new(path).join("pac.at");
        if !config.is_file() {
            return Err(format!("No pac.at file found in {}", path).into());
        }
        // 2. Parse pac.at
        let config = AutoConfig::from_file(config.as_path())?;
        println!("Config: {}", config.root);
        let mut pac = Pac::new(config);
        pac.print_targets();
        // 3. Resolve targets in the pac, including:
        // 3.1. Resolve dependencies
        // 3.2. Link targets with dependencies
        // 3.3. Scan targets for its local source files
        pac.resolve()?;
        Ok(pac)
    }

    pub fn new(path: &str) -> Result<Self, AutoError> {
        Ok(Self { pac: Self::parse_pac(path)?, builder: new_builder(BuilderKind::CMake("CMakeLists.txt".to_string())) })
    }

    pub fn pull(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn build(&mut self) -> Result<(), AutoError> {
        self.builder.build(&self.pac)?;
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), AutoError> {
        self.builder.run(&self.pac)?;
        Ok(())
    }

    pub fn clean(&mut self) -> Result<(), AutoError> {
        self.builder.clean()?;
        Ok(())
    }
}
