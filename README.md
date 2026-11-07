# agy-usage

`agy-usage` is a CLI tool designed to track Antigravity model quotas and usage.

## Features

- **Quota Information Display**: Runs the Antigravity `agy` CLI's `/usage` command and renders the structured result in your terminal. Output automatically falls back to Markdown format when piped or redirected, making it CI/CD and script-friendly. JSON output format is also supported.
- **Wakeup Functionality**: Discovers models with `agy models` and triggers them via `agy -p` to initiate quota limitation timers or wake up models.
- **Atomic Cache Writes**: Cache files are written to a temporary file, flushed securely to physical disk (`sync_all`), and renamed atomically to prevent corruption on crash or power loss.
- **Path Sanitization**: Cache keys are sanitized before being used in filesystem paths.
- **Per-Account State**: Wakeup caches are isolated per agy-authenticated account when the account can be determined.

## Installation / Build

This tool is built with Rust. Follow the steps below to compile and run it:

1. Clone this repository.
2. Run the following command to build the release binary:

```bash
cargo build --release
```

The compiled binary is generated at `target/release/agy-usage`.

> **Note:** Quota display and wakeup invoke the Antigravity `agy` CLI, so `agy` must be installed and authenticated, and available in your `PATH`. Authentication is handled entirely by the `agy` CLI. `status` and `accounts` report the account currently authenticated in the `agy` CLI.

## Usage

Global options:
- `--json`: shorthand for `quota --json`.
- `--debug`: shows `agy` stderr for `quota` and `wakeup`.

### 1. Account Status

Show the account currently authenticated in the `agy` CLI. These commands read the account from the OS keyring used by agy, falling back to agy's token file when the keyring is unavailable. They do not make API calls.

```bash
# Show authentication status and account
agy-usage status

# List the authenticated account
agy-usage accounts list

# Show the authenticated account
agy-usage accounts current
```

Account switching and removal are managed by the `agy` CLI, so `agy-usage` no longer provides `accounts switch` or `accounts remove`.

### 2. Checking Quota and Usage

Display the user quota summary (remaining usage and reset times) by running `agy -p "/usage" --output-format json` and rendering its output. The `agy` CLI must be installed and authenticated.

**Tip:** When you pipe or redirect the output (e.g., `agy-usage quota > result.md`), it automatically switches to Markdown format and disables terminal colors. Errors and diagnostics are written to `stderr` to keep your output clean for scripts.

```bash
# Display quota
agy-usage quota

# Output the raw JSON produced by `agy /usage`
agy-usage quota --json
```

The account used for quota display is the one authenticated in the `agy` CLI.

### 3. Wakeup Trigger

Trigger models to start quota limitation timers or wake them up. By default, the tool runs `agy models` to discover available models and automatically selects the most cost-effective model across all active quota categories (standard Gemini 3.x Flash/Pro models, dedicated Gemini 4 Argon models, and Claude/GPT models) from the returned list. Actual triggering is performed by `agy -p`, so no direct API calls are made.

To prevent excessive triggers and save your quota, `wakeup` implements a 5-minute cache per model and agy account. If triggered multiple times within 5 minutes, subsequent calls will be skipped.

> **Note:** `wakeup` runs a full `agy` agent turn for each selected model, so it consumes more tokens than a minimal API request. Use `--models` to limit the number of targets.

```bash
# Trigger wakeup for dynamically resolved cheapest models
agy-usage wakeup

# Specify models manually (e.g. Gemini 4 Argon Low, Claude Sonnet 5.5 Low) and use a custom prompt
agy-usage wakeup --models gemini-4-argon-low,claude-sonnet-5-5-low --prompt "Hello"
```

### 4. Periodic Execution (Scheduled Wakeup)

To keep models warmed up or to reset/trigger quota limits continuously, you can run the `wakeup` command periodically. Make sure the `agy` CLI is authenticated as the user who will run the scheduled job, that it is available in `PATH`, and that the process can access the same credential store used by agy (the OS keyring or agy's token file).

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

### 5. Self-Update

To update the CLI tool to the latest version directly from GitHub releases:

```bash
agy-usage self-update
```

This command automatically checks for new releases on GitHub, downloads the correct binary for your operating system and architecture, and replaces the running executable in-place.

### 6. Version

Show the current version of the CLI tool.

```bash
agy-usage version
```

You can also use the standard flags `-V` or `--version` to display the version:

```bash
agy-usage -V
agy-usage --version
```

## Changelog

Please see [CHANGELOG.md](CHANGELOG.md) for the release history.

## License

[MIT License](LICENSE)

Copyright (c) 2026 Yoshihiro OKUMURA <orrisroot@gmail.com>
