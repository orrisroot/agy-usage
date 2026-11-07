# Changelog

All notable changes to this project are documented below, grouped by release version.

## [0.1.11] - 2026-11-07

### Added
- Added an `agy` CLI wrapper for quota display using `agy -p "/usage" --output-format json`.
- Added structured parsing of agy usage groups and buckets, with a fallback to the tab-separated `response` text.
- Added Gemini 4 Argon support and updated model scoring for Gemini 3.x and Claude 5.5 models, including `extra-low` and `tiered` tier adjustments.
- Added OS keyring support for reading the agy-authenticated account.

### Changed
- `quota` now renders agy `/usage` output. Markdown fallback and raw JSON output are preserved; plan type and prompt credits are no longer displayed.
- `wakeup` now discovers models via `agy models`, selects only models returned by it, and triggers them with `agy -p`. Hardcoded fallback models and retired-model detection were removed.
- `wakeup` now runs a full agy agent turn per model, which increases token consumption.
- `wakeup` caches triggers per model and agy account for 5 minutes and exits nonzero when any trigger fails.
- `status` and `accounts list/current` now read the agy account from the OS keyring, falling back to agy's token file when keyring access is unavailable.
- Quota bucket labels now omit redundant group-name prefixes and fall back to the original label if stripping would empty it.
- Simplified storage and configuration helpers after removing token-based authentication.

### Removed
- Removed the `login` and `logout` commands, the OAuth flow, and the Cloud Code API client.
- Removed the `--account` and `--force` options from the global and quota commands, and `--account` / `--keep-system-prompt` from wakeup.
- Removed `accounts switch` and `accounts remove`.
- Removed hardcoded retired-model filtering and hardcoded wakeup fallback models.
- Removed direct dependencies (`reqwest`, `open`, `url`, `rand`) that were only used by the API client and OAuth flow.

### Fixed
- `wakeup` and `quota` now surface agy failures with nonzero exit codes.
- Avoided empty quota bucket labels when a bucket name matches its group name.

## [0.1.10] - 2026-08-13
- Trimmed trailing `" Remaining"` suffix from quota bucket display names in table output to reduce column width.
- Added unit test verifying quota bucket display name trimming logic.
- Updated dependency version constraints in `Cargo.toml` (`tokio` to `1.53`, `open` to `5.4`).

## [0.1.9] - 2026-07-25
- Modularized internal API client architecture by splitting monolithic `google_api` module into specialized `src/api/` submodules (`client`, `types`, `sanitizer`).
- Extracted `StorageProvider` trait and `FileStorage` implementation with path key sanitization to standardize and decouple cache and token storage operations.
- Added HTML escaping to OAuth loopback callback server HTTP responses to protect against parameter injection vulnerabilities.
- Fixed model extraction in `wakeup` command to traverse all model groups and sort types returned by `fetchAvailableModels`, preventing Gemini models in secondary groups from being skipped.
- Updated default fallback Gemini model to `gemini-3.6-flash-low` and refined model cost scoring (`get_model_cost_score`) with version-based preference prioritizing Gemini 3.6 models.
- Added automatic filtering of deprecated model IDs (`deprecatedModelIds`) during model resolution and removed legacy `extra-low` cost tier matching.

## [0.1.8] - 2026-07-03
- Added dynamic model selection for the `wakeup` command.
- Integrated the internal `/v1internal:fetchAvailableModels` API endpoint in `ApiClient` to retrieve active agent models.
- Implemented a local cache (5-minute TTL) for available models to minimize API query frequency.
- Added a heuristic scoring logic (`get_model_cost_score`) based on model ID patterns to automatically select and trigger the most cost-effective model in each category (Gemini and Other).

## [0.1.7] - 2026-06-27
- Refactored API client architecture to improve decoupling and persistence reliability (atomic configuration/token writes and abstract `TokenStorage` trait).
- Added `version` command and standard flags (`-V`, `--version`).

## [0.1.6] - 2026-06-22
- Formatted exhausted and unlimited statuses inside the progress bar.
- Rounded remaining percentage to the nearest integer.
- Rounded up remaining minutes to avoid off-by-one display issues.

## [0.1.5] - 2026-06-22
- Implemented caching for quota details and added a `--force` flag to bypass the cache.

## [0.1.4] - 2026-06-22
- Added automatic markdown fallback formatting and color support.
- Modernized the quota status terminal UI output format and added plan details.
- Implemented caching for API requests and removed legacy model quota listing.

## [0.1.3] - 2026-06-21
- Upgraded `self_update` dependency to `0.44`.
- Fixed the API client to use a consistent `CLOUDCODE_BASE_URL`.

## [0.1.2] - 2026-06-20
- Switched the API base URL to the daily internal endpoint.
- Added proper onboarding and project ID resolution workflows.
- Consolidated request execution and redacted sensitive credentials in logs.
- Encapsulated API logic and token management in `ApiClient`.

## [0.1.1] - 2026-06-20
- Extracted quota formatting helpers into separate modules.
- Integrated the `retrieveUserQuotaSummary` API endpoint.

## [0.1.0] - 2026-06-19
- Initial release with Google OAuth login, multi-account switching, real-time quota tracking, and a wakeup trigger command.
