use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use forge::{Error, domain::ProjectName, storage::Store};

#[derive(Parser)]
#[command(version, about = "Local project management for Saturday Forge")]
struct Cli {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    Add { name: String },
    List,
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Project {
            command: ProjectCommand::Add { name },
        } => {
            let name = ProjectName::new(&name)?;
            let project = Store::open(&cli.db)?.add_project(name)?;
            println!("{}\t{}", project.id, project.name.as_str());
        }
        Command::Project {
            command: ProjectCommand::List,
        } => {
            for project in Store::open(&cli.db)?.list_projects()? {
                println!("{}\t{}", project.id, project.name.as_str());
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("forge: {error}");
            ExitCode::FAILURE
        }
    }
}
