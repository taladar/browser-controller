# Changelog

## 0.1.5 - 2026-04-02 16:47:30Z

### 🚀 Features

- *(extension)* Declare no data collection per Firefox built-in consent

### 🐛 Bug Fixes

- *(release)* Add missing commits field to context.json before git-cliff reads
  it
- *(release)* Manually generate empty Debian changelog section when no commits
- *(release)* Manually generate empty CHANGELOG.md section when no commits
- *(release)* Correctly rewrite CHANGELOG.md header when inserting empty section

## 0.1.4 - 2026-04-02 16:09:03Z

### 📚 Documentation

- Add correct per-crate badges to workspace and crate READMEs
- *(readme)* Add user-facing installation and usage instructions

## 0.1.3 - 2026-04-02 15:48:20Z

### 🐛 Bug Fixes

- Add version to browser-controller-types workspace dependency
- *(ci)* Upgrade checkout to v6, fix archive name for multi-binary upload

### ⚙️ Miscellaneous Tasks

- Add workspace Cargo.toml to cliff include_paths for all crates

## 0.1.2 - 2026-04-02 13:06:05Z

### 🚀 Features

- Implement browser-controller workspace with mediator, CLI, and extension
- *(types/mediator/cli/extension)* Add browser vendor and profile ID to instance
  output
- *(types/mediator/cli/extension)* Add event-stream subcommand
- *(types/mediator/cli/extension)* Add tabs pin/unpin and diagnostic logging
- *(types/cli/extension)* Add --strip-credentials to tabs open
- *(types/cli/extension)* Add TabStatusChanged event and fix Tab result display
- *(types/cli/extension)* Add tabs warmup subcommand
- *(types/cli/extension)* Add tabs mute/unmute subcommands
- *(cli/mediator/extension)* Add Chrome/Chromium support
- *(cross-platform)* Add macOS and Windows support
- *(extension)* Add package-extension.sh to build .xpi and .zip archives
- *(release)* Update extension manifest version and package extension
- *(release)* Add workspace release tag and update GitHub release workflow

### 🐛 Bug Fixes

- *(extension)* Declare both service_worker and scripts for cross-browser MV3
- *(cross-platform)* Eliminate unused-variable warnings on Windows cross-build
- *(mediator)* Keep socket guard alive for full duration of run()
- *(release)* Bump workspace version once instead of once per crate
- *(cli)* Set metadata.deb.name to match cargo package name

### 🚜 Refactor

- *(workspace)* Rename crate dirs to full names, add per-crate release tooling

### 📚 Documentation

- *(readme)* Add extension loading instructions and manifest warning note

### ⚙️ Miscellaneous Tasks

- *(init)* Initial commit with cargo-generate output
- *(release)* Release new version
- Enable publishing and improve keywords/categories for all crates
- Fix exclude lists for crates.io publishing
- *(types)* Sort Cargo.toml fields via cargo-sort
- Add LICENSE files, per-crate READMEs, fix mediator description

## 0.1.0

Initial Release
