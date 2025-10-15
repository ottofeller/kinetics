use crate::commands;
use crate::crat::Crate;
use crate::error::Error;
use crate::function::Type as FunctionType;
use crate::logger::Logger;
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
enum ProjCommands {
    /// [DANGER] Destroy a project
    Destroy {},

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
    List {},
}

#[derive(Subcommand)]
enum FuncCommands {
    /// List all functions in the project
    List {
        /// Show detailed information for each function
        #[arg(short, long, action = ArgAction::SetTrue)]
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
}

#[derive(Subcommand)]
enum Commands {
    /// Commands for managing projects
    Proj {
        #[command(subcommand)]
        command: Option<ProjCommands>,
    },

    /// Commands for managing functions
    Func {
        #[command(subcommand)]
        command: Option<FuncCommands>,
    },

    /// Commands for managing environment variables
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

        #[arg(value_delimiter = ',')]
        functions: Vec<String>,
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
    },

    /// Login
    Login {
        /// Your registered email address
        #[arg()]
        email: String,
    },

    /// Invoke a function
    Invoke {
        #[arg()]
        name: String,

        #[arg(long, default_value = "{}")]
        headers: String,

        #[arg(short, long, default_value = "{}")]
        payload: String,

        #[arg(short, long, default_value = "")]
        table: String,

        #[arg(short, long, action = ArgAction::SetFalse)]
        remote: bool,

        /// Provision local SQL database for invoked function
        #[arg(long="with-database", visible_aliases=["with-db", "db"])]
        with_database: bool,
    },

    /// Logout from Kinetics platform
    Logout {},
}

pub async fn run(deploy_config: Option<Arc<dyn commands::deploy::DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // Commands that should be available outside of a project
    match &cli.command {
        Some(Commands::Login { email }) => {
            return commands::login::login(email).await.map_err(Error::from);
        }
        Some(Commands::Logout {}) => {
            return commands::logout::logout().await.map_err(Error::from);
        }
        Some(Commands::Init {
            name,
            cron,
            endpoint: _,
            worker,
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
            )
            .await
            .map_err(Error::from);
        }
        _ => {}
    }

    let crat = Crate::from_current_dir()?;

    color_eyre::config::HookBuilder::default()
        .display_location_section(false)
        .display_env_section(false)
        .theme(color_eyre::config::Theme::new())
        .install()?;

    // Project commands
    match &cli.command {
        Some(Commands::Proj {
            command: Some(ProjCommands::Destroy {}),
        }) => {
            return commands::proj::destroy::destroy(&crat)
                .await
                .wrap_err("Failed to destroy the project")
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::Rollback { version }),
        }) => {
            return commands::proj::rollback::rollback(&crat, *version)
                .await
                .wrap_err("Failed to rollback the project")
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjCommands::List {}),
        }) => return commands::proj::list::list().await,
        Some(Commands::Proj {
            command: Some(ProjCommands::Versions {}),
        }) => return commands::proj::versions::versions(&crat).await,
        _ => Ok(()),
    }?;

    // Functions commands
    match &cli.command {
        Some(Commands::Func {
            command: Some(FuncCommands::List { verbose }),
        }) => {
            return commands::func::list::list(&crat, *verbose)
                .await
                .wrap_err("Failed to list functions")
                .map_err(Error::from);
        }
        Some(Commands::Func {
            command: Some(FuncCommands::Stats { name, period }),
        }) => {
            return commands::func::stats::stats(name, &crat, *period)
                .await
                .wrap_err("Failed to get function statistics")
                .map_err(Error::from);
        }
        Some(Commands::Func {
            command: Some(FuncCommands::Logs { name, period }),
        }) => commands::func::logs::logs(name, &crat, period).await,
        _ => Ok(()),
    }?;

    // Envs commands
    match &cli.command {
        Some(Commands::Envs {
            command: Some(EnvsCommands::List {}),
        }) => {
            return commands::envs::list(&crat)
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
            ..
        }) => commands::deploy::run(functions, max_concurrency, *envs, deploy_config).await,
        Some(Commands::Invoke {
            name,
            payload,
            headers,
            table,
            remote,
            with_database: sqldb,
        }) => {
            commands::invoke::invoke(
                name,
                &crat,
                payload,
                headers,
                if !table.is_empty() { Some(table) } else { None },
                remote.to_owned(),
                sqldb.to_owned(),
            )
            .await
        }
        _ => Ok(()),
    }
    .map_err(Error::from)
}
