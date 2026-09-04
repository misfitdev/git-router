mod cli;
mod config;
mod credential;
mod ssh_wrap;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

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
    /// Wire git-router into the global gitconfig
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
        /// Git user.name for commits in repos matching this route
        #[arg(long)]
        user_name: Option<String>,
        /// Git user.email for commits in repos matching this route
        #[arg(long)]
        user_email: Option<String>,
    },

    /// List all configured routes
    List,

    /// Show full configuration details
    Show,

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

    /// Credential helper (called by git via generated credential config)
    #[command(name = "credential-helper")]
    CredentialHelper {
        /// Operation: get, store, or erase
        operation: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate for
        shell: Shell,
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
            user_name,
            user_email,
        } => cli::add(
            &host,
            &org,
            ssh_key.as_deref(),
            token.as_deref(),
            user_name.as_deref(),
            user_email.as_deref(),
        ),
        Commands::List => cli::list(),
        Commands::Show => cli::show(),
        Commands::Remove { host, org } => cli::remove(&host, &org),
        Commands::Doctor => cli::doctor(),
        Commands::SshWrap { args } => ssh_wrap::run(&args),
        Commands::CredentialHelper { operation } => {
            credential::run(&operation);
            Ok(())
        }
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "git-router",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("git-router: {e}");
        std::process::exit(1);
    }
}
