use crate::commands::build::pipeline::Pipeline;
use crate::commands::{self};
use crate::config::deploy::DeployConfig;
use crate::credentials::Credentials;
use crate::error::Error;
use crate::function::Type as FunctionType;
use crate::logger::Logger;
use crate::project::Project;
use clap::{ArgAction, Parser, Subcommand};
use eyre::{eyre, Ok, WrapErr};
use std::sync::Arc;

#[derive(Parser)]
#[command(
    arg_required_else_help = true,
    name = "kinetics",
    version,
    about = "CLI tool for building and deploying serverless Rust functions",
    long_about = "A comprehensive CLI for managing Kinetics serverless Rust functions, including building, deploying and managing your infrastructure."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum MigrationsCommands {
    /// Create a new migration file
    Create {
        /// User-defined name for the migration
        #[arg(value_name = "NAME")]
        name: Option<String>,

        /// Relative path to migrations directory
        #[arg(short, long, value_name = "PATH", default_value = "migrations")]
        path: String,
    },

    /// Apply migrations to remote DB
    Apply {
        /// Relative path to migrations directory
        #[arg(short, long, value_name = "PATH", default_value = "migrations")]
        path: String,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Delete local access token locally and remotely
    Logout {},
}

#[derive(Subcommand)]
enum ProjCommands {
    /// [DANGER] Destroy a project
    Destroy {
        /// Name of the project to destroy (optional, defaults to current project name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Rollback to older version
    Rollback {
        /// Specific version to rollback to (optional)
        #[arg(short, long)]
        version: Option<u32>,
    },

    /// List projects
    List {},

    /// List all available versions
    Versions {},
}

#[derive(Subcommand)]
enum EnvsCommands {
    /// List all environment variables for all functions
    List {
        /// When passed shows env vars used by deployed functions
        #[arg(short, long, action = ArgAction::SetTrue)]
        remote: bool,
    },
}

#[derive(Subcommand)]
enum CicdCommands {
    /// Initialize a CI/CD pipeline
    Init {
        /// Create a GitHub workflow file.
        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        github: bool,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Manage projects
    Proj {
        #[command(subcommand)]
        command: Option<ProjCommands>,
    },

    /// Environment variables for functions
    Envs {
        #[command(subcommand)]
        command: Option<EnvsCommands>,
    },

    /// Legacy/deprecated deploy path. Use `kinetics deploy` instead.
    DeployOld {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 3)]
        max_concurrency: usize,

        /// Deploy only environment variables instead of full deployment
        #[arg(short, long, action = ArgAction::SetTrue)]
        envs: bool,

        /// Use hotswap deployment for faster updates
        #[arg(long, action = ArgAction::SetTrue)]
        hotswap: bool,

        #[arg(value_delimiter = ',')]
        functions: Vec<String>,
    },

    /// Database migrations
    Migrations {
        #[command(subcommand)]
        command: Option<MigrationsCommands>,
    },

    /// Start new project from template
    Init {
        /// Name of the project to create
        #[arg()]
        name: String,

        /// Cron job template
        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        cron: bool,

        /// REST API endpoint
        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        endpoint: bool,

        /// Queue worker
        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        worker: bool,

        /// Disable git repository initialization
        #[arg(short, long)]
        no_git: bool,
    },

    /// Login
    Login {
        /// Your registered email address
        #[arg()]
        email: String,
    },

    /// Manage GitHub (and pther providers') workflows
    Cicd {
        #[command(subcommand)]
        command: Option<CicdCommands>,
    },
}

impl Commands {
    /// Determines whether the current command requires authentication
    ///
    /// Returns true for all commands except those explicitly marked as not requiring authentication.
    /// Used to prevent situations when a long running command (like deploy) gets interrupted after doing some job
    /// because of the missing credentials.
    pub fn requires_auth(&self) -> bool {
        match self {
            Commands::Init { .. } => false,
            Commands::Login { .. } => false,
            Commands::Envs {
                command: Some(EnvsCommands::List { .. }),
            } => false,
            Commands::Cicd { .. } => false,
            Commands::Migrations {
                command: Some(MigrationsCommands::Create { .. }),
            } => false,
            _ => true,
        }
    }
}

pub async fn run(deploy_config: Option<Arc<dyn DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // Check credentials for commands that require authentication
    if cli.command.as_ref().is_some_and(|c| c.requires_auth()) {
        let credentials = Credentials::new().await.map_err(Error::from)?;

        if !credentials.is_valid() {
            return Err(Error::from(eyre!(
                "Please run `kinetics login <email>` to authenticate."
            )));
        }
    }

    // Commands that should be available outside of a project
    match &cli.command {
        Some(Commands::Init {
            name,
            cron,
            endpoint: _,
            worker,
            no_git,
        }) => {
            return commands::init::init(
                name,
                if *cron {
                    FunctionType::Cron
                } else if *worker {
                    FunctionType::Worker
                } else {
                    FunctionType::Endpoint
                },
                !*no_git,
            )
            .await
            .map_err(Error::from);
        }
        _ => {}
    }

    let project = Project::from_current_dir();

    // Since this point all commands need the project to be presented
    let project = project.wrap_err(
        "Either provide \"--name <project name>\" argument or run command in project's dir",
    )?;

    // Project commands
    match &cli.command {
        Some(Commands::Proj {
            command: Some(ProjCommands::Destroy { name }),
        }) => {
            return commands::proj::destroy::destroy(&project, name.as_deref())
                .await
                .inspect_err(|err| log::error!("Error: {:?}", err))
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::Rollback { version }),
        }) => {
            return commands::proj::rollback::rollback(&project, *version)
                .await
                .wrap_err("Failed to rollback the project")
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::List {}),
        }) => return commands::proj::list::list().await,
        Some(Commands::Proj {
            command: Some(ProjCommands::Versions {}),
        }) => return commands::proj::versions::versions(&project).await,
        _ => Ok(()),
    }?;

    // Migrations commands
    match &cli.command {
        Some(Commands::Migrations {
            command: Some(MigrationsCommands::Create { name, path }),
        }) => {
            return commands::migrations::create(&project, path, name.as_deref())
                .await
                .wrap_err("Failed to create migration")
                .map_err(Error::from);
        }

        Some(Commands::Migrations {
            command: Some(MigrationsCommands::Apply { path }),
        }) => {
            return commands::migrations::apply(&project, path)
                .await
                .wrap_err("Failed to apply migrations")
                .map_err(Error::from);
        }

        _ => Ok(()),
    }?;

    // DEPRECATED This is left to maintain compatibility with the backend
    // Global commands
    match &cli.command {
        Some(Commands::DeployOld {
            functions,
            max_concurrency,
            hotswap,
            ..
        }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(true)
                .with_hotswap(*hotswap)
                .with_deploy_config(deploy_config)
                .set_project(Project::from_current_dir()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await
        }
        _ => Ok(()),
    }
    .map_err(Error::from)
}
