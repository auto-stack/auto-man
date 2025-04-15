use crate::asset::Templates;
use crate::AutoResult;
use crate::Pac;
use crate::{Index, IndexStore};
use auto_lang::config::AutoConfig;
use auto_val::shared;
use auto_val::{AutoPath, AutoStr, Obj};
use log::*;
use std::env;
use std::fmt::Display;
use std::path::Path;

pub struct Automan {
    config: AmConfig,
    pac: Pac,
}

// Static API
impl Automan {
    pub fn create_app(name: &str) -> AutoResult<()> {
        Self::create_by_template(name, "app")
    }

    pub fn create_capp(name: &str) -> AutoResult<()> {
        Self::create_by_template(name, "capp")
    }

    pub fn create_lib(name: &str) -> AutoResult<()> {
        Self::create_by_template(name, "lib")
    }

    pub fn create_clib(name: &str) -> AutoResult<()> {
        Self::create_by_template(name, "clib")
    }

    pub fn create_by_template(name: &str, template: &str) -> AutoResult<()> {
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
    pub fn parse_pac(path: &str, am_config: &AmConfig) -> AutoResult<Pac> {
        // 1. Find pac.at
        let config = Path::new(path).join("pac.at");
        if !config.is_file() {
            return Err(format!("No pac.at file found in {}", path).into());
        }
        // 2. Parse pac.at
        let config = AutoConfig::from_file(config.as_path(), &Obj::new())?;
        // info!("Config: {}", config.root);
        let mut pac = Pac::new(config);
        let index_store = Self::index_store(am_config)?;
        println!("loading index store from {}", index_store.path.to_astr());
        pac.index = shared(Index::load(index_store.path.join("index.at").to_astr())?);
        pac.device_index = shared(Index::load(index_store.path.join("devices.at").to_astr())?);
        println!("index: {}", pac.index.borrow());
        // pac.print_targets();

        Ok(pac)
    }

    pub fn new(path: &str, am: AmConfig) -> AutoResult<Self> {
        // Currently, only CMake builder is supported
        let config = am;
        let pac = Self::parse_pac(path, &config)?;
        Ok(Self { config, pac })
    }

    pub fn pull(&self) -> AutoResult<()> {
        Ok(())
    }

    pub fn set_port(&mut self, port: AutoStr) -> AutoResult<()> {
        self.pac.set_port(port)
    }

    pub fn open_ide(&mut self) -> AutoResult<()> {
        println!("builder: {}", self.pac.build_location);
        println!("port: {}", self.pac.port);
        match self.pac.port.as_str() {
            "iar" => {
                // open iar ide in the build_location
                let eww_path = AutoPath::new(self.pac.build_location.as_str())
                    .join(self.pac.name.to_string() + ".eww")
                    .to_astr()
                    .replace("/", "\\");
                println!("eww: {}", eww_path.clone());
                std::process::Command::new("explorer.exe")
                    .arg(eww_path.as_str())
                    .output()?;
            }
            _ => {
                todo!()
            }
        }
        Ok(())
    }

    pub fn scan(&mut self) -> AutoResult<()> {
        // self.pac.print_targets();
        // 3. Resolve targets in the pac, including:
        // 3.1. Resolve dependencies
        // 3.2. Link targets with dependencies
        // 3.3. Scan targets for its local source files
        self.pac.resolve()?;
        self.pac.print_targets();
        self.pac.print_atom();
        Ok(())
    }

    pub fn build(&mut self) -> AutoResult<()> {
        // 1. Transpile auto code to c code
        self.transpile_auto()?;
        // 2. Build the project with a specific builder
        self.pac.build()
    }

    pub fn transpile_auto(&mut self) -> AutoResult<()> {
        self.pac.transpile_autot()?;
        Ok(())
    }

    pub fn run(&mut self) -> AutoResult<()> {
        self.pac.run()
    }

    pub fn clean(&mut self) -> AutoResult<()> {
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

    pub fn list_deps(config: &AmConfig) -> AutoResult<()> {
        let store = Self::index_store(config)?;
        store.list_deps()
    }

    pub fn index_store(config: &AmConfig) -> AutoResult<IndexStore> {
        // try to get index location from amconfig
        let am_path = AutoPath::new(&config.am);
        if !am_path.is_dir() {
            info!("creating am dir: {}", am_path.to_astr());
            std::fs::create_dir_all(am_path.path())?;
        }
        let index_path = am_path.join("index");
        if !index_path.is_dir() {
            // try to clone
            info!("cloneing index dir: {}", index_path.to_astr());
            std::fs::create_dir_all(index_path.path())?;
            let repo = config.index.as_str();
            let result = std::process::Command::new("git")
                .arg("clone")
                .arg(repo)
                .arg(index_path.path())
                .output();
            match result {
                Err(e) => {
                    error!(
                        "Failed to clone repository {} to {} with error {}",
                        repo, index_path, e
                    );
                }
                Ok(_) => {}
            }
        }
        return Ok(IndexStore::new(index_path));

        // let dir = Self::search_dirs_or_clone(
        //     config,
        //     ".am/index",
        //     vec!["D://".into(), home_path(), "C://".into(), "E://".into()],
        // );
        // println!("dir: {:?}", dir);
        // if let Some(dir) = dir {
        //     Ok(IndexStore::new(dir))
        // } else {
        //     Err(format!("No index found").into())
        // }
    }

    pub fn list_devices(config: &AmConfig) -> AutoResult<()> {
        let store = Self::index_store(config)?;
        store.list_devices()
    }

    fn search_dirs_or_clone(
        config: &AmConfig,
        dir: &str,
        locations: Vec<AutoPath>,
    ) -> Option<AutoPath> {
        if locations.is_empty() {
            return None;
        }
        for location in &locations {
            let dir = location.join(dir);
            if dir.is_dir() {
                return Some(dir);
            }
        }
        // find first existing location
        let mut first = None;
        for loc in &locations {
            if loc.is_dir() {
                first = Some(loc.clone());
            }
        }
        if first.is_none() {
            return None;
        }
        // try to create the directory in the first location
        let dir = first.unwrap().join(dir);
        println!("cloning to: {}", dir);
        // git clone from git@codeup.aliyun.com:soutek/auto-stack/auto-index.git
        let repo = config.index.as_str();
        let result = std::process::Command::new("git")
            .arg("clone")
            .arg(repo)
            .arg(dir.path())
            .output();
        match result {
            Ok(_) => Some(dir),
            Err(e) => {
                error!(
                    "Failed to clone repository {} to {} with error {}",
                    repo, dir, e
                );
                None
            }
        }
    }
}

fn home_path() -> AutoPath {
    dirs::home_dir().unwrap().to_str().unwrap().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_dirs() {
        // let path = search_dirs_or_clone(
        //     ".am/index",
        //     vec!["D://".into(), home_path(), "C://".into(), "E://".into()],
        // );
        // assert!(path.is_some());
        // assert!(path.unwrap().is_dir());
    }

    #[test]
    fn test_env() {
        use std::env;
        println!("home: {}", env::var("HOME").unwrap());
    }

    #[test]
    fn test_git() {
        use git2::Cred;
        use git2::RemoteCallbacks;
        // Prepare callbacks.
        // let rsa_path = dirs::home_dir().unwrap().join(".ssh/id_rsa");
        let rsa_path = AutoPath::new("D://.am/id_rsa").normalized();
        // format!("{}/.ssh/id_rsa", env::var("HOME").unwrap());
        println!("rsa_path: {}", rsa_path);
        // read the rsa file
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            println!("username_from_url: {}", username_from_url.unwrap());
            Cred::ssh_key(username_from_url.unwrap(), None, rsa_path.path(), None)
        });

        // Prepare fetch options.
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(callbacks);

        // Prepare builder.
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fo);

        // Clone the project.
        // match builder.clone(INDEX_REPO, Path::new("D://test_git")) {
        // Ok(_) => println!("Repository cloned successfully"),
        // Err(e) => println!("Failed to clone repository: {}", e),
        // }
    }
}

pub struct AmConfig {
    pub index: AutoStr,
    pub am: AutoStr,
    pub is_default: bool,
}

impl Default for AmConfig {
    fn default() -> Self {
        Self {
            index: "git@codeup.aliyun.com:soutek/auto-stack/auto-index.git".into(),
            am: home_path().join(".auto/auto-man").to_astr(),
            is_default: true,
        }
    }
}

impl Display for AmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AmConfig {\n")?;
        f.write_str(format!("  index: {}\n", self.index).as_str())?;
        f.write_str(format!("  am: {}\n", self.am).as_str())?;
        f.write_str("}\n")
    }
}

pub fn load_am_config() -> Option<AmConfig> {
    // try to look for am.at file beside am.exe or in the current directory
    let mut am_path = None;
    match env::current_exe() {
        Ok(path) => {
            let config_path = path.as_path().parent();
            if let Some(config_path) = config_path {
                let config_path = config_path.join("am.at");
                println!("Loading AmConfig from {:?}...", config_path.to_str());
                if config_path.is_file() {
                    am_path = Some(config_path);
                }
            }
        }
        _ => {}
    }
    // check local directory
    if am_path.is_none() {
        let local_config_path = env::current_dir().unwrap().join("am.at");
        if local_config_path.is_file() {
            am_path = Some(local_config_path);
        }
    }
    if let Some(am_path) = am_path {
        let code = std::fs::read_to_string(am_path.clone());
        if let Ok(code) = code {
            let config = AutoConfig::from_code(code, &Obj::EMPTY);
            if let Ok(config) = config {
                let mut am = AmConfig::default();
                if config.root.has_prop("index") {
                    am.index = config.root.get_prop("index").to_astr();
                }
                if config.root.has_prop("am") {
                    am.am = config.root.get_prop_of("am").to_astr();
                }
                am.is_default = false;
                println!("AmConfig: {}", am);
                return Some(am);
            }
        }
    }
    None
}
