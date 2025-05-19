use std::process::{Command, Stdio};

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
            // download latest version of auto-up
            let url =
                "https://gitee.com/auto-stack/auto-man/raw/master/crates/auto-up/assets/auto-up.at";

            println!("downloading: {}", url);

            let page = get(url)?.text()?;

            println!("got page: {}", page);
            let config = AutoConfig::new(page)?;
            let url = config.root.props.get_str_of("url");
            let resp = reqwest::blocking::get(url.as_str()).expect("request failed");
            let body = resp.bytes().expect("body invalid");
            let zip_name = url.split('/').last().unwrap();
            let zip_path = AutoPath::from(".").join(zip_name);
            if zip_path.is_file() {
                std::fs::remove_file(zip_path.path())?;
            }
            std::fs::write(zip_path.path(), &body)?;
            println!("download auto-up success");

            // unzip auto-up.zip
            let mut zip = ZipArchive::new(std::fs::File::open(zip_path.path())?)?;
            for i in 0..zip.len() {
                let mut file = zip.by_index(i)?;
                let mut out = std::fs::File::create(file.name())?;
                std::io::copy(&mut file, &mut out)?;
            }
            std::fs::remove_file(zip_path.path())?;

            // invoke auto-up command
            Command::new("./auto-up.exe")
                .stdout(Stdio::inherit())
                .stdin(Stdio::inherit())
                .spawn()
                .unwrap();

            // sleep for 300ms
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::process::exit(0);

            // sid.wait().unwrap();
        }
        None => {
            Cli::command().print_help()?;
        }
    }
    Ok(())
}
