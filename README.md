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

Control the windows and tabs of a running web browser (Firefox, Chrome,
Chromium, Brave, Edge, Librewolf, Waterfox) from the command line.

## How it works

Three components work together:

1. **browser-controller-mediator** — a native messaging host binary that the
   browser launches when the extension connects. It listens on a local IPC
   socket (Unix Domain Socket on Linux/macOS, Named Pipe on Windows).
2. **browser extension** — loaded into the browser; opens the native messaging
   connection to the mediator.
3. **browser-controller CLI** — connects to the mediator's IPC socket and
   sends commands or streams events.

## Installation

Install the CLI and mediator binaries:

```sh
cargo install browser-controller-cli
cargo install browser-controller-mediator
```

Install the browser extension from your browser's add-on store (search for
"browser-controller"). Then register the mediator with your browser:

```sh
# Firefox / Librewolf / Waterfox
browser-controller install-manifest --browser firefox

# Chrome / Chromium / Brave / Edge
# (requires the 32-character extension ID shown on chrome://extensions)
browser-controller install-manifest --browser chrome --extension-id <id>
```

Restart the browser after installing the manifest.

## Usage

```sh
# List running browser instances
browser-controller instances

# List all open windows
browser-controller windows list

# Open a new tab in window 1 at a URL
browser-controller tabs open 1 --url https://example.com

# Stream browser events as newline-delimited JSON
browser-controller event-stream

# Machine-readable JSON output for any command
browser-controller --output json windows list
```

See `browser-controller --help` or the
[browser-controller-cli README](browser-controller-cli/README.md) for the
full command reference.

## Security

The mediator's IPC socket is created in the user's runtime directory
(`$XDG_RUNTIME_DIR` on Linux, which is typically mode 0700). Any process
running as the same user can connect to the socket and issue commands to the
browser. There is no additional authentication layer beyond the filesystem
permissions on the socket directory.

This is intentional — adding authentication would require persistent
configuration state in the extension, complicating setup for a tool whose
threat model is already scoped to same-user local access. Moreover, any
process that can connect to the socket could also read configuration files
for the CLI or client library that would contain the authentication
credentials, so socket-level authentication would not meaningfully raise
the bar. If you need stronger isolation, ensure that untrusted processes
do not run under your user account.

## Development

### Loading the extension from source

#### Firefox

Load via `about:debugging` → This Firefox → Load Temporary Add-on, selecting
`extension/manifest.json`.

Alternatively, if Firefox has remote debugging enabled, you can load or reload
the extension from the command line:

```sh
browser-controller load-extension --path ./extension --port 6000
```

This connects to Firefox's Remote Debugging Protocol and installs the
extension as a temporary add-on. If the extension is already loaded, it
reloads it. This is useful during development to quickly test changes without
navigating `about:debugging` manually each time.

To enable the debugger server, set these preferences in `about:config`:

- `devtools.debugger.remote-enabled` = `true`
- `devtools.chrome.enabled` = `true`
- `devtools.debugger.prompt-connection` = `false`

Then either restart Firefox with `firefox --start-debugger-server 6000`
(the port must be space-separated, not `=`-separated), or without
restarting press Shift+F2 to open the Developer Toolbar and type `listen`.

**Note:** This command is for development and testing of unreleased extension
versions only. Temporary extensions are removed when Firefox restarts. For
production use, install the extension through `about:addons` or the Mozilla
Add-ons website.

#### Chrome / Chromium

Enable Developer mode in `chrome://extensions`, then click Load unpacked and
select the `extension/` directory. Note the 32-character extension ID shown
on the extensions page — you will need it when running `install-manifest`.

### Expected warning about unrecognized manifest key

Both browsers will log a warning similar to:

> Warning: Reading manifest: Warning processing background.scripts:
> An unexpected property was found in the WebExtension manifest.

or

> Warning: 'background.service_worker' is not allowed for specified
> manifest version.

This is expected and harmless. The manifest intentionally declares both
`background.service_worker` (used by Chrome) and `background.scripts` (used
by Firefox). Each browser uses the key it supports and ignores the other.
There is no cross-browser way to avoid the warning.
