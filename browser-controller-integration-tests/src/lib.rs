//! Integration test framework for the browser-controller system.
//!
//! Provides a [`harness::Harness`] that orchestrates the full stack:
//! geckodriver/chromedriver -> browser with extension -> mediator -> CLI commands.
//!
//! Tests communicate with the mediator over Unix Domain Sockets using the same
//! newline-delimited JSON protocol as the CLI, and can optionally verify
//! compositor-level effects (window titles, counts) via niri-ipc.

pub mod bidi;
pub mod browser;
pub mod chrome_for_testing;
pub mod cli;
pub mod driver;
pub mod harness;
pub mod mediator;
pub mod niri;
pub mod profile;
pub mod test_server;

pub use harness::Harness;
