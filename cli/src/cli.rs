use crate::build::{self, prepare_crates};
use crate::config::build_config;
use crate::crat::Crate;
use crate::deploy::{self, DeployConfig};
use crate::destroy::destroy;
use crate::error::Error;
use crate::function::{Function, Type as FunctionType};
use crate::init::init;
use crate::invoke::invoke;
use crate::list::list;
use crate::logger::Logger;
use crate::login::login;
use crate::logout::logout;
use crate::logs::logs;
use clap::{ArgAction, Parser, Subcommand};
use eyre::{Ok, WrapErr};
use std::path::PathBuf;
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
enum Commands {
    /// Build your serverless functions
    Build {
        /// Comma-separated list of function names to build (if not specified, all functions will be built)
        #[arg(short, long)]
        functions: Option<String>,
    },

    /// Deploy your serverless functions to the cloud
    Deploy {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 6)]
        max_concurrency: usize,

        #[arg(short, long)]
        functions: Option<String>,
    },

    /// Destroy your serverless functions
    Destroy {},

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

    /// Invoke a functions
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
    },

    /// List all serverless functions
    List {
        /// Show detailed information for each function
        #[arg(short, long, action = ArgAction::SetTrue)]
        verbose: bool,
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

        Some(Commands::Init {
            name,
            cron,
            endpoint: _,
            worker,
        }) => {
            return init(
                name,
                if cron.to_owned() {
                    FunctionType::Cron
                } else if worker.to_owned() {
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

    // All functions to add to the template
    let all_functions: Vec<Function> =
        prepare_crates(PathBuf::from(build_config()?.build_path), crat.clone())?
            .into_iter()
            // Avoid building functions supposed for local invocations only
            .filter(|f| !f.is_local().unwrap())
            .collect();

    // Functions to deploy
    let mut deploy_functions: Vec<Function> = all_functions.clone();

    // Filter functions based on --functions parameter if provided
    let build_names = if let Some(Commands::Build { functions: names }) = &cli.command {
        names
    } else {
        &None::<String>
    };

    let deploy_names = if let Some(Commands::Deploy {
        functions: names, ..
    }) = &cli.command
    {
        names
    } else {
        &None::<String>
    };

    if build_names.is_some() || deploy_names.is_some() {
        let names: Vec<String> = build_names
            .clone()
            .unwrap_or(deploy_names.clone().unwrap_or_default())
            .split(',')
            .map(|s| s.trim().into())
            .collect();

        deploy_functions = all_functions
            .clone()
            .into_iter()
            .filter(|f| names.contains(&f.name.as_str().to_string()))
            .collect();
    }

    color_eyre::config::HookBuilder::default()
        .display_location_section(false)
        .display_env_section(false)
        .theme(color_eyre::config::Theme::new())
        .install()?;

    match &cli.command {
        Some(Commands::Build { .. }) => {
            build::run(all_functions.clone(), deploy_functions.clone()).await
        }
        Some(Commands::Deploy {
            max_concurrency, ..
        }) => {
            deploy::run(
                all_functions,
                deploy_functions,
                max_concurrency,
                deploy_config,
            )
            .await
        }
        Some(Commands::Destroy {}) => {
            destroy(&Crate::from_current_dir()?)
                .await
                .wrap_err("Failed to destroy the project")?;

            Ok(())
        }
        Some(Commands::Invoke {
            name,
            payload,
            headers,
            table,
            remote,
        }) => {
            invoke(
                &Function::find_by_name(&all_functions, name)?,
                &crat,
                payload,
                headers,
                if !table.is_empty() { Some(table) } else { None },
                remote.to_owned(),
            )
            .await
        }
        Some(Commands::Logs { name }) => {
            logs(&Function::find_by_name(&all_functions, name)?, &crat).await
        }
        Some(Commands::List { verbose }) => list(&crat, *verbose).await,
        Some(Commands::Logout {}) => logout().await,
        _ => Ok(()),
    }
    .map_err(Error::from)
}
