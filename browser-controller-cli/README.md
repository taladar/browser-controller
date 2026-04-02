# browser-controller-cli

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
| `windows list` | List all open windows with their tabs |
| `windows open` | Open a new browser window |
| `windows close <id>` | Close a window |
| `windows set-title-prefix <id> <prefix>` | Set a window title prefix |
| `windows remove-title-prefix <id>` | Remove a window title prefix |
| `tabs list <window-id>` | List tabs in a window |
| `tabs open <window-id>` | Open a new tab |
| `tabs close <tab-id>` | Close a tab |
| `tabs navigate <tab-id> <url>` | Navigate a tab to a URL |
| `install-manifest` | Install the native messaging host manifest |
| `generate-manpage` | Generate a man page |
| `generate-shell-completion` | Generate shell completion scripts |

Use `--output json` for machine-readable output on any command.

## Development

To load the extension directly from source, see the instructions in the
[repository](https://github.com/taladar/browser-controller).

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or
[MIT license](../LICENSE-MIT) at your option.
