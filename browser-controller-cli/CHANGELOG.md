# Changelog

## 0.2.1 - 2026-04-29T08:55:03Z

No changes in this crate; see other browser-controller components.

## 0.2.0 - 2026-04-28 22:20:42Z

### 🚀 Features

- *(tabs)* Add command to sort tabs by domain
- *(tabs)* Add --background flag to open tab without activating it
- *(tabs)* Add tab ID to tab summaries in windows list output
- *(windows)* Add last-focused window indicator to windows list
- *(cli)* Add window and tab matcher system for flexible selection
- *(open)* Add --title-prefix, --if-title-prefix-does-not-exist,
  --if-url-does-not-exist
- *(tests)* Add WebDriver BiDi integration test framework
- *(auth)* Replace strip_credentials with onAuthRequired credential injection
- *(tests)* Add CLI tests for all window matching parameters
- *(cli)* Add load-extension command for development workflows
- *(downloads)* Add download management commands and event streaming
- *(tabs)* Add reload command with optional cache bypass
- *(containers)* Add Firefox container support
- *(containers)* Add container_name to tab listings and matching
- Add Password newtype with zeroize, fix CLI argument bugs
- Pre-release hardening — browser compat, robustness, Chrome keepalive, docs
- Add window/tab properties, tab groups, Chrome compat, and test infra

### 🐛 Bug Fixes

- *(auth)* Fix credential injection race conditions and add CLI timeout
- *(cli)* Treat zero matches as success for close commands

### 🚜 Refactor

- Extract browser-controller-client library crate
- Add newtype IDs, typed Client API, and mandatory timeout
- Move event-stream filtering to mediator, narrow client re-exports
- Rework matchers with value enums, derive_builder, and extension trait
- Split errors into module-specific types with CommandError<E> wrapper

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Upgrade dependencies

## 0.1.5 - 2026-04-02T16:47:29Z

No changes in this crate; see other browser-controller components.

## 0.1.4 - 2026-04-02 16:09:02Z

### 📚 Documentation

- Add correct per-crate badges to workspace and crate READMEs

## 0.1.3 - 2026-04-02 15:48:19Z

### 🐛 Bug Fixes

- Add version to browser-controller-types workspace dependency

### ⚙️ Miscellaneous Tasks

- Add workspace Cargo.toml to cliff include_paths for all crates

## 0.1.2 - 2026-04-02 13:06:04Z

### ⚙️ Miscellaneous Tasks

- Enable publishing and improve keywords/categories for all crates
- Fix exclude lists for crates.io publishing
- *(types)* Sort Cargo.toml fields via cargo-sort
- Add LICENSE files, per-crate READMEs, fix mediator description

## 0.1.1 - 2026-04-02 12:23:32Z

### 🚀 Features

- Implement browser-controller workspace with mediator, CLI, and extension
- *(cross-platform)* Add macOS and Windows support

### 🐛 Bug Fixes

- *(mediator)* Keep socket guard alive for full duration of run()
- *(cli)* Set metadata.deb.name to match cargo package name

### 🚜 Refactor

- *(workspace)* Rename crate dirs to full names, add per-crate release tooling

## 0.1.0

Initial Release
