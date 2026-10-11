use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use forge::{
    Error,
    domain::{Criterion, ProjectName, TaskTitle},
    storage::Store,
};

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
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
}

#[derive(Subcommand)]
enum ProjectCommand {
    Add { name: String },
    List,
}

#[derive(Subcommand)]
enum TaskCommand {
    Add {
        #[arg(long, value_name = "ID", allow_hyphen_values(true))]
        project: i64,
        title: String,
        #[arg(long = "criterion")]
        criterion: Vec<String>,
    },
    List {
        #[arg(long, value_name = "ID", allow_hyphen_values(true))]
        project: i64,
    },
    Show {
        #[arg(allow_hyphen_values(true))]
        id: i64,
    },
    Dependency {
        #[command(subcommand)]
        command: DependencyCommand,
    },
}

#[derive(Subcommand)]
enum DependencyCommand {
    Add {
        #[arg(allow_hyphen_values(true))]
        task_id: i64,
        #[arg(allow_hyphen_values(true))]
        prerequisite_id: i64,
    },
    List {
        #[arg(allow_hyphen_values(true))]
        task_id: i64,
    },
    Remove {
        #[arg(allow_hyphen_values(true))]
        task_id: i64,
        #[arg(allow_hyphen_values(true))]
        prerequisite_id: i64,
    },
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
        Command::Task {
            command:
                TaskCommand::Add {
                    project,
                    title,
                    criterion,
                },
        } => {
            let title = TaskTitle::new(&title)?;
            if criterion.is_empty() {
                return Err(Error::MissingCriteria);
            }
            let criteria = criterion
                .iter()
                .map(|value| Criterion::new(value))
                .collect::<Result<Vec<_>, _>>()?;
            let task = Store::open(&cli.db)?.add_task(project, title, criteria)?;
            println!("{}\t{}", task.id, task.title.as_str());
        }
        Command::Task {
            command: TaskCommand::List { project },
        } => {
            for task in Store::open(&cli.db)?.list_tasks(project)? {
                println!(
                    "{}\t{}\t{}",
                    task.id,
                    task.title.as_str(),
                    task.status.as_str()
                );
            }
        }
        Command::Task {
            command: TaskCommand::Show { id },
        } => {
            let store = Store::open(&cli.db)?;
            let task = store.get_task(id)?;
            println!("Task {}", task.id);
            println!("Project: {}", task.project_id);
            println!("Title: {}", task.title.as_str());
            println!("Status: {}", task.status.as_str());
            println!("Criteria:");
            for (index, criterion) in task.criteria.iter().enumerate() {
                println!("{}. {}", index + 1, criterion.as_str());
            }
            println!("Dependencies:");
            let dependencies = store.list_dependencies(id)?;
            if dependencies.is_empty() {
                println!("(none)");
            } else {
                for dependency in dependencies {
                    println!("{dependency}");
                }
            }
        }
        Command::Task {
            command: TaskCommand::Dependency { command },
        } => match command {
            DependencyCommand::Add {
                task_id,
                prerequisite_id,
            } => {
                Store::open(&cli.db)?.add_dependency(task_id, prerequisite_id)?;
            }
            DependencyCommand::List { task_id } => {
                for dependency in Store::open(&cli.db)?.list_dependencies(task_id)? {
                    println!("{dependency}");
                }
            }
            DependencyCommand::Remove {
                task_id,
                prerequisite_id,
            } => {
                Store::open(&cli.db)?.remove_dependency(task_id, prerequisite_id)?;
            }
        },
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
