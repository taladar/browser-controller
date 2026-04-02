# browser-controller

[![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/taladar/browser-controller/github-release.yaml)](https://github.com/taladar/browser-controller/actions/workflows/github-release.yaml)

browser-controller-cli:
[![Crates.io Version browser-controller-cli](https://img.shields.io/crates/v/browser-controller-cli)](https://crates.io/crates/browser-controller-cli)
[![lib.rs Version browser-controller-cli](https://img.shields.io/crates/v/browser-controller-cli?label=lib.rs)](https://lib.rs/crates/browser-controller-cli)
![docs.rs browser-controller-cli - none for binary crate](https://img.shields.io/badge/docs-none_for_binary_crate-lightgrey)
[![Dependency status browser-controller-cli](https://deps.rs/crate/browser-controller-cli/latest/status.svg)](https://deps.rs/crate/browser-controller-cli/)

browser-controller-mediator:
[![Crates.io Version browser-controller-mediator](https://img.shields.io/crates/v/browser-controller-mediator)](https://crates.io/crates/browser-controller-mediator)
[![lib.rs Version browser-controller-mediator](https://img.shields.io/crates/v/browser-controller-mediator?label=lib.rs)](https://lib.rs/crates/browser-controller-mediator)
![docs.rs browser-controller-mediator - none for binary crate](https://img.shields.io/badge/docs-none_for_binary_crate-lightgrey)
[![Dependency status browser-controller-mediator](https://deps.rs/crate/browser-controller-mediator/latest/status.svg)](https://deps.rs/crate/browser-controller-mediator/)

browser-controller-types:
[![Crates.io Version browser-controller-types](https://img.shields.io/crates/v/browser-controller-types)](https://crates.io/crates/browser-controller-types)
[![lib.rs Version browser-controller-types](https://img.shields.io/crates/v/browser-controller-types?label=lib.rs)](https://lib.rs/crates/browser-controller-types)
[![docs.rs browser-controller-types](https://img.shields.io/docsrs/browser-controller-types)](https://docs.rs/browser-controller-types/latest/browser_controller_types)
[![Dependency status browser-controller-types](https://deps.rs/crate/browser-controller-types/latest/status.svg)](https://deps.rs/crate/browser-controller-types/)
Allows controlling the windows and tabs of a web browser via a CLI emitting JSON

## Loading the extension

### Firefox

Load via `about:debugging` → This Firefox → Load Temporary Add-on, selecting
`extension/manifest.json`.

### Chrome / Chromium

Enable Developer mode in `chrome://extensions`, then click Load unpacked and
select the `extension/` directory. Note the 32-character extension ID shown on
the extensions page — you will need it when running `install-manifest`.

### Expected warning about unrecognized manifest key

Both browsers will log a warning similar to:

> Warning: Reading manifest: Warning processing background.scripts:
> An unexpected property was found in the WebExtension manifest.

or

> Warning: 'background.service_worker' is not allowed for specified
> manifest version.

This is expected and harmless. The manifest intentionally declares both
`background.service_worker` (used by Chrome) and `background.scripts` (used by
Firefox). Each browser uses the key it supports and ignores the other. There is
no cross-browser way to avoid the warning.
