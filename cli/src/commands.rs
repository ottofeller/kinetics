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

    #[clap(subcommand)]
    Func(func::FuncCommands),

    /// Invoke a function
    Invoke(invoke::InvokeCommand),

    /// Deploy entire project or certain function(s)
    Deploy(deploy::DeployCommand),

    /// Build functions, without deployment
    Build(build::BuildCommand),

    /// Log in with your email
    Login(login::LoginCommand),
}
