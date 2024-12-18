pub mod fs;
pub mod asset;
pub mod util;
pub mod pull;
mod pac;
mod builder;

pub use builder::*;
pub use pac::*;

pub type AutoError = Box<dyn std::error::Error>;

use clap::{Parser, Subcommand, CommandFactory};
use std::error::Error;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, subcommand_required = true)]
struct AutoMan {
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
    Clean,
}


fn main() -> Result<(), Box<dyn Error>> {
    let automan = AutoMan::parse();
    let builder = Builder::new();
    match automan.command {
        Some(Commands::New { name }) => {
            println!("Creating new project {}", name);
            builder.create(&name)?;
        }
        Some(Commands::Pull) => {
            println!("Pulling dependencies and resources");
            builder.pull()?;
        }
        Some(Commands::Build { dir }) => {
            let dir = if let Some(dir) = dir { dir } else { ".".to_string() };
            builder.build(&dir)?;
        }
        Some(Commands::Run) => {
            println!("Running at the moment");
            builder.run()?;
        }
        Some(Commands::Clean) => {
            println!("Cleaning");
            builder.clean()?;
        }
        None => {
            AutoMan::command().print_help()?;
        }
    }
    Ok(())
}
