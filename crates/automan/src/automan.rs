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
        // 3. Find build targets with dependency ordering
        let mut pac = Pac::new(config);
        // 4. For each target, scan for source files
        for target in pac.targets.iter_mut() {
            target.scan()?;
        }
        // 5. Invode build script
        Ok(pac)
    }

    pub fn new(path: &str) -> Result<Self, AutoError> {
        Ok(Self { pac: Self::parse_pac(path)?, builder: new_builder(BuilderKind::CMake("CMakeLists.txt".to_string())) })
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


// Implementations
impl Automan {
    fn copy_template(&self, template: &str, path: &str) -> Result<(), AutoError> {
        println!("Copying template from {}", template);
        Templates::copy(template, path)?;
        Ok(())
    }
}
