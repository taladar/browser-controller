//! Async Rust client library for the browser-controller system.
//!
//! This crate provides a high-level API for connecting to a running
//! `browser-controller-mediator` instance and controlling the browser.
//!
//! # Quick start
//!
//! ```no_run
//! use std::time::Duration;
//! use browser_controller_client::{MatchWith, discover_instances, InstanceMatcher};
//!
//! # async fn example() -> Result<(), browser_controller_client::Error> {
//! let instances = discover_instances().await?;
//! let matched = instances.match_with(&InstanceMatcher::default())?;
//! let instance = matched.first().expect("at least one instance");
//! let client = instance.client(Duration::from_secs(30));
//! let info = client.browser_info().await?;
//! println!("Connected to {} {}", info.browser_name, info.browser_version);
//! # Ok(())
//! # }
//! ```

// Re-export data types that appear in the public API so users don't need to
// depend on browser-controller-types directly.  Protocol-internal types
// (CliCommand, CliRequest, CliResponse, CliOutcome, ExtensionHello,
// ExtensionMessage) are deliberately excluded.
pub use browser_controller_types::{
    BrowserEvent, BrowserInfo, CliResult, ContainerInfo, CookieStoreId, DownloadId, DownloadItem,
    DownloadState, FilenameConflictAction, InvalidCookieStoreId, InvalidDownloadId,
    InvalidTabGroupId, InvalidTabId, InvalidWindowId, Password, TabDetails, TabGroupColor,
    TabGroupId, TabGroupInfo, TabId, TabStatus, TabSummary, WindowId, WindowState, WindowSummary,
    WindowType,
};

mod client;
mod discovery;
mod error;
mod event_stream;
mod manifest;
mod matchers;
mod rdp;

pub use client::{
    Client, OpenTabParams, OpenTabParamsBuilder, OpenTabParamsBuilderError, SendCommandError,
};
pub use discovery::{DiscoveredInstance, DiscoveryError, discover_instances, socket_dir};
pub use error::{CommandError, Error};
pub use event_stream::{EventStream, EventStreamError};
pub use manifest::{BrowserFamily, InstallManifestResult, ManifestError, install_manifest};
pub use matchers::{
    BooleanCondition, BrowserKind, InstanceMatcher, InstanceMatcherBuilder,
    InstanceMatcherBuilderError, MatchError, MatchWith, MultipleMatchBehavior, TabMatcher,
    TabMatcherBuilder, TabMatcherBuilderError, WindowMatcher, WindowMatcherBuilder,
    WindowMatcherBuilderError,
};
pub use rdp::{RdpError, load_temporary_extension};
