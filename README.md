# agy-usage

`agy-usage` is a CLI tool designed to track and manage Antigravity model quotas and usage.

## Features

- **Google OAuth Authentication**: Sign in and authenticate using your Google account.
- **Multi-Account Management**: Register multiple Google accounts and switch between them seamlessly.
- **Quota Information Display**: Fetch and display real-time model usage limits and quotas in your terminal (JSON output format is also supported).
- **Wakeup Functionality**: Trigger models to initiate quota limitation timers or wake up the model.

## Installation / Build

This tool is built with Rust. Follow the steps below to compile and run it:

1. Clone this repository.
2. Run the following command to build the release binary:

```bash
cargo build --release
```

The compiled binary will be generated at `target/release/agy-usage`.

## Usage

### 1. Authentication (Login)

Use the `login` command for the initial setup or to add a new account. A browser window will open automatically for OAuth authentication.

```bash
agy-usage login
```

If you are running the tool in a headless environment, use the `--no-browser` or `--manual` options.

### 2. Account Management

You can list, switch, or remove registered accounts.

```bash
# List all registered accounts
agy-usage accounts list

# Show the current active account
agy-usage accounts current

# Switch the active account
agy-usage accounts switch user@example.com

# Remove a registered account
agy-usage accounts remove user@example.com
```

### 3. Checking Quota and Usage

Display the quota information for the currently active account.

```bash
# Display quota
agy-usage quota

# Include all models (including Gemini 2.5 autocomplete models)
agy-usage quota --all-models

# Output in JSON format
agy-usage quota --json
```

### 4. Wakeup Trigger

Trigger models to start quota limitation timers or wake them up.

```bash
# Trigger wakeup for default models
agy-usage wakeup

# Specify models and a custom prompt
agy-usage wakeup --models gemini-3.1-pro-low,claude-sonnet-4-6 --prompt "Hello"

# Retain the original long system prompt
agy-usage wakeup --keep-system-prompt
```

### 5. Logging Out

Log out from one or all accounts.

```bash
# Log out from a specific account
agy-usage logout user@example.com

# Log out from all accounts
agy-usage logout --all
```

## License

[MIT License](LICENSE)

Copyright (c) 2026 Yoshihiro OKUMURA <orrisroot@gmail.com>
