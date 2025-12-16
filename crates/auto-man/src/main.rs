use auto_man::*;
use auto_val::{AutoStr, Instance};
use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use dialoguer::Select;
use log::*;
use simplelog::*;

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
    Run { args: Vec<String> },
    #[command(about = "Clean Project")]
    Clean { dir: Option<String> },
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
    #[command(about = "Pull all dependencies")]
    Pull,
    #[command(about = "Reset automan config and index")]
    Reset,
    #[command(about = "Install am.at config file")]
    Install { file: String },
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
        Some(Commands::Run { args }) => {
            let mut am = Automan::new(".", config)?;
            // TODO: add build process before running when user specified a '-b' flag
            info!("Running app ...");
            println!();
            println!("------------ output ------------");
            am.run(args)?;
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
        Some(Commands::Pull) => {
            let mut am = Automan::new(".", config)?;
            am.pull()?;
        }
        Some(Commands::Reset) => {
            Automan::reset_index()?;
        }
        Some(Commands::Install { file }) => {
            Automan::install_config(&file)?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }
    Ok(())
}
