pub mod auth;
pub mod build;
pub mod cicd;
pub mod deploy;
pub mod envs;
pub mod func;
pub mod init;
pub mod invoke;
pub mod login;
pub mod migrations;
pub mod proj;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Commands {
    #[clap(subcommand)]
    Auth(auth::AuthCommands),

    /// Manage GitHub (and other providers') workflows
    #[clap(subcommand)]
    Cicd(cicd::CicdCommands),

    /// Environment variables for functions
    #[clap(subcommand)]
    Envs(envs::EnvsCommands),

    #[clap(subcommand)]
    Func(func::FuncCommands),

    /// Database migrations
    #[clap(subcommand)]
    Migrations(migrations::MigrationsCommands),

    /// Invoke a function
    Invoke(invoke::InvokeCommand),

    /// Deploy entire project or certain function(s)
    Deploy(deploy::DeployCommand),

    /// Build functions, without deployment
    Build(build::BuildCommand),

    /// Log in with your email
    Login(login::LoginCommand),
}
