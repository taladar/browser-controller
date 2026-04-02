# browser-controller-mediator

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
