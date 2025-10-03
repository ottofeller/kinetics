use crate::build;
use crate::crat::Crate;
use crate::deploy::{self, DeployConfig};
use crate::destroy::destroy;
use crate::error::Error;
use crate::function::Type as FunctionType;
use crate::init::init;
use crate::invoke::invoke;
use crate::list::list;
use crate::logger::Logger;
use crate::login::login;
use crate::logout::logout;
use crate::logs::logs;
use crate::rollback::rollback;
use crate::stats::stats;
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
enum ProjectCommands {
    /// [DANGER] Destroy in the cloud
    Destroy {},

    /// Rollback to previous version
    Rollback {},
}

#[derive(Subcommand)]
enum FunctionsCommands {
    /// List all serverless functions
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
}

#[derive(Subcommand)]
enum Commands {
    /// Commands for managing projects
    Proj {
        #[command(subcommand)]
        command: Option<ProjectCommands>,
    },

    /// Commands for managing functions
    Func {
        #[command(subcommand)]
        command: Option<FunctionsCommands>,
    },

    /// Build your serverless functions
    Build {
        /// Comma-separated list of function names to build (if not specified, all functions will be built)
        #[arg(short, long, value_delimiter = ',')]
        functions: Vec<String>,
    },

    /// Deploy your serverless functions to the cloud
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

    /// Start new Kinetics project from template
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

    /// Login to Kinetics platform
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

    /// Logout from Kinetics platform
    Logout {},
}

pub async fn run(deploy_config: Option<Arc<dyn DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // Commands that should be available outside of a project
    match &cli.command {
        Some(Commands::Login { email }) => {
            return login(email).await.map_err(Error::from);
        }
        Some(Commands::Logout {}) => {
            return logout().await.map_err(Error::from);
        }
        Some(Commands::Init {
            name,
            cron,
            endpoint: _,
            worker,
        }) => {
            return init(
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
            command: Some(ProjectCommands::Destroy {}),
        }) => {
            return destroy(&crat)
                .await
                .wrap_err("Failed to destroy the project")
                .map_err(Error::from);
        }
        Some(Commands::Proj {
            command: Some(ProjectCommands::Rollback {}),
        }) => {
            return rollback(&crat)
                .await
                .wrap_err("Failed to rollback the project")
                .map_err(Error::from);
        }
        _ => Ok(()),
    }?;

    // Functions commands
    match &cli.command {
        Some(Commands::Func {
            command: Some(FunctionsCommands::List { verbose }),
        }) => {
            return list(&crat, *verbose)
                .await
                .wrap_err("Failed to destroy the project")
                .map_err(Error::from);
        }
        Some(Commands::Func {
            command: Some(FunctionsCommands::Stats { name, period }),
        }) => {
            return stats(name, &crat, *period)
                .await
                .wrap_err("Failed to rollback the project")
                .map_err(Error::from);
        }
        _ => Ok(()),
    }?;

    // Global commands
    match &cli.command {
        Some(Commands::Build { functions, .. }) => build::run(functions).await,
        Some(Commands::Deploy {
            functions,
            max_concurrency,
            envs,
            ..
        }) => deploy::run(functions, max_concurrency, *envs, deploy_config).await,
        Some(Commands::Invoke {
            name,
            payload,
            headers,
            table,
            remote,
        }) => {
            invoke(
                name,
                &crat,
                payload,
                headers,
                if !table.is_empty() { Some(table) } else { None },
                remote.to_owned(),
            )
            .await
        }
        Some(Commands::Logs { name, period }) => logs(name, &crat, period).await,
        _ => Ok(()),
    }
    .map_err(Error::from)
}
