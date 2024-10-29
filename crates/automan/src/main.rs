pub mod man;
pub mod fs;
pub mod asset;
pub mod util;

use man::Man;

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
    New{name: String},
    #[command(about = "Build Project")]
    Build,
    #[command(about = "Run Executable")]
    Run,
    #[command(about = "Clean Project")]
    Clean,
}


fn main() -> Result<(), Box<dyn Error>> {
    let automan = AutoMan::parse();
    let man = Man::new();
    match automan.command {
        Some(Commands::New { name }) => {
            println!("Creating new project {}", name);
            man.create(&name)?;
        }
        Some(Commands::Build) => {
            println!("Building");
            man.build()?;
        }
        Some(Commands::Run) => {
            println!("Running");
            man.run()?;
        }
        Some(Commands::Clean) => {
            println!("Cleaning");
            man.clean()?;
        }
        None => {
            AutoMan::command().print_help()?;
        }
    }
    Ok(())
}
