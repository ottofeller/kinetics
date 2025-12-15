use crate::commands::{self};
use crate::api::func::toggle;
use crate::config::deploy::DeployConfig;
use crate::error::Error;
use crate::function::Type as FunctionType;
use crate::logger::Logger;
use crate::project::Project;
use clap::{ArgAction, Parser, Subcommand};
use eyre::{Ok, WrapErr};
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
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Delete local access token locally and remotely
    Logout {},

    /// Create a new authentication token
    Token {
        /// Time period for which the token is active.
        ///
        /// The period object (e.g. `1day 3hours`) is a concatenation of time spans.
        /// Where each time span is an integer number and a suffix representing time units.
        ///
        /// Defaults to 30days.
        ///
        #[arg(short, long)]
        period: Option<String>,
    },
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
enum FuncCommands {
    /// List all functions in the project
    List {
        /// Show detailed information for each function
        #[arg(short, long)]
        verbose: bool,
    },

    /// Get function statistics,
    /// that include run statistics (error/success/total count)
    /// as well as last call time and status.
    Stats {
        /// Function name to get statistics for.
        /// Run `kinetics list` to get a complete list of function names in a project.
        #[arg()]
        name: String,

        /// Period to get statistics for (in days).
        /// Maximum value is 7 days.
        #[arg(short, long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=7))]
        period: u32,
    },

    /// Show function logs
    Logs {
        /// Function name to retrieve logs for
        #[arg()]
        name: String,

        /// Time period to get logs for.
        ///
        /// The period object (e.g. `1day 3hours`) is a concatenation of time spans.
        /// Where each time span is an integer number and a suffix representing time units.
        ///
        /// Maximum available period is 1 month.
        /// Defaults to 1hour.
        ///
        #[arg(short, long)]
        period: Option<String>,
    },

    /// Stop function in the cloud
    Stop {
        /// Function name to stop
        #[arg()]
        name: String,
    },

    /// Start previously stopped function
    Start {
        /// Function name to start
        #[arg()]
        name: String,
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
    /// Auth and access tokens
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommands>,
    },

    /// Manage projects
    Proj {
        #[command(subcommand)]
        command: Option<ProjCommands>,
    },

    /// Functions log, telemetry, and management
    Func {
        #[command(subcommand)]
        command: Option<FuncCommands>,
    },

    /// Environment variables for functions
    Envs {
        #[command(subcommand)]
        command: Option<EnvsCommands>,
    },

    /// Build functions, without deployment
    Build {
        /// Comma-separated list of function names to build (if not specified, all functions will be built)
        #[arg(short, long, value_delimiter = ',')]
        functions: Vec<String>,
    },

    /// Deploy your functions
    Deploy {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 10)]
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

    /// Invoke a function
    Invoke {
        /// Name of a function, use "kinetics func list" to see all names
        #[arg()]
        name: String,

        /// Headers to be sent to endpoint function, in JSON.
        ///
        /// Example: --headers '{"auth": "Bearer 111"}'.
        #[arg(long)]
        headers: Option<String>,

        /// Set URL path while calling endpoint function.
        /// Required for endpoints with parametrized URLs, e.g. /user/*/profile.
        ///
        /// Example: --url-path /user/1/profile
        #[arg(long)]
        url_path: Option<String>,

        /// Must be a valid JSON.
        /// In case of endpoint functions payload is a body.
        /// In case of workers payload is queue event payload.
        ///
        /// Example: --payload '{"name": "John Smith"}'
        #[arg(short, long)]
        payload: Option<String>,

        /// [DEPRECATED]
        #[arg(short, long)]
        table: Option<String>,

        /// Invoke function remotely. Only works if function was deployed before.
        #[arg(short, long, action = ArgAction::SetFalse)]
        remote: bool,

        /// Provision local SQL database for invoked function to use. Not available when called with --remote flag.
        #[arg(long="with-database", visible_aliases=["with-db", "db"])]
        with_database: bool,

        /// Apply migrations to locally provisioned database. Not available when called with --remote flag.
        ///
        /// Accepts a path to dir with SQL-files relative to crate's root, defaults to <crate>/migrations/
        #[arg(short, long = "with-migrations", num_args = 0..=1, default_missing_value = "")]
        with_migrations: Option<String>,

        /// Provision a queue. Helpful when you test a function which sends something to queue. Not available when called with --remote flag.
        #[arg(long="with-queue", visible_aliases=["queue"])]
        with_queue: bool,
    },

    /// Manage GitHub (and pther providers') workflows
    Cicd {
        #[command(subcommand)]
        command: Option<CicdCommands>,
    },
}

pub async fn run(deploy_config: Option<Arc<dyn DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // Commands that should be available outside of a project
    match &cli.command {
        Some(Commands::Login { email }) => {
            return commands::login::login(email).await.map_err(Error::from);
        }
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

    // Auth commands
    match &cli.command {
        Some(Commands::Auth {
            command: Some(AuthCommands::Logout {}),
        }) => {
            return commands::auth::logout::logout().await.map_err(Error::from);
        }
        Some(Commands::Auth {
            command: Some(AuthCommands::Token { period }),
        }) => {
            return commands::auth::token::token(period)
                .await
                .map_err(Error::from);
        }
        _ => Ok(()),
    }?;

    // Project commands
    match &cli.command {
        Some(Commands::Proj {
            command: Some(ProjCommands::Destroy { name }),
        }) => {
            return commands::proj::destroy::destroy(&project.ok(), name.as_deref())
                .await
                .inspect_err(|err| log::error!("Error: {:?}", err))
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::Rollback { version }),
        }) => {
            return commands::proj::rollback::rollback(&project?, *version)
                .await
                .wrap_err("Failed to rollback the project")
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::List {}),
        }) => return commands::proj::list::list().await,
        Some(Commands::Proj {
            command: Some(ProjCommands::Versions {}),
        }) => return commands::proj::versions::versions(&project?).await,
        _ => Ok(()),
    }?;

    // Since this point all commands need the project to be presented
    let project = project?;

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

        _ => Ok(()),
    }?;

    // Functions commands
    match &cli.command {
        Some(Commands::Func {
            command: Some(FuncCommands::List { verbose }),
        }) => {
            return commands::func::list::list(&project, *verbose)
                .await
                .wrap_err("Failed to list functions")
                .map_err(Error::from);
        }
        Some(Commands::Func {
            command: Some(FuncCommands::Stats { name, period }),
        }) => {
            return commands::func::stats::stats(name, &project, *period)
                .await
                .wrap_err("Failed to get function statistics")
                .map_err(Error::from);
        }
        Some(Commands::Func {
            command: Some(FuncCommands::Logs { name, period }),
        }) => commands::func::logs::logs(name, &project, period).await,
        Some(Commands::Func {
            command: Some(FuncCommands::Stop { name }),
        }) => commands::func::toggle::toggle(name, &project, toggle::Op::Stop).await,
        Some(Commands::Func {
            command: Some(FuncCommands::Start { name }),
        }) => commands::func::toggle::toggle(name, &project, toggle::Op::Start).await,
        _ => Ok(()),
    }?;

    // CI/CD commands
    match &cli.command {
        Some(Commands::Cicd {
            command: Some(CicdCommands::Init { github }),
        }) => commands::cicd::init::init(&project, *github).await,
        _ => Ok(()),
    }?;

    // Envs commands
    match &cli.command {
        Some(Commands::Envs {
            command: Some(EnvsCommands::List { remote }),
        }) => {
            return commands::envs::list::list(&project, *remote)
                .await
                .wrap_err("Failed to list environment variables")
                .inspect_err(|e| log::error!("{e:?}"))
                .map_err(Error::from);
        }
        _ => Ok(()),
    }?;

    // Global commands
    match &cli.command {
        Some(Commands::Build { functions, .. }) => commands::build::run(functions).await,
        Some(Commands::Deploy {
            functions,
            max_concurrency,
            envs,
            hotswap,
            ..
        }) => {
            commands::deploy::run(functions, max_concurrency, *envs, *hotswap, deploy_config).await
        }
        Some(Commands::Invoke {
            name,
            payload,
            headers,
            url_path,
            table,
            remote,
            with_database: sqldb,
            with_queue,
            with_migrations,
        }) => {
            commands::invoke::invoke(
                name,
                &project,
                payload.as_deref(),
                headers.as_deref(),
                url_path.as_deref(),
                table.as_deref(),
                remote.to_owned(),
                sqldb.to_owned(),
                with_queue.to_owned(),
                with_migrations.as_deref(),
            )
            .await
        }
        _ => Ok(()),
    }
    .map_err(Error::from)
}
