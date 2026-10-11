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
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
}

#[derive(Subcommand)]
enum RunCommand {
    /// Execute a real, credential-isolated OpenCode proposal.
    Opencode {
        task: i64,
        #[command(flatten)]
        options: AgentOptions,
    },
    /// Execute the oldest eligible ready task in this project.
    Next {
        #[arg(long)]
        project: i64,
        #[command(flatten)]
        options: AgentOptions,
    },
    /// Deterministic simulation, never a real agent.
    Mock {
        task: i64,
        #[arg(long)]
        fail: bool,
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
    },
    List {
        task: i64,
    },
    Events {
        id: i64,
    },
    Cancel {
        id: i64,
    },
    Recover,
}

#[derive(clap::Args)]
struct AgentOptions {
    #[arg(long, default_value = "free")]
    policy: String,
    #[arg(long)]
    model: Option<String>,
    #[arg(long, default_value = "opencode")]
    executable: PathBuf,
    #[arg(long, default_value_t = 120)]
    timeout: u64,
}

#[derive(Subcommand)]
enum ModelCommand {
    Add {
        id: String,
        #[arg(long)]
        cost: String,
        #[arg(long)]
        access: String,
        #[arg(long)]
        enable: bool,
        #[arg(long, default_value_t = 0)]
        priority: i64,
    },
    List,
    Route {
        #[arg(long, default_value = "free")]
        policy: String,
        #[arg(long)]
        model: Option<String>,
    },
    /// Fetch metadata only; imported entries are disabled and cost remains unknown.
    Discover,
}

fn agent(db: &std::path::Path, task: i64, options: AgentOptions) -> Result<i64, Error> {
    let model = Store::open(db)?.route(&options.policy, options.model.as_deref())?;
    let backend = forge::backend::OpenCode::new(
        options.executable,
        &model,
        std::time::Duration::from_secs(options.timeout),
    )?;
    forge::backend::execute(db, task, &backend)
}

#[derive(Subcommand)]
enum ProjectCommand {
    Add { name: String },
    List,
}

#[derive(Subcommand)]
enum TaskCommand {
    Status {
        id: i64,
        status: String,
    },
    Ready {
        #[arg(long)]
        project: i64,
    },
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
        Command::Model { command } => {
            let mut store = Store::open(&cli.db)?;
            match command {
                ModelCommand::Add {
                    id,
                    cost,
                    access,
                    enable,
                    priority,
                } => store.register_model(&forge::providers::Model {
                    id,
                    cost,
                    access,
                    enabled: enable,
                    priority,
                    source: "manual".into(),
                })?,
                ModelCommand::List => {
                    for model in store.models()? {
                        println!(
                            "{}\t{}\t{}\t{}\t{}",
                            model.id,
                            model.cost,
                            model.access,
                            model.source,
                            if model.enabled { "enabled" } else { "disabled" }
                        );
                    }
                }
                ModelCommand::Route { policy, model } => {
                    println!("{}", store.route(&policy, model.as_deref())?.id)
                }
                ModelCommand::Discover => println!(
                    "{} discovered models (disabled, cost unverified)",
                    forge::providers::discover(&mut store)?
                ),
            }
        }
        Command::Run { command } => {
            use forge::execution::{WorkerGuard, run_mock};
            match command {
                RunCommand::Opencode { task, options } => {
                    println!("{}", agent(&cli.db, task, options)?)
                }
                RunCommand::Next { project, options } => {
                    let ready = Store::open(&cli.db)?.ready_tasks(project)?;
                    let task = *ready
                        .first()
                        .ok_or_else(|| Error::Policy("no eligible ready tasks".into()))?;
                    println!("{}", agent(&cli.db, task, options)?);
                }
                RunCommand::Mock {
                    task,
                    fail,
                    delay_ms,
                } => println!("{}", run_mock(&cli.db, task, fail, delay_ms)?),
                RunCommand::List { task } => {
                    for run in Store::open(&cli.db)?.runs(task)? {
                        println!("{}\t{}\t{}\t{}", run.id, run.backend, run.model, run.status);
                    }
                }
                RunCommand::Events { id } => {
                    for (kind, detail) in Store::open(&cli.db)?.events(id)? {
                        println!("{kind}\t{detail}");
                    }
                }
                RunCommand::Cancel { id } => Store::open(&cli.db)?.request_cancel(id)?,
                RunCommand::Recover => {
                    let mut store = Store::open(&cli.db)?;
                    let guard = WorkerGuard::acquire(&cli.db)?;
                    println!("{} interrupted runs recovered", store.recover(&guard)?);
                }
            }
        }
        Command::Task {
            command: TaskCommand::Status { id, status },
        } => Store::open(&cli.db)?.change_status(id, status.parse()?)?,
        Command::Task {
            command: TaskCommand::Ready { project },
        } => {
            for id in Store::open(&cli.db)?.ready_tasks(project)? {
                println!("{id}");
            }
        }
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
