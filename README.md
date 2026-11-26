# agy-usage

`agy-usage` is a CLI tool for checking Antigravity model quotas and triggering model wakeup turns through the Antigravity `agy` CLI.

## Features

- **Quota Status Display**: Queries `agy -p "/usage" --output-format json` and renders quota groups, remaining percentages with progress bars, and local reset countdowns (`N/A` for unset timers). Outputs Markdown automatically when piped or redirected, and supports raw JSON output via `--json`.
- **Dynamic Wakeup Trigger**: Queries `agy -p "/usage"` for active quota groups and `agy models` for available models, selects the most cost-effective model for each group (with group-name and heuristic fallbacks), and triggers each selected model via `agy -p` while reporting duration and token usage.
- **Wakeup Cooldown Cache**: Caches recent triggers per model and `agy` account for 5 minutes using atomic file writes to avoid redundant triggers.
- **Account Inspection**: Displays the account currently authenticated in the `agy` CLI.
- **Self-Update**: Updates the `agy-usage` binary in-place from GitHub Releases.

## Prerequisites

- The Antigravity `agy` CLI must be installed, authenticated, and available in your `PATH`.

## Installation / Build

Prebuilt binaries are available on [GitHub Releases](https://github.com/orrisroot/agy-usage/releases).

To build from source (requires Rust 1.88 or newer, Edition 2024):

1. Clone this repository.
2. Build the release binary:

```bash
cargo build --release
```

The compiled binary is written to `target/release/agy-usage`.

## Usage

Running `agy-usage` without a subcommand defaults to `quota`.

Global options:
- `--json`: Output raw JSON (shorthand for `quota --json`).
- `--debug`: Print `agy` `stderr` output for `quota` and `wakeup`.

### 1. Checking Quota and Usage

Display quota groups, remaining percentages, and reset times:

```bash
# Display formatted quota status (or Markdown when piped/redirected)
agy-usage

# Equivalent explicit subcommand
agy-usage quota

# Output the raw JSON returned by `agy -p "/usage" --output-format json`
agy-usage quota --json
```

When stdout is not a terminal (e.g., `agy-usage quota > result.md`), output switches to plain Markdown tables without ANSI colors. Errors and diagnostics are written to `stderr`. Unset reset timestamps are displayed as `N/A`.

### 2. Wakeup Trigger

Start quota reset timers or wake up models across active quota groups:

```bash
# Automatically select and trigger the cheapest model per quota group
agy-usage wakeup

# Trigger specific models with a custom prompt
agy-usage wakeup --models gemini-3.8-flash-low,claude-sonnet-5-5-low --prompt "Hello"
```

Options:
- `--models <MODELS>`: Comma-separated list of model IDs to trigger instead of automatic selection.
- `--prompt <PROMPT>`: Custom prompt sent to `agy` (defaults to `"."`).
- `--debug`: Print `agy` `stderr` output.

When `--models` is omitted, `wakeup` queries `agy -p "/usage"` for active quota groups and `agy models` for available models, matches models to their quota groups (falling back to group-name matching and Gemini vs. Claude/GPT heuristic grouping if needed), and selects the lowest-cost model in each group. Each selected model is invoked with `agy --disable-slash-commands -p <prompt> --model <model> --output-format json`, reporting execution duration and token usage (`Input`, `Output` including thinking tokens, `Total`, and `Cache Read` when reported).

Triggers are cached for 5 minutes per model and `agy` account; models triggered within the cooldown window are skipped. `wakeup` exits with a non-zero status if any trigger fails. Because each target runs a full `agy` agent turn, use `--models` to restrict the target list when needed.

### 3. Account Status

Inspect the account authenticated in the `agy` CLI. These commands read credentials from the OS keyring used by `agy` (service `gemini`, user `antigravity`), falling back to `~/.gemini/antigravity-cli/antigravity-oauth-token` if the keyring is unavailable:

```bash
# Show authentication status, account email, and auth method
agy-usage status

# List the authenticated account
agy-usage accounts list

# Show the authenticated account
agy-usage accounts current
```

### 4. Periodic Execution (Scheduled Wakeup)

Run `agy-usage wakeup` on a schedule to keep quota timers active. Ensure that the scheduled execution environment has `agy` in `PATH` and access to `agy`'s credential store (OS keyring or token file).

#### Linux (cron)

Open your user crontab:

```bash
crontab -e
```

Add a job to run every hour (use the absolute path to your binary and log file):

```cron
0 * * * * /absolute/path/to/agy-usage wakeup >> /absolute/path/to/agy-usage-wakeup.log 2>&1
```

#### Linux (systemd User Timer)

1. Create `~/.config/systemd/user/agy-wakeup.service`:

```ini
[Unit]
Description=Trigger agy-usage wakeup
After=network.target

[Service]
Type=oneshot
ExecStart=/absolute/path/to/agy-usage wakeup
```

2. Create `~/.config/systemd/user/agy-wakeup.timer`:

```ini
[Unit]
Description=Run agy-usage wakeup periodically

[Timer]
OnCalendar=hourly
Persistent=true

[Install]
WantedBy=timers.target
```

3. Reload systemd and enable the timer:

```bash
systemctl --user daemon-reload
systemctl --user enable --now agy-wakeup.timer
```

4. Check timer status or service logs:

```bash
systemctl --user status agy-wakeup.timer
journalctl --user -u agy-wakeup.service
```

#### macOS (launchd)

1. Create `~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist`:

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
    <integer>3600</integer>
    <key>StandardOutPath</key>
    <string>/tmp/agy-usage-wakeup.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/agy-usage-wakeup.stderr.log</string>
</dict>
</plist>
```

2. Load the launch agent:

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist
```

3. Unload the launch agent when no longer needed:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.user.agy-usage.wakeup.plist
```

#### Windows (Task Scheduler)

Run PowerShell to register an hourly scheduled task:

```powershell
$action = New-ScheduledTaskAction -Execute "C:\path\to\agy-usage.exe" -Argument "wakeup"
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Hours 1)
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName "AgyUsageWakeup" -Action $action -Trigger $trigger -Settings $settings -Description "Trigger agy-usage wakeup"
```

Alternatively, configure the task in the **Task Scheduler** GUI:
1. Create a Basic Task named `AgyUsageWakeup`.
2. Set the Trigger to **Daily**, and under Advanced Settings set **Repeat task every** to **1 hour**.
3. Set the Action to **Start a program**, select `agy-usage.exe`, and set the arguments to `wakeup`.

### 5. Self-Update

Update `agy-usage` to the latest GitHub release in-place:

```bash
agy-usage self-update
```

This command checks GitHub Releases, downloads the archive matching your OS and architecture, and replaces the running executable.

### 6. Version

Display the installed version:

```bash
agy-usage version
agy-usage -V
agy-usage --version
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the release history.

## License

[MIT License](LICENSE)

Copyright (c) 2026 Yoshihiro OKUMURA <orrisroot@gmail.com>
