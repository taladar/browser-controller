# browser-controller-cli

[![Crates.io Version browser-controller-cli](https://img.shields.io/crates/v/browser-controller-cli)](https://crates.io/crates/browser-controller-cli)
[![lib.rs Version browser-controller-cli](https://img.shields.io/crates/v/browser-controller-cli?label=lib.rs)](https://lib.rs/crates/browser-controller-cli)
![docs.rs browser-controller-cli - none for binary crate](https://img.shields.io/badge/docs-none_for_binary_crate-lightgrey)
[![Dependency status browser-controller-cli](https://deps.rs/crate/browser-controller-cli/latest/status.svg)](https://deps.rs/crate/browser-controller-cli/)

Command-line tool to control the windows and tabs of a running web browser
(Firefox, Chrome, Chromium, Brave, Edge, Librewolf, Waterfox) via a native
messaging host.

## How it works

Three components work together:

1. **browser-controller-mediator** — a native messaging host binary that the
   browser launches when the extension connects. It listens on a local IPC
   socket (Unix Domain Socket on Linux/macOS, Named Pipe on Windows).
2. **browser extension** — loaded into the browser; opens the native messaging
   connection to the mediator.
3. **browser-controller** (this crate) — a CLI that connects to the mediator's
   IPC socket and sends commands or streams events.

## Installation

```sh
cargo install browser-controller-cli
cargo install browser-controller-mediator
```

Install the browser extension from your browser's add-on store (search for
"browser-controller"), then install the native messaging manifest so the
browser can find the mediator binary:

```sh
# Firefox
browser-controller install-manifest --browser firefox

# Chrome / Chromium (requires the extension ID shown on chrome://extensions)
browser-controller install-manifest --browser chrome --extension-id <id>
```

## Commands

| Command | Description |
|---|---|
| `instances` | List all running mediator instances |
| `event-stream` | Stream browser events as newline-delimited JSON |
| **Windows** | |
| `windows list` | List all open windows with their tabs |
| `windows open` | Open a new browser window |
| `windows close` | Close one or more windows |
| `windows set-title-prefix` | Set a window title prefix (Firefox-only) |
| `windows remove-title-prefix` | Remove a window title prefix (Firefox-only) |
| **Tabs** | |
| `tabs list` | List all tabs in one or more windows |
| `tabs open` | Open a new tab in a window |
| `tabs activate` | Activate (switch to) a tab |
| `tabs navigate` | Navigate a tab to a new URL |
| `tabs reopen-in-container` | Reopen tab(s) in a different container (Firefox-only) |
| `tabs reload` | Reload one or more tabs |
| `tabs close` | Close one or more tabs |
| `tabs pin` | Pin one or more tabs |
| `tabs unpin` | Unpin one or more tabs |
| `tabs toggle-reader-mode` | Toggle Reader Mode for a tab (Firefox-only) |
| `tabs discard` | Discard (unload) one or more tabs from memory |
| `tabs warmup` | Warm up discarded tabs without activating (Firefox-only) |
| `tabs mute` | Mute one or more tabs |
| `tabs unmute` | Unmute one or more tabs |
| `tabs move` | Move a tab to a new position |
| `tabs back` | Navigate backward in session history |
| `tabs forward` | Navigate forward in session history |
| `tabs sort` | Sort tabs by domain order |
| **Downloads** | |
| `downloads list` | List downloads, optionally filtered by state |
| `downloads start` | Start a new download |
| `downloads cancel` | Cancel an active download |
| `downloads pause` | Pause an active download |
| `downloads resume` | Resume a paused download |
| `downloads retry` | Retry an interrupted download |
| `downloads erase` | Remove a download from browser history |
| `downloads clear` | Clear all downloads from history |
| **Containers** | |
| `containers list` | List all Firefox containers (Firefox-only) |
| **Tab Groups** | |
| `tab-groups list` | List all tab groups, optionally filtered by window (Chrome-only) |
| `tab-groups get` | Get a single tab group by ID (Chrome-only) |
| `tab-groups update` | Update a tab group's title, color, or collapsed state (Chrome-only) |
| `tab-groups move` | Move a tab group to a new position (Chrome-only) |
| `tab-groups group` | Add tabs to a group, creating a new group if needed (Chrome-only) |
| `tab-groups ungroup` | Remove tabs from their tab groups (Chrome-only) |
| **Setup & Tools** | |
| `install-manifest` | Install the native messaging host manifest |
| `load-extension` | Load/reload a temporary extension via Firefox RDP (development only) |
| `generate-manpage` | Generate a man page |
| `generate-shell-completion` | Generate shell completion scripts |

Use `--output json` for machine-readable output on any command.

## Development

To load the extension directly from source, see the instructions in the
[repository](https://github.com/taladar/browser-controller).

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or
[MIT license](../LICENSE-MIT) at your option.
