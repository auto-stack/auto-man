use crate::asset::Templates;
use crate::Pac;
use crate::{AutoError, AutoResult};
use auto_lang::config::AutoConfig;
use auto_val::{AutoPath, AutoStr, Obj};
use log::*;
use tabled::{
    settings::{object::Rows, style::Style, themes::Colorization, Color},
    Table, Tabled
};
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

    pub fn list_deps() -> AutoResult<()> {
        let store = Self::index_store()?;
        store.list_deps()
    }

    pub fn index_store() -> AutoResult<IndexStore> {
        let dir = search_dirs_or_clone(".am/index", vec![
            "D://".into(),
            home_path(),
            "C://".into(),
            "E://".into(),
        ]);
        if let Some(dir) = dir {
            Ok(IndexStore::new(dir))
        } else {
            Err(format!("No index found").into())
        }
    }

    pub fn list_devices() -> AutoResult<()> {
        let store = Self::index_store()?;
        store.list_devices()
    }
}

fn home_path() -> AutoPath {
    dirs::home_dir().unwrap().to_str().unwrap().into()
}

fn search_dirs_or_clone(dir: &str, locations: Vec<AutoPath>) -> Option<AutoPath> {
    if locations.is_empty() {
        return None;
    }
    let first = locations[0].clone();
    for location in locations {
        let dir = location.join(dir);
        if dir.is_dir() {
            return Some(dir);
        }
    }
    // try to create the directory in the first location
    let dir = first.join(dir);
    // git clone from git@codeup.aliyun.com:soutek/auto-stack/auto-index.git
    let result = std::process::Command::new("git")
        .arg("clone")
        .arg(INDEX_REPO)
        .arg(dir.path())
        .output();
    match result {
        Ok(_) => Some(dir),
        Err(e) => {
            error!("Failed to clone repository {} to {} with error {}", INDEX_REPO, dir, e);
            None
        }
    }
}

pub struct IndexStore {
    path: AutoPath,
}

const INDEX_REPO: &str = "git@codeup.aliyun.com:soutek/auto-stack/auto-index.git";

#[derive(Tabled)]
pub struct PacIndex {
    name: AutoStr,
    url: AutoStr,
    versions: AutoStr,
}

impl IndexStore {
    pub fn new(path: AutoPath) -> Self {
        Self { path }
    }

    pub fn list_deps(&self) -> AutoResult<()> {
        // read index.at within the path's dir
        let index = self.path.join("index.at");
        if !index.is_file() {
            return Err(format!("No index.at file found in {}", self.path).into());
        }
        let index = AutoConfig::read(index.path())?;
        let mut pacs = Vec::new();
        for pac in index.root.nodes("pac") {
            pacs.push(PacIndex {
                name: pac.main_arg().to_astr(),
                url: pac.get_prop_of("repo").to_astr(),
                versions: pac.get_prop_of("versions").to_astr(),
            });
        }
        // print as table
        let mut table = Table::new(pacs);
        table
            .with(Style::modern_rounded())
            .with(Colorization::exact([Color::FG_BRIGHT_BLUE], Rows::first()));
        println!("{}", table);
        Ok(())
    }

    pub fn list_devices(&self) -> AutoResult<()> {
        let index = self.path.join("devices.at");
        if !index.is_file() {
            return Err(format!("No device.at file found in {}", self.path).into());
        }
        let index = AutoConfig::read(index.path())?;
        let mut devices = Vec::new();
        for device in index.root.nodes("device") {
            devices.push(PacIndex {
                name: device.main_arg().to_astr(),
                url: device.get_prop_of("repo").to_astr(),
                versions: device.get_prop_of("versions").to_astr(),
            });
        }
        // print as table
        let mut table = Table::new(devices);
        table
            .with(Style::modern_rounded())
            .with(Colorization::exact([Color::FG_BRIGHT_BLUE], Rows::first()));
        println!("{}", table);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_dirs() {
        let path = search_dirs_or_clone(".am/index", vec![
            "D://".into(),
            home_path(),
            "C://".into(),
            "E://".into(),
        ]);
        assert!(path.is_some());
        assert!(path.unwrap().is_dir());
    }

    #[test]
    fn test_env() {
        use std::env;
        println!("home: {}", env::var("HOME").unwrap());
    }

    #[test]
    fn test_git() {
        use git2::RemoteCallbacks;
        use git2::Cred;
        // Prepare callbacks.
        // let rsa_path = dirs::home_dir().unwrap().join(".ssh/id_rsa");
        let rsa_path = AutoPath::new("D://.am/id_rsa").normalized();
        // format!("{}/.ssh/id_rsa", env::var("HOME").unwrap());
        println!("rsa_path: {}", rsa_path);
        // read the rsa file
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            println!("username_from_url: {}", username_from_url.unwrap());
            Cred::ssh_key(
                username_from_url.unwrap(),
                None,
                rsa_path.path(),
                None,
            )
        });

        // Prepare fetch options.
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(callbacks);

        // Prepare builder.
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fo);

        // Clone the project.
        match builder.clone(INDEX_REPO, Path::new("D://test_git")) {
            Ok(_) => println!("Repository cloned successfully"),
            Err(e) => println!("Failed to clone repository: {}", e),
        }
    }
}