//! Async Rust client library for the browser-controller system.
//!
//! This crate provides a high-level API for connecting to a running
//! `browser-controller-mediator` instance and controlling the browser.
//!
//! # Quick start
//!
//! ```no_run
//! use std::time::Duration;
//! use browser_controller_client::{Client, discover_instances, select_instance, socket_dir};
//!
//! # async fn example() -> Result<(), browser_controller_client::Error> {
//! let instances = discover_instances().await?;
//! let dir = socket_dir()?;
//! let instance = select_instance(&instances, None, &dir)?;
//! let client = Client::new(instance.socket_path.clone(), Duration::from_secs(30));
//! let info = client.browser_info().await?;
//! println!("Connected to {} {}", info.browser_name, info.browser_version);
//! # Ok(())
//! # }
//! ```

// Re-export all types for convenience so users don't need to depend on
// browser-controller-types directly.
pub use browser_controller_types::*;

mod client;
mod discovery;
mod error;
mod event_stream;
mod manifest;
mod matchers;
mod rdp;
mod url_util;

pub use client::{Client, OpenTabParams};
pub use discovery::{DiscoveredInstance, discover_instances, select_instance, socket_dir};
pub use error::Error;
pub use event_stream::EventStream;
pub use manifest::{BrowserFamily, BrowserTarget, InstallManifestResult, install_manifest};
pub use matchers::{MultipleMatchBehavior, TabMatcher, WindowMatcher, match_tabs, match_windows};
pub use rdp::load_temporary_extension;
pub use url_util::strip_url_credentials;
