use crate::asset::Templates;
use crate::Pac;
use crate::{AutoError, AutoResult};
use auto_lang::config::AutoConfig;
use auto_val::{AutoStr, Obj};
use glob::glob;
use log::*;
use std::path::Path;
pub struct Automan {
    pac: Pac,
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

    pub(crate) fn update_deps(&mut self) -> AutoResult<()> {
        self.pac.update_deps()
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
        let config = AutoConfig::from_file(config.as_path(), &Obj::new())?;
        // info!("Config: {}", config.root);
        let pac = Pac::new(config);
        // pac.print_targets();

        Ok(pac)
    }

    pub fn new(path: &str) -> Result<Self, AutoError> {
        // Currently, only CMake builder is supported
        Ok(Self {
            pac: Self::parse_pac(path)?,
        })
    }

    pub fn pull(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn set_port(&mut self, port: AutoStr) -> AutoResult<()> {
        self.pac.set_port(port)
    }

    pub fn scan(&mut self) -> Result<(), AutoError> {
        // 3. Resolve targets in the pac, including:
        // 3.1. Resolve dependencies
        // 3.2. Link targets with dependencies
        // 3.3. Scan targets for its local source files
        self.pac.resolve()?;
        self.pac.print_targets();
        self.pac.print_atom();
        Ok(())
    }

    pub fn build(&mut self) -> Result<(), AutoError> {
        // 1. Transpile auto code to c code
        self.transpile_auto()?;
        // 2. Build the project with a specific builder
        self.pac.build()
    }

    pub fn transpile_auto(&mut self) -> Result<(), AutoError> {
        self.pac.transpile_autot()?;
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), AutoError> {
        self.pac.run()
    }

    pub fn clean(&mut self) -> Result<(), AutoError> {
        // deps and devices
        if Path::new("deps").is_dir() {
            info!("deleting directory deps");
            std::fs::remove_dir_all("deps")?;
        }
        if Path::new("devices").is_dir() {
            info!("deleting directory devices");
            std::fs::remove_dir_all("devices")?;
        }

        // builds
        if Path::new("build").is_dir() {
            info!("deleting directory build");
            std::fs::remove_dir_all("build")?;
        }
        if Path::new("Debug").is_dir() {
            info!("deleting directory Debug");
            std::fs::remove_dir_all("Debug")?;
        }
        if Path::new("Release").is_dir() {
            info!("deleting directory Release");
            std::fs::remove_dir_all("Release")?;
        }

        // CMake
        let files = glob::glob("CMakeLists*")?;
        for file in files {
            if let Ok(file) = file {
                info!("deleting file {}", file.display());
                std::fs::remove_file(file)?;
            }
        }

        // IAR: TODO: dir maybe configured in pac.at
        if Path::new("iar").is_dir() {
            info!("deleting directory iar");
            std::fs::remove_dir_all("iar")?;
        }

        // logs
        let files = glob::glob("*.log")?;
        for file in files {
            if let Ok(file) = file {
                info!("deleting file {}", file.display());
                std::fs::remove_file(file)?;
            }
        }

        // clean pac related files
        self.pac.clean()
    }

    pub fn list_port_names(&self) -> Vec<AutoStr> {
        self.pac.list_port_names()
    }
}
