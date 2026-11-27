# Changelog

## 0.1.8 - 2026-11-27 19:28:51Z

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Update webdriverbidi to 0.5

## 0.1.7 - 2026-11-26 20:09:28Z

### 🐛 Bug Fixes

- *(extension)* Keep a tab reopened in another container in its window

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Drop the unused time workspace dependency

## 0.1.6 - 2026-11-26 18:20:56Z

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Update dependencies

## 0.1.5 - 2026-11-21 17:31:33Z

### 🚀 Features

- *(tabs)* Move tabs to another window

## 0.1.4 - 2026-11-21 12:14:58Z

### 🚀 Features

- *(tests)* Give test browsers a distinct app-id

### 🐛 Bug Fixes

- *(dependencies)* Replace unmaintained humantime with fundu

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Update dependencies

## 0.1.3 - 2026-11-16 15:21:33Z

### ⚙️ Miscellaneous Tasks

- *(dependencies)* Update dependencies
- *(dependencies)* Update dependencies
- *(dependencies)* Update dependencies

## 0.1.2 - 2026-06-10 10:43:58Z

### 🎨 Styling

- *(toml)* Apply tombi formatting to deny.toml and nextest.toml

### ⚙️ Miscellaneous Tasks

- *(template)* Sync lint config from cargo-generate template
- *(dependencies)* Update dependencies

## 0.1.1 - 2026-04-29 08:55:03Z

### 🐛 Bug Fixes

- *(cross-platform)* Make workspace build on Windows and macOS

## 0.1.0 - 2026-04-28 22:20:43Z

### 🚀 Features

- Implement browser-controller workspace with mediator, CLI, and extension
- *(cross-platform)* Add macOS and Windows support
- *(tabs)* Add command to sort tabs by domain
- *(cli)* Add window and tab matcher system for flexible selection
- *(tests)* Add WebDriver BiDi integration test framework
- *(tests)* Add Chrome test variants and shared test helpers
- *(tests)* Add tab operations, navigation history, and event tests
- *(tests)* Add end-to-end CLI tests including Sort command
- *(tests)* Add CLI tests for all window matching parameters
- *(auth)* Replace strip_credentials with onAuthRequired credential injection
- *(tests)* Add idempotent open tests and trailing-space title prefix variants
- *(tests)* Add CLI tests for all window matching parameters
- *(downloads)* Add download management commands and event streaming
- *(tabs)* Add reload command with optional cache bypass
- *(tests)* Add comprehensive event and event-stream filter tests
- *(containers)* Add Firefox container support
- *(containers)* Add container_name to tab listings and matching
- Add Password newtype with zeroize, fix CLI argument bugs
- Add window/tab properties, tab groups, Chrome compat, and test infra

### 🐛 Bug Fixes

- Add version to browser-controller-types workspace dependency
- *(auth)* Fix credential injection race conditions and add CLI timeout
- *(cliff)* Add cliff.toml for browser-controller-integration-tests

### 🚜 Refactor

- *(workspace)* Rename crate dirs to full names, add per-crate release tooling
- Extract browser-controller-client library crate
- Add newtype IDs, typed Client API, and mandatory timeout
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

## 0.0.0 - 2026-04-13

### Added

- Initial integration test framework with WebDriver BiDi support
- Test harness orchestrating geckodriver, Firefox, extension, and mediator
- Niri-ipc support for compositor-level window verification
- Phase 1 tests: smoke, windows, tabs
