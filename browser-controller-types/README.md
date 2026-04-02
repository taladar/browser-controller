# browser-controller-types

[![Crates.io Version browser-controller-types](https://img.shields.io/crates/v/browser-controller-types)](https://crates.io/crates/browser-controller-types)
[![lib.rs Version browser-controller-types](https://img.shields.io/crates/v/browser-controller-types?label=lib.rs)](https://lib.rs/crates/browser-controller-types)
[![docs.rs browser-controller-types](https://img.shields.io/docsrs/browser-controller-types)](https://docs.rs/browser-controller-types/latest/browser_controller_types)
[![Dependency status browser-controller-types](https://deps.rs/crate/browser-controller-types/latest/status.svg)](https://deps.rs/crate/browser-controller-types/)

Shared protocol types for the browser-controller project. Defines the
serializable request and response types exchanged between the
[browser-controller CLI](https://crates.io/crates/browser-controller-cli) and
the
[browser-controller-mediator](https://crates.io/crates/browser-controller-mediator)
over their IPC channel.

This crate is useful if you want to implement a custom client or server that
speaks the browser-controller protocol.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or
[MIT license](../LICENSE-MIT) at your option.
