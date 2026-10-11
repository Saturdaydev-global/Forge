use std::{path::PathBuf, process::ExitCode, time::Duration};

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
    db: Option<PathBuf>,
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
    /// Detect and exercise an ACP harness.
    Acp {
        #[command(subcommand)]
        command: AcpCommand,
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

#[derive(Subcommand)]
enum AcpCommand {
    /// Detect harness name, version, and ACP subcommand support.
    Detect {
        #[arg(long, default_value = "opencode")]
        executable: PathBuf,
    },
    /// Exercise initialize + session/new (and optionally one prompt turn).
    Probe {
        #[arg(long, default_value = "opencode")]
        executable: PathBuf,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long = "arg", default_value = "acp")]
        args: Vec<String>,
    },
}

fn run(cli: Cli) -> Result<(), Error> {
    // Commands that touch the database require --db; ACP commands do not.
    let db = || {
        cli.db
            .as_deref()
            .ok_or_else(|| Error::Policy("--db is required for this command".into()))
    };
    match cli.command {
        Command::Model { command } => {
            let mut store = Store::open(db()?)?;
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
                    println!("{}", agent(db()?, task, options)?)
                }
                RunCommand::Next { project, options } => {
                    let ready = Store::open(db()?)?.ready_tasks(project)?;
                    let task = *ready
                        .first()
                        .ok_or_else(|| Error::Policy("no eligible ready tasks".into()))?;
                    println!("{}", agent(db()?, task, options)?);
                }
                RunCommand::Mock {
                    task,
                    fail,
                    delay_ms,
                } => println!("{}", run_mock(db()?, task, fail, delay_ms)?),
                RunCommand::List { task } => {
                    for run in Store::open(db()?)?.runs(task)? {
                        println!("{}\t{}\t{}\t{}", run.id, run.backend, run.model, run.status);
                    }
                }
                RunCommand::Events { id } => {
                    for (kind, detail) in Store::open(db()?)?.events(id)? {
                        println!("{kind}\t{detail}");
                    }
                }
                RunCommand::Cancel { id } => Store::open(db()?)?.request_cancel(id)?,
                RunCommand::Recover => {
                    let mut store = Store::open(db()?)?;
                    let guard = WorkerGuard::acquire(db()?)?;
                    println!("{} interrupted runs recovered", store.recover(&guard)?);
                }
            }
        }
        Command::Task {
            command: TaskCommand::Status { id, status },
        } => Store::open(db()?)?.change_status(id, status.parse()?)?,
        Command::Task {
            command: TaskCommand::Ready { project },
        } => {
            for id in Store::open(db()?)?.ready_tasks(project)? {
                println!("{id}");
            }
        }
        Command::Project {
            command: ProjectCommand::Add { name },
        } => {
            let name = ProjectName::new(&name)?;
            let project = Store::open(db()?)?.add_project(name)?;
            println!("{}\t{}", project.id, project.name.as_str());
        }
        Command::Project {
            command: ProjectCommand::List,
        } => {
            for project in Store::open(db()?)?.list_projects()? {
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
            let task = Store::open(db()?)?.add_task(project, title, criteria)?;
            println!("{}\t{}", task.id, task.title.as_str());
        }
        Command::Task {
            command: TaskCommand::List { project },
        } => {
            for task in Store::open(db()?)?.list_tasks(project)? {
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
            let store = Store::open(db()?)?;
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
                Store::open(db()?)?.add_dependency(task_id, prerequisite_id)?;
            }
            DependencyCommand::List { task_id } => {
                for dependency in Store::open(db()?)?.list_dependencies(task_id)? {
                    println!("{dependency}");
                }
            }
            DependencyCommand::Remove {
                task_id,
                prerequisite_id,
            } => {
                Store::open(db()?)?.remove_dependency(task_id, prerequisite_id)?;
            }
        },
        Command::Acp { command } => match command {
            AcpCommand::Detect { executable } => {
                let info = forge::acp::detect_harness(&executable)?;
                println!(
                    "{}\t{}\tacp={}",
                    info.name, info.version, info.acp_supported
                );
            }
            AcpCommand::Probe {
                executable,
                cwd,
                timeout,
                prompt,
                args,
            } => {
                let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap());
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let options = forge::acp::ExerciseOptions {
                    cwd,
                    timeout: Duration::from_secs(timeout),
                    prompt,
                    cancel_after_updates: None,
                    extra_envs: Vec::new(),
                };
                let report = forge::acp::exercise(&executable, &arg_refs, options)?;
                println!(
                    "agent={}\tversion={}\tprotocol={}\tsession={}\tstop={}\tupdates={}",
                    report.agent_name.as_deref().unwrap_or("-"),
                    report.agent_version.as_deref().unwrap_or("-"),
                    report.protocol_version,
                    report.session_id,
                    report.stop_reason.as_deref().unwrap_or("-"),
                    report.updates.len()
                );
                for method in &report.auth_methods {
                    println!(
                        "auth={}\tkind={}\tname={}",
                        method.id, method.kind, method.name
                    );
                }
                if !report.agent_text.is_empty() {
                    println!("text={}", report.agent_text);
                }
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
