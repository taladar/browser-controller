# browser-controller-mediator

[![Crates.io Version browser-controller-mediator](https://img.shields.io/crates/v/browser-controller-mediator)](https://crates.io/crates/browser-controller-mediator)
[![lib.rs Version browser-controller-mediator](https://img.shields.io/crates/v/browser-controller-mediator?label=lib.rs)](https://lib.rs/crates/browser-controller-mediator)
![docs.rs browser-controller-mediator - none for binary crate](https://img.shields.io/badge/docs-none_for_binary_crate-lightgrey)
[![Dependency status browser-controller-mediator](https://deps.rs/crate/browser-controller-mediator/latest/status.svg)](https://deps.rs/crate/browser-controller-mediator/)

Native messaging host for the browser-controller project. The browser launches
this binary when the browser-controller extension opens a native messaging
connection. It listens on a local IPC socket (Unix Domain Socket on
Linux/macOS, Named Pipe on Windows) and relays commands from the
[browser-controller CLI](https://crates.io/crates/browser-controller-cli) to
the browser and back.

## Usage

This binary is not invoked directly by users. Install it and register it with
your browser using the CLI:

```sh
cargo install browser-controller-cli
cargo install browser-controller-mediator

# Firefox
browser-controller install-manifest --browser firefox

# Chrome / Chromium
browser-controller install-manifest --browser chrome --extension-id <id>
```

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or
[MIT license](../LICENSE-MIT) at your option.
