pub mod fs;
pub mod asset;
pub mod util;
pub mod pull;
mod builder;
mod pac;
mod automan;
mod target;

pub use automan::*;
pub use pac::*;
pub use target::*;
pub use builder::*;

use log::*;
use simplelog::*;
use auto_val::AutoStr;
use clap::{Parser, Subcommand, CommandFactory};
use dialoguer::Select;

// use std::fs::File;

pub type AutoError = Box<dyn std::error::Error>;
pub type AutoResult<T> = Result<T, AutoError>;



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
    Scan {
        port: Option<String>,
    },
    #[command(about = "Build Project", alias = "b")]
    Build {
        dir: Option<String>,
        port: Option<String>,
    },
    #[command(about = "Run Executable", alias = "r")]
    Run,
    #[command(about = "Clean Project")]
    Clean {
        dir: Option<String>,
    },
}

fn init_logger() {
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
            // WriteLogger::new(LevelFilter::Info, Config::default(), File::create("automan.log").unwrap()),
        ]
    ).unwrap();
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
    println!("-----------------------------------");
    println!("Hello, I'm Automan!");
    println!("-----------------------------------");
    println!();
    let cli = Cli::parse();
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
        Some(Commands::Scan { port }) => {
            info!("Scanning dependencies");
            let mut am = Automan::new(".")?;
            let port = select_port(port, &am.list_port_names())?;
            am.set_port(port)?;
            am.scan()?;
        }
        Some(Commands::Build { dir, port }) => {
            let dir = if let Some(dir) = dir { dir } else { ".".to_string() };
            let mut am = Automan::new(&dir)?;
            let port = select_port(port, &am.list_port_names())?;
            am.set_port(port)?;
            am.scan()?;
            am.build()?;
        }
        Some(Commands::Run) => {
            let mut am = Automan::new(".")?;
            // TODO: add build process before running when user specified a '-b' flag
            info!("Running app ...");
            println!();
            println!("------------ output ------------");
            am.run()?;
            println!("------------- end --------------");
        }
        Some(Commands::Clean { dir }) => {
            let dir = if let Some(dir) = dir { dir } else { ".".to_string() };
            let mut am = Automan::new(&dir)?;
            am.clean()?;    
        }
        None => {
            Cli::command().print_help()?;
        }
     }
    Ok(())

}
