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

pub type AutoError = Box<dyn std::error::Error>;
pub type AutoResult<T> = Result<T, AutoError>;

use clap::{Parser, Subcommand, CommandFactory};

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
    #[command(about = "Pull dependencies and resources")]
    Pull,
    #[command(about = "Build Project", alias = "b")]
    Build {
        dir: Option<String>,
    },
    #[command(about = "Run Executable", alias = "r")]
    Run,
    #[command(about = "Clean Project")]
    Clean {
        dir: Option<String>,
    },
}


fn main() -> Result<(), AutoError> {
    println!("Hello, world!");
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::App { name }) => {
            println!("Creating new app package {}", name);
            Automan::create_app(&name)?;
        }
        Some(Commands::Lib { name }) => {
            println!("Creating new library package {}", name);
            Automan::create_lib(&name)?;
        }
        Some(Commands::Capp { name }) => {
            println!("Creating new C app package {}", name);
            Automan::create_capp(&name)?;
        }
        Some(Commands::Clib { name }) => {
            println!("Creating new C library package {}", name);
            Automan::create_clib(&name)?;
        }
        Some(Commands::Pull) => {
            println!("Pulling dependencies and resources");
        }
        Some(Commands::Build { dir }) => {
            let dir = if let Some(dir) = dir { dir } else { ".".to_string() };
            let mut am = Automan::new(&dir)?;
            am.build()?;
        }
        Some(Commands::Run) => {
            let mut am = Automan::new(".")?;
            am.run()?;
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
