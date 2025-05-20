use std::{
    process::{Command, Stdio},
    thread::sleep,
};

use auto_lang::config::AutoConfig;
use auto_man::*;
use auto_val::{AutoPath, AutoStr};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use dialoguer::Select;
use log::*;
use reqwest::blocking::get;
use simplelog::*;
use zip::ZipArchive;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, subcommand_required = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "New App Package")]
    App { name: String },
    #[command(about = "New Library Package")]
    Lib { name: String },
    #[command(about = "New C App Package")]
    Capp { name: String },
    #[command(about = "New C Library Package")]
    Clib { name: String },
    #[command(about = "Scan Dependencies")]
    Scan,
    #[command(about = "Build Project", alias = "b")]
    Build { dir: Option<String> },
    #[command(about = "Run Executable", alias = "r")]
    Run,
    #[command(about = "Clean Project")]
    Clean { dir: Option<String> },
    #[command(about = "Update Dependencies")]
    Update,
    #[command(about = "Show Dependencies")]
    Deps,
    #[command(about = "Show Devices")]
    Devices,
    #[command(about = "Open Related IDE", alias = "o")]
    Open,
    #[command(about = "Show Package or Target Info", alias = "i")]
    Info { target: Option<String> },
    #[command(about = "Show and select port")]
    Port,
    #[command(about = "Upgrade auto-man itself")]
    Upgrade,
    #[command(about = "Install master version locally")]
    Install,
}

fn init_logger() {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        // WriteLogger::new(LevelFilter::Info, Config::default(), File::create("automan.log").unwrap()),
    ])
    .unwrap();
}

fn select_port(input: Option<String>, ports: &Vec<AutoStr>) -> AutoResult<AutoStr> {
    let port = if let Some(input) = input {
        input.into()
    } else {
        if ports.len() == 1 {
            ports[0].clone()
        } else {
            let selection = Select::new()
                .with_prompt("Which port do you want to build?")
                .default(0)
                .items(&ports)
                .interact()?;

            ports[selection].clone()
        }
    };
    Ok(port)
}

fn do_upgrade() -> AutoResult<()> {
    info!("Upgrading Automan");
    // Implement upgrade logic here
    use crate::up::*;
    upgrade()?;
    Ok(())
}

fn main() -> Result<(), AutoError> {
    init_logger();
    let logo = format!(
        r#"---------------------------
Hello, I'm Automan {}!
---------------------------"#,
        AUTOMAN_VERSION,
    );
    println!("{}", logo.bright_yellow().bold());
    let cli = Cli::parse();

    let config = match load_am_config() {
        Some(config) => config,
        _ => AmConfig::default(),
    };
    // try to load base config
    match cli.command {
        Some(Commands::App { name }) => {
            info!("Creating new app package {}", name);
            Automan::create_app(&name)?;
        }
        Some(Commands::Lib { name }) => {
            info!("Creating new library package {}", name);
            Automan::create_lib(&name)?;
        }
        Some(Commands::Capp { name }) => {
            info!("Creating new C app package {}", name);
            Automan::create_capp(&name)?;
        }
        Some(Commands::Clib { name }) => {
            info!("Creating new C library package {}", name);
            Automan::create_clib(&name)?;
        }
        Some(Commands::Scan) => {
            info!("Scanning dependencies");
            let mut am = Automan::new(".", config)?;
            am.scan()?;
        }
        Some(Commands::Build { dir }) => {
            let dir = if let Some(dir) = dir {
                dir
            } else {
                ".".to_string()
            };
            let mut am = Automan::new(&dir, config)?;
            am.scan()?;
            am.build()?;
        }
        Some(Commands::Run) => {
            let mut am = Automan::new(".", config)?;
            // TODO: add build process before running when user specified a '-b' flag
            info!("Running app ...");
            println!();
            println!("------------ output ------------");
            am.run()?;
            println!("------------- end --------------");
        }
        Some(Commands::Clean { dir }) => {
            let dir = if let Some(dir) = dir {
                dir
            } else {
                ".".to_string()
            };
            let mut am = Automan::new(&dir, config)?;
            am.clean()?;
        }
        Some(Commands::Update) => {
            let mut am = Automan::new(".", config)?;
            am.update_deps()?;
        }
        Some(Commands::Deps) => {
            Automan::list_deps(&config)?;
        }
        Some(Commands::Devices) => {
            Automan::list_devices(&config)?;
        }
        Some(Commands::Open) => {
            // std::process::Command::new("explorer.exe")
            // .arg("iar\\hello.eww")
            // .spawn()?;
            let mut am = Automan::new(".", config)?;
            am.open_ide()?;
        }
        Some(Commands::Info { target }) => {
            let am = Automan::new(".", config)?;
            am.info(target)?;
        }
        Some(Commands::Port) => {
            let mut am = Automan::new(".", config)?;
            let port = select_port(None, &am.list_port_names())?;
            am.set_port(port.clone())?;
            info!("port \"{}\" written to .am/state.at", port)
        }
        Some(Commands::Upgrade) => {
            do_upgrade()?;
        }
        Some(Commands::Install) => {
            // Copy auto-man.exe to $HOME/.auto/auto-man/auto-man_master.exe
            let install_path = dirs::home_dir().unwrap().join(".auto/auto-man");
            let file = "auto-man.master.exe";
            let dest = install_path.join(file);
            let exe = "target/debug/auto-man.exe";
            if dest.exists() {
                info!("dest file {} exists, deleting...", dest.display());
                std::fs::remove_file(&dest)?;
            }
            // std::thread::sleep(std::time::Duration::from_millis(1000));
            info!("copy {} to {}", exe, dest.display());
            std::fs::copy(exe, &dest)?;

            // Link am.exe and auto-man.exe
            info!("linking {} to {}", exe, dest.display());
            let link = install_path.join("am.exe");
            if link.exists() {
                std::fs::remove_file(&link)?;
            }
            std::fs::hard_link(&dest, link)?;

            let link = install_path.join("auto-man.exe");
            if link.exists() {
                std::fs::remove_file(&link)?;
            }
            std::fs::hard_link(&dest, link)?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }
    Ok(())
}
