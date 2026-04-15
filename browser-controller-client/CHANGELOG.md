# Changelog

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
