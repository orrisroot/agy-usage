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

### 5. Periodic Execution (Scheduled Wakeup)

To keep models warmed up or to reset/trigger quota limits continuously, you can run the `wakeup` command periodically. Since the tool stores OAuth tokens per user, **make sure to run these tasks under the same user account that performed the login.**

#### Linux (cron)

Open your user crontab:
```bash
crontab -e
```

Add the following line to run the command every hour (adjust the path to your compiled binary and log path):
```cron
0 * * * * /absolute/path/to/agy-usage wakeup >> /absolute/path/to/agy-usage-wakeup.log 2>&1
```

#### Linux (systemd User Timer)

Alternatively, you can use systemd user timers which handle logging automatically and run reliably.

1. Create a service file `~/.config/systemd/user/agy-wakeup.service`:
```ini
[Unit]
Description=Trigger agy-usage wakeup
After=network.target

[Service]
Type=oneshot
ExecStart=/absolute/path/to/agy-usage wakeup
```

2. Create a timer file `~/.config/systemd/user/agy-wakeup.timer`:
```ini
[Unit]
Description=Run agy-usage wakeup periodically

[Timer]
OnCalendar=hourly
Persistent=true

[Install]
WantedBy=timers.target
```

3. Reload systemd configuration and enable the timer:
```bash
systemctl --user daemon-reload
systemctl --user enable --now agy-wakeup.timer
```

4. Check the status or logs:
```bash
systemctl --user status agy-wakeup.timer
journalctl --user -u agy-wakeup.service
```

#### macOS (launchd)

Create a user launch agent plist at `~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.agy-usage.wakeup</string>
    <key>ProgramArguments</key>
    <array>
        <string>/absolute/path/to/agy-usage</string>
        <string>wakeup</string>
    </array>
    <key>StartInterval</key>
    <integer>3600</integer> <!-- Run every hour (3600 seconds) -->
    <key>StandardOutPath</key>
    <string>/tmp/agy-usage-wakeup.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/agy-usage-wakeup.stderr.log</string>
</dict>
</plist>
```

Load the plist:
```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist
```

To stop/unload:
```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist
```

#### Windows (Task Scheduler)

Open PowerShell and run the following command to register a task that runs every hour:

```powershell
$action = New-ScheduledTaskAction -Execute "C:\path\to\agy-usage.exe" -Argument "wakeup"
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Hours 1)
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName "AgyUsageWakeup" -Action $action -Trigger $trigger -Settings $settings -Description "Trigger agy-usage wakeup"
```

Or you can create the task via the GUI **Task Scheduler**:
1. Create a Basic Task named "AgyUsageWakeup".
2. Set the Trigger to **Daily**, and under Advanced Settings set **Repeat task every** to **1 hour**.
3. Set the Action to **Start a program**, browse to `agy-usage.exe`, and add `wakeup` in the arguments.

### 6. Logging Out

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
