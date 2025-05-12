use crate::build::pipeline::Pipeline;
use crate::build::prepare_crates;
use crate::config::build_config;
use crate::crat::Crate;
use crate::deploy::DeployConfig;
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
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 6)]
        max_concurrency: usize,
    },

    /// Deploy your serverless functions to the cloud
    Deploy {
        /// Maximum number of parallel concurrent builds
        #[arg(short, long, default_value_t = 6)]
        max_concurrency: usize,
    },

    /// Destroy your serverless functions
    Destroy {},

    /// Start new Kinetics project
    Init {
        /// Name of the project to create
        #[arg()]
        name: String,

        // When true the endpoint template will be used
        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        cron: bool,

        #[arg(
            short,
            long,
            action = ArgAction::SetTrue,
            required = false
        )]
        endpoint: bool,

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
    },

    /// Show function logs
    Logs {
        /// Function name to retrieve logs for
        #[arg()]
        name: String,
    },

    /// List all serverless functions
    List {},

    /// Logout from Kinetics platform
    Logout {},
}

pub async fn run(deploy_config: Option<Arc<dyn DeployConfig>>) -> Result<(), Error> {
    Logger::init();
    let cli = Cli::parse();

    // Commands that should be available outside of a project
    match &cli.command {
        Some(Commands::Login { email }) => {
            let is_new_session = login(email).await?;

            println!(
                "{} {} {}",
                console::style(if is_new_session {
                    "Successfully logged in"
                } else {
                    "Already logged in"
                })
                .green()
                .bold(),
                console::style("via").dim(),
                console::style(email).underlined().bold()
            );

            return Ok(()).map_err(Error::from);
        }

        Some(Commands::Init {
            name,
            cron,
            endpoint: _,
            worker,
        }) => {
            init(
                name,
                if cron.to_owned() {
                    FunctionType::Cron
                } else if worker.to_owned() {
                    FunctionType::Worker
                } else {
                    FunctionType::Endpoint
                },
            )
            .await?;

            return Ok(()).map_err(Error::from);
        }

        _ => {}
    }

    let crat = Crate::from_current_dir()?;
    let directories = prepare_crates(PathBuf::from(build_config()?.build_path), crat.clone())?;

    // Functions to deploy
    let functions: Vec<Function> = directories
        .into_iter()
        .map(|p| Function::new(&p).unwrap())
        // Avoid building functions supposed for local invocations only
        .filter(|f| !f.is_local().unwrap())
        .collect();

    color_eyre::config::HookBuilder::default()
        .display_location_section(false)
        .display_env_section(false)
        .theme(color_eyre::config::Theme::new())
        .install()?;

    match &cli.command {
        Some(Commands::Build { max_concurrency }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(false)
                .set_crat(Crate::from_current_dir()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await?;

            Ok(())
        }
        Some(Commands::Deploy { max_concurrency }) => {
            Pipeline::builder()
                .set_max_concurrent(*max_concurrency)
                .with_deploy_enabled(true)
                .with_deploy_config(deploy_config)
                .set_crat(Crate::from_current_dir()?)
                .build()
                .wrap_err("Failed to build pipeline")?
                .run(functions)
                .await?;

            Ok(())
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
        }) => {
            invoke(
                &Function::find_by_name(&functions, name)?,
                &crat,
                payload,
                headers,
                if !table.is_empty() { Some(table) } else { None },
            )
            .await?;

            Ok(())
        }
        Some(Commands::Logs { name }) => {
            logs(&Function::find_by_name(&functions, name)?, &crat).await?;
            Ok(())
        }
        Some(Commands::List {}) => {
            if functions.is_empty() {
                println!("{}", console::style("No functions found").yellow());
            } else {
                list(&crat).await?;
            }
            Ok(())
        }
        Some(Commands::Logout {}) => match logout().await {
            Result::Ok(_) => {
                println!(
                    "{}",
                    console::style("You was successfully logged out")
                        .green()
                        .bold()
                );
                Ok(())
            }
            Err(error) => {
                println!("{}", console::style("Failed to logout").red().bold());
                Err(error)
            }
        },
        _ => Ok(()),
    }
    .map_err(Error::from)
}
