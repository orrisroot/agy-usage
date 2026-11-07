use clap::{Parser, Subcommand};
use colored::Colorize;
mod agy;
mod config;
mod quota;
mod storage;
mod wakeup;

#[derive(Parser)]
#[command(name = "agy-usage")]
#[command(version)]
#[command(about = "CLI tool to track Antigravity model quota and usage", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output as JSON (shortcut for quota --json)
    #[arg(long)]
    json: bool,

    /// Enable debug output (shortcut for quota --debug / wakeup --debug)
    #[arg(long)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the authenticated agy CLI account
    Status,
    /// Show the authenticated agy CLI account
    Accounts {
        #[command(subcommand)]
        command: Option<AccountsCommands>,
    },
    /// Fetch and display quota information
    Quota {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Enable debug output (shows `agy` stderr)
        #[arg(long)]
        debug: bool,
    },
    /// Trigger models to start quota limitation timers / wakeup
    Wakeup {
        /// Models to trigger (comma separated)
        #[arg(long, value_delimiter = ',')]
        models: Option<Vec<String>>,

        /// Custom prompt to send
        #[arg(long)]
        prompt: Option<String>,

        /// Enable debug output (shows `agy` stderr)
        #[arg(long)]
        debug: bool,
    },
    /// Update the CLI to the latest version
    SelfUpdate,
    /// Show the current version of the CLI
    Version,
}

#[derive(Subcommand)]
enum AccountsCommands {
    /// List the authenticated agy CLI account
    List,
    /// Show the authenticated agy CLI account
    Current,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => match crate::agy::read_account() {
            Some(account) => {
                println!("{}", "agy CLI authentication".blue().bold());
                println!("   Status: {}", "logged in".green().bold());
                match account.email() {
                    Some(email) => println!("   Account: {}", email.bold()),
                    None => println!("   Account: unknown"),
                }
                if let Some(method) = account
                    .auth_method
                    .as_deref()
                    .filter(|method| !method.is_empty())
                {
                    println!("   Auth method: {}", method);
                }
            }
            None => {
                println!("Not logged in to the agy CLI. Run `agy` to authenticate.");
            }
        },
        Some(Commands::Accounts { command }) => {
            let subcmd = command.unwrap_or(AccountsCommands::List);
            match subcmd {
                AccountsCommands::List => match crate::agy::read_account() {
                    Some(account) => {
                        println!("\n📊 agy CLI Account:");
                        let email = account.email().unwrap_or("unknown");
                        println!("  {}", format!("* {}", email).green().bold());
                    }
                    None => {
                        println!("No agy CLI account found. Run `agy` to authenticate.");
                    }
                },
                AccountsCommands::Current => match crate::agy::read_account() {
                    Some(account) => {
                        println!("agy CLI account: {}", account.email().unwrap_or("unknown"));
                    }
                    None => {
                        println!("No agy CLI account found. Run `agy` to authenticate.");
                    }
                },
            }
        }
        Some(Commands::Quota { json, debug }) => {
            let quota_opts = quota::QuotaOptions {
                json,
                debug: debug || cli.debug,
            };
            if let Err(e) = quota::run_quota(quota_opts).await {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Wakeup {
            models,
            prompt,
            debug,
        }) => {
            let opts = wakeup::WakeupOptions {
                models,
                prompt,
                debug: debug || cli.debug,
            };
            if let Err(e) = wakeup::run_wakeup(opts).await {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::SelfUpdate) => {
            if let Err(e) = run_self_update() {
                eprintln!("{} Self-update failed: {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Version) => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        }
        None => {
            let quota_opts = quota::QuotaOptions {
                json: cli.json,
                debug: cli.debug,
            };
            if let Err(e) = quota::run_quota(quota_opts).await {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
    }
}

fn run_self_update() -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking for updates...");
    let status = self_update::backends::github::Update::configure()
        .repo_owner("orrisroot")
        .repo_name("agy-usage")
        .bin_name("agy-usage")
        .show_download_progress(true)
        .current_version(self_update::cargo_crate_version!())
        .build()?
        .update()?;

    if status.updated() {
        println!(
            "{} Updated to version {}!",
            "Success:".green().bold(),
            status.version()
        );
    } else {
        println!("Already up to date (version {}).", status.version());
    }
    Ok(())
}
