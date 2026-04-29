# Changelog

## 0.2.1 - 2026-04-29 08:55:03Z

### 🐛 Bug Fixes

- *(cross-platform)* Make workspace build on Windows and macOS

## 0.2.0 - 2026-04-28 22:20:42Z

### 🚀 Features

- Implement browser-controller workspace with mediator, CLI, and extension
- *(cross-platform)* Add macOS and Windows support
- *(tabs)* Add command to sort tabs by domain
- *(cli)* Add window and tab matcher system for flexible selection
- *(tests)* Add WebDriver BiDi integration test framework
- Add Password newtype with zeroize, fix CLI argument bugs
- Pre-release hardening — browser compat, robustness, Chrome keepalive, docs
- Add window/tab properties, tab groups, Chrome compat, and test infra

### 🐛 Bug Fixes

- Add version to browser-controller-types workspace dependency
- *(cliff)* Add cliff.toml for browser-controller-client

### 🚜 Refactor

- *(workspace)* Rename crate dirs to full names, add per-crate release tooling
- Extract browser-controller-client library crate
- Add newtype IDs, typed Client API, and mandatory timeout
- Move event-stream filtering to mediator, narrow client re-exports
- Rework matchers with value enums, derive_builder, and extension trait
- Split errors into module-specific types with CommandError<E> wrapper

### ⚙️ Miscellaneous Tasks

- *(release)* Release new version
- *(release)* Release new version
- *(release)* Release new version
- *(release)* Release new version
- *(release)* Release new version
- *(dependencies)* Upgrade dependencies

All notable changes to this project will be documented in this file.

## 0.1.5 - 2026-04-15

### Added

- Initial release of the browser-controller-client library crate
- `Client` struct for async communication with the mediator
- `EventStream` for subscribing to browser events
- Instance discovery (`socket_dir`, `discover_instances`, `select_instance`)
- Window and tab matchers
  (`WindowMatcher`, `TabMatcher`, `match_windows`, `match_tabs`)
- `Client::resolve_windows` and `Client::resolve_tabs`
  for matcher-based resolution
- Native messaging manifest installation (`install_manifest`, `BrowserTarget`)
- Firefox RDP extension loading (`load_temporary_extension`)
- URL utility (`strip_url_credentials`)
- Re-exports all types from `browser-controller-types`
