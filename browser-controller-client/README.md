# browser-controller-client

[![Crates.io Version browser-controller-client](https://img.shields.io/crates/v/browser-controller-client)](https://crates.io/crates/browser-controller-client)
[![lib.rs Version browser-controller-client](https://img.shields.io/crates/v/browser-controller-client?label=lib.rs)](https://lib.rs/crates/browser-controller-client)
[![docs.rs browser-controller-client](https://img.shields.io/docsrs/browser-controller-client)](https://docs.rs/browser-controller-client/)
[![Dependency status browser-controller-client](https://deps.rs/crate/browser-controller-client/latest/status.svg)](https://deps.rs/crate/browser-controller-client/)

Async Rust client library for the browser-controller system.

This crate provides a high-level API for connecting to a running
`browser-controller-mediator` instance and controlling the browser
(windows, tabs, downloads, containers, events).

## Quick start

```rust,no_run
use browser_controller_client::{discover_instances, InstanceMatcher, MatchWith};
use std::time::Duration;

# async fn example() -> Result<(), browser_controller_client::Error> {
    let instances = discover_instances().await?;
    let matched = instances.match_with(&InstanceMatcher::default())?;
    let instance = matched.first().expect("at least one instance");
    let client = instance.client(Duration::from_secs(30));
    let info = client.browser_info().await?;
    println!(
        "Connected to {} {}",
        info.browser_name, info.browser_version
    );
#     Ok(())
# }
```

See the [API documentation](https://docs.rs/browser-controller-client/) for
the full list of available operations.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or
[MIT license](../LICENSE-MIT) at your option.
