use clap::{Parser, Subcommand};
use colored::Colorize;
mod config;
mod google_api;
mod oauth;
mod quota;
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

    /// Account email to use (shortcut for quota --account)
    #[arg(short, long)]
    account: Option<String>,

    /// Enable debug output (shortcut for quota --debug / wakeup --debug)
    #[arg(long)]
    debug: bool,

    /// Force bypass cache to fetch latest data
    #[arg(short, long)]
    force: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with Google (adds a new account)
    Login {
        /// Do not open browser, print URL instead
        #[arg(long)]
        no_browser: bool,

        /// Manual login flow (copy-paste URL)
        #[arg(long)]
        manual: bool,

        /// Port for OAuth callback server
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Log out of one or all accounts
    Logout {
        /// Account email to log out from
        email: Option<String>,

        /// Log out of all accounts
        #[arg(long)]
        all: bool,
    },
    /// Show current login status
    Status,
    /// Manage multiple Google accounts
    Accounts {
        #[command(subcommand)]
        command: Option<AccountsCommands>,
    },
    /// Fetch and display quota information
    Quota {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Account email to check
        #[arg(short, long)]
        account: Option<String>,

        /// Enable debug output (shows API request and response details)
        #[arg(long)]
        debug: bool,

        /// Force bypass cache to fetch latest data
        #[arg(short, long)]
        force: bool,
    },
    /// Trigger models to start quota limitation timers / wakeup
    Wakeup {
        /// Models to trigger (comma separated)
        #[arg(long, value_delimiter = ',')]
        models: Option<Vec<String>>,

        /// Custom prompt to send
        #[arg(long)]
        prompt: Option<String>,

        /// Account email to use
        #[arg(short, long)]
        account: Option<String>,

        /// Retain the original long system prompt instead of omitting it
        #[arg(long)]
        keep_system_prompt: bool,

        /// Enable debug output (shows API request and response details)
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
    /// List all registered accounts
    List,
    /// Switch the active account
    Switch { email: String },
    /// Remove an account
    Remove { email: String },
    /// Show current active account
    Current,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Login {
            no_browser,
            manual,
            port,
        }) => {
            let opts = oauth::LoginOptions {
                no_browser,
                manual,
                port,
            };
            match oauth::run_login(opts).await {
                Ok(email) => {
                    println!(
                        "{} Logged in successfully as {}!",
                        "Success:".green().bold(),
                        email.bold()
                    );
                }
                Err(e) => {
                    eprintln!("{} Login failed: {}", "Error:".red().bold(), e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Logout { email, all }) => {
            let global = config::load_global_config();
            if all {
                let emails = config::list_accounts();
                if emails.is_empty() {
                    println!("No accounts are logged in.");
                    return;
                }
                for email in emails {
                    if let Err(e) = config::remove_account(&email) {
                        eprintln!("Failed to remove account {}: {}", email, e);
                    } else {
                        println!("Logged out of {}.", email);
                    }
                }
                println!("{}", "Successfully logged out of all accounts.".green());
            } else if let Some(ref email) = email {
                if let Err(e) = config::remove_account(email) {
                    eprintln!(
                        "{} Failed to logout from {}: {}",
                        "Error:".red().bold(),
                        email,
                        e
                    );
                    std::process::exit(1);
                }
                println!(
                    "{}",
                    format!("Successfully logged out of {}.", email).green()
                );
            } else if let Some(ref email) = global.active_email {
                if let Err(e) = config::remove_account(email) {
                    eprintln!(
                        "{} Failed to logout from {}: {}",
                        "Error:".red().bold(),
                        email,
                        e
                    );
                    std::process::exit(1);
                }
                println!(
                    "{}",
                    format!("Successfully logged out of active account {}.", email).green()
                );
            } else {
                println!("No active account to log out from.");
            }
        }
        Some(Commands::Status) => {
            let global = config::load_global_config();
            let accounts = config::list_accounts();
            if accounts.is_empty() {
                println!("Not logged in. Use 'agy-usage login' to authenticate.");
                return;
            }
            println!("Registered accounts:");
            for email in accounts {
                let is_active = global.active_email.as_ref() == Some(&email);
                let status_str = if is_active {
                    "[*] (active)".green().to_string()
                } else {
                    "".to_string()
                };
                println!(" - {} {}", email, status_str);
            }
        }
        Some(Commands::Accounts { command }) => {
            let subcmd = command.unwrap_or(AccountsCommands::List);
            match subcmd {
                AccountsCommands::List => {
                    let global = config::load_global_config();
                    let emails = config::list_accounts();
                    if emails.is_empty() {
                        println!("No accounts found. Use 'agy-usage login' to add one.");
                        return;
                    }
                    println!("\n📊 Registered Accounts:");
                    for email in emails {
                        let is_active = global.active_email.as_ref() == Some(&email);
                        if is_active {
                            println!("  {}", format!("* {}", email).green().bold());
                        } else {
                            println!("    {}", email);
                        }
                    }
                    println!("\n* = active account");
                }
                AccountsCommands::Switch { email } => {
                    let emails = config::list_accounts();
                    if !emails.contains(&email) {
                        eprintln!(
                            "{} Account {} not found. Log in first.",
                            "Error:".red().bold(),
                            email
                        );
                        std::process::exit(1);
                    }
                    let mut global = config::load_global_config();
                    global.active_email = Some(email.clone());
                    if let Err(e) = config::save_global_config(&global) {
                        eprintln!(
                            "{} Failed to save configuration: {}",
                            "Error:".red().bold(),
                            e
                        );
                        std::process::exit(1);
                    }
                    println!(
                        "{}",
                        format!("Switched active account to: {}", email).green()
                    );
                }
                AccountsCommands::Remove { email } => {
                    let emails = config::list_accounts();
                    if !emails.contains(&email) {
                        eprintln!("{} Account {} not found.", "Error:".red().bold(), email);
                        std::process::exit(1);
                    }
                    if let Err(e) = config::remove_account(&email) {
                        eprintln!("{} Failed to remove account: {}", "Error:".red().bold(), e);
                        std::process::exit(1);
                    }
                    println!("{}", format!("Removed account {}.", email).green());
                }
                AccountsCommands::Current => {
                    let global = config::load_global_config();
                    if let Some(ref email) = global.active_email {
                        println!("Active account: {}", email);
                    } else {
                        println!("No active account selected.");
                    }
                }
            }
        }
        Some(Commands::Quota {
            json,
            account,
            debug,
            force,
        }) => {
            let quota_opts = quota::QuotaOptions {
                json,
                account,
                debug: debug || cli.debug,
                force: force || cli.force,
            };
            if let Err(e) = quota::run_quota(quota_opts).await {
                eprintln!("{} {}", "Error:".red().bold(), e);
                std::process::exit(1);
            }
        }
        Some(Commands::Wakeup {
            models,
            prompt,
            account,
            keep_system_prompt,
            debug,
        }) => {
            let opts = wakeup::WakeupOptions {
                models,
                prompt,
                account,
                keep_system_prompt,
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
                account: cli.account,
                debug: cli.debug,
                force: cli.force,
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
