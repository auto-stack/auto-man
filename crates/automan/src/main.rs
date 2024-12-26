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
    #[command(about = "New Project")]
    New { name: String },
    #[command(about = "Pull dependencies and resources")]
    Pull,
    #[command(about = "Build Project")]
    Build {
        dir: Option<String>,
    },
    #[command(about = "Run Executable")]
    Run,
    #[command(about = "Clean Project")]
    Clean {
        dir: Option<String>,
    },
}


fn main() -> Result<(), AutoError> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::New { name }) => {
            println!("Creating new project {}", name);
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
            println!("Running at the moment");
        }
        Some(Commands::Clean { dir }) => {
            println!("Cleaning");
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
