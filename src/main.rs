mod cli;
mod config;
mod credential;
mod ssh_wrap;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "git-router",
    version,
    about = "Route SSH keys and HTTPS credentials by org"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Write core.sshCommand and credential.helper to global gitconfig
    Init,

    /// Add or update a route
    Add {
        /// Remote host (e.g., github.com)
        host: String,
        /// Organization or namespace
        org: String,
        /// Path to SSH key (public or private)
        #[arg(long)]
        ssh_key: Option<String>,
        /// HTTPS token
        #[arg(long)]
        token: Option<String>,
    },

    /// List all configured routes
    List,

    /// Remove a route
    Remove {
        /// Remote host
        host: String,
        /// Organization or namespace
        org: String,
    },

    /// Verify configuration and environment
    Doctor,

    /// SSH wrapper (called by git via core.sshCommand)
    #[command(name = "ssh-wrap")]
    SshWrap {
        /// Arguments passed by git to the SSH command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Credential helper (called by git via credential.helper)
    #[command(name = "credential-helper")]
    CredentialHelper {
        /// Operation: get, store, or erase
        operation: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => cli::init(),
        Commands::Add {
            host,
            org,
            ssh_key,
            token,
        } => cli::add(&host, &org, ssh_key.as_deref(), token.as_deref()),
        Commands::List => cli::list(),
        Commands::Remove { host, org } => cli::remove(&host, &org),
        Commands::Doctor => cli::doctor(),
        Commands::SshWrap { args } => ssh_wrap::run(&args),
        Commands::CredentialHelper { operation } => {
            credential::run(&operation);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("git-router: {e}");
        std::process::exit(1);
    }
}
