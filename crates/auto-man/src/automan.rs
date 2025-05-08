use crate::asset::Templates;
use crate::AutoResult;
use crate::Pac;
use crate::{Index, IndexStore};
use auto_lang::config::AutoConfig;
use auto_lang::Universe;
use auto_val::shared;
use auto_val::{AutoPath, AutoStr, Obj, Value};
use log::*;
use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::path::Path;

pub struct Automan {
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
        let config_path = Path::new(path).join("pac.at");
        if !config_path.is_file() {
            return Err(format!("No pac.at file found in {}", path).into());
        }

        // 2. try load saved port
        let mut port = Self::try_load_port()?;

        // let user select a port
        let config = AutoConfig::read(config_path.as_path())?;
        let ports = config.root.get_nodes("port");

        let mut port_names = Vec::new();
        for port in ports {
            port_names.push(port.main_arg().to_astr());
        }

        let win32: AutoStr = "win32".into();
        let port_name = match port {
            None => {
                if !port_names.contains(&win32) {
                    // default port is win32
                    port_names.push(win32);
                }
                Self::select_port(&port_names)?
            }
            Some(port) => {
                if !port_names.contains(&port) {
                    if !port_names.contains(&win32) {
                        // default port is win32
                        port_names.push(win32);
                    }
                    Self::select_port(&port_names)?
                } else {
                    port
                }
            }
        };

        println!("PORT NAME: {}", port_name);

        // use this port to reload config and make a pac
        let mut env = Universe::new();
        env.set_global("port", port_name.clone().into());

        let config = AutoConfig::from_file(config_path.as_path(), &Obj::EMPTY, env)?;
        let mut index_used: Vec<AutoStr> = Vec::new();
        if config.root.has_prop("index") {
            index_used = config
                .root
                .get_prop("index")
                .as_array()
                .iter()
                .map(|t| t.to_astr())
                .collect();
        }
        let mut pac = Pac::new(config);
        let index_store = Self::index_store(am_config, index_used)?;
        info!("loading index store from {}", index_store.path.to_astr());
        pac.index = shared(Index::load(index_store.path.join("index.at").to_astr())?);
        pac.device_index = shared(Index::load(index_store.path.join("devices.at").to_astr())?);
        pac.set_port(port_name.clone())?;
        pac.save_port(port_name.clone())?;
        Ok(pac)
    }

    fn select_port(ports: &Vec<AutoStr>) -> AutoResult<AutoStr> {
        use dialoguer::Select;
        let port = if ports.len() == 1 {
            ports[0].clone()
        } else {
            let selection = Select::new()
                .with_prompt("Which port do you want to build?")
                .default(0)
                .items(&ports)
                .interact()?;

            ports[selection].clone()
        };
        Ok(port)
    }

    pub fn get_state_file() -> AutoResult<AutoPath> {
        // check state file exists? .am/state.at
        let am_dir = AutoPath::new(".am");

        if !am_dir.is_dir() {
            // Create the directory if it doesn't exist
            std::fs::create_dir_all(&am_dir.path())?;
        }

        let state_file = am_dir.join("state.at");
        if !state_file.is_file() {
            // Create the state file if it doesn't exist
            std::fs::File::create(&state_file.path())?;
        }

        Ok(state_file)
    }

    pub fn try_load_port() -> AutoResult<Option<AutoStr>> {
        // load local state from .am/state.at
        let state_file = Self::get_state_file()?;
        let state = AutoConfig::read(&state_file.path())?;

        let port_name = state.root.props.get_str_of("port");
        if !port_name.is_empty() {
            Ok(Some(port_name))
        } else {
            Ok(None)
        }
    }

    pub fn new(path: &str, am: AmConfig) -> AutoResult<Self> {
        // Currently, only CMake builder is supported
        let pac = Self::parse_pac(path, &am)?;
        Ok(Self { pac })
    }

    pub fn pull(&self) -> AutoResult<()> {
        Ok(())
    }

    pub fn set_port(&mut self, port: AutoStr) -> AutoResult<()> {
        self.pac.set_port(port.clone())?;
        self.pac.save_port(port)
    }

    pub fn open_ide(&mut self) -> AutoResult<()> {
        println!("builder: {}", self.pac.build_location);
        println!("port: {}", self.pac.port.name);
        match self.pac.port.builder.as_str() {
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
            "ghs" => {
                let ghs_path = AutoPath::new(self.pac.build_location.as_str())
                    .join("default.gpj")
                    .to_astr()
                    .replace("/", "\\");
                println!("ghs: {}", ghs_path.clone());
                std::process::Command::new("explorer.exe")
                    .arg(ghs_path.as_str())
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
        self.pac.print_atom()
    }

    pub fn info(&self, target: Option<String>) -> AutoResult<()> {
        if let Some(target) = target {
            self.pac.print_target_info(&target)
        } else {
            self.pac.print_targets();
            self.pac.print_atom()
        }
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
        let store = Self::index_store(config, vec![])?;
        store.list_deps()
    }

    pub fn index_store(config: &AmConfig, indexs: Vec<AutoStr>) -> AutoResult<IndexStore> {
        // try to get index location from amconfig
        let am_path = AutoPath::new(&config.am);
        if !am_path.is_dir() {
            info!("creating am dir: {}", am_path.to_astr());
            std::fs::create_dir_all(am_path.path())?;
        }
        // get index name and urls from config and indexs
        let all_index = &config.index;
        println!("all index: {:?}", all_index);
        let mut used_indexs = indexs;
        if used_indexs.is_empty() {
            // fill all index
            for k in all_index.keys() {
                used_indexs.push(k.clone());
            }
        }
        println!("used index: {:?}", used_indexs);

        let mut used_index_path = AutoPath::new(".");

        for index in used_indexs {
            let index_path = am_path.join("index").join(index.clone());
            used_index_path = index_path.clone();
            let repo = all_index.get(&index).unwrap();
            if !index_path.is_dir() {
                // try to clone
                info!("cloning index dir: {}", index_path.to_astr());
                std::fs::create_dir_all(index_path.path())?;

                let result = std::process::Command::new("git")
                    .arg("clone")
                    .arg(repo.as_str())
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
        }
        // else {
        // // try to update
        // info!("updating index dir: {}", index_path.to_astr());
        // let result = std::process::Command::new("git")
        //     .arg("pull")
        //     .arg("--rebase")
        //     .current_dir(index_path.path())
        //     .output();
        // match result {
        //     Err(e) => {
        //         let repo = config.index.as_str();
        //         error!(
        //             "Failed to update repository {} to {} with error {}",
        //             repo, index_path, e
        //         );
        //     }
        //     Ok(_) => {}
        // }
        // }
        return Ok(IndexStore::new(used_index_path));

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
        let store = Self::index_store(config, vec![])?;
        store.list_devices()
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
    pub name: AutoStr,
    pub index: HashMap<AutoStr, AutoStr>,
    pub am: AutoStr,
    pub is_default: bool,
}

impl Default for AmConfig {
    fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(
            "default".into(),
            "git@gitee.com:auto-stack/auto-index.git".into(),
        );
        Self {
            name: "default".into(),
            index: map,
            am: home_path().join(".auto/auto-man").to_astr(),
            is_default: true,
        }
    }
}

impl Display for AmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "AmConfig {{")?;
        writeln!(f, "  name: {}", self.name)?;
        writeln!(f, "  index: {{")?;
        for (k, v) in self.index.iter() {
            writeln!(f, "    {}: {}", k, v)?;
        }
        writeln!(f, "  }}")?;
        writeln!(f, "  am: {}", self.am)?;
        writeln!(f, "}}\n")
    }
}

pub fn load_am_config() -> Option<AmConfig> {
    // try to look for am.at file in the working directory
    let mut am_path = None;

    let local_config_path = env::current_dir().unwrap().join("am.at");
    if local_config_path.is_file() {
        am_path = Some(local_config_path);
    }

    if am_path.is_none() {
        let user_home_config_path = env::home_dir()
            .unwrap()
            .join(".auto")
            .join("auto-man")
            .join("am.at");
        if user_home_config_path.is_file() {
            am_path = Some(user_home_config_path);
        }
    }

    let Some(am_path) = am_path else {
        info!("Automan Config file am.at not found in current or home directory, using the default one");
        return None;
    };
    let code = std::fs::read_to_string(am_path.clone());
    let Ok(code) = code else {
        return None;
    };
    let config = AutoConfig::new(code);
    let Ok(config) = config else {
        return None;
    };
    let mut am = AmConfig::default();
    if config.root.has_prop("index") {
        let index_prop = config.root.get_prop("index");
        if let Value::Obj(obj) = index_prop {
            am.index = obj.to_hashmap();
        }
    }
    if config.root.has_prop("am") {
        am.am = config.root.get_prop_of("am").to_astr();
    }
    am.is_default = false;
    println!("Loading AmConfig from {:?}...", am_path.to_str());
    println!("AmConfig: {}", am);
    Some(am)
}
