//! CLI command helpers for integration tests.
//!
//! Thin wrappers around [`browser_controller_client`] for use in test harnesses.

use std::path::Path;
use std::time::Duration;

use browser_controller_client::Client;
use browser_controller_types::BrowserEvent;

/// Default command timeout for integration tests.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Error type for CLI command operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error from the browser-controller client library.
    #[error("{0}")]
    Client(#[from] browser_controller_client::Error),
}

/// Create a [`Client`] for the given socket path with the default test timeout.
#[must_use]
pub fn client(socket_path: &Path) -> Client {
    Client::new(socket_path.to_owned(), TEST_TIMEOUT)
}

/// An active event subscription connection.
///
/// After sending `SubscribeEvents`, the mediator streams [`BrowserEvent`] objects
/// as newline-delimited JSON. Use [`EventSubscription::next_event`] to read them.
#[expect(
    missing_debug_implementations,
    reason = "EventStream does not implement Debug"
)]
pub struct EventSubscription {
    /// The underlying event stream from the client crate.
    inner: browser_controller_client::EventStream,
}

impl EventSubscription {
    /// Open a new event subscription to the mediator.
    ///
    /// Sends `SubscribeEvents` and returns a handle for reading events.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket connection or command send fails.
    pub async fn open(socket_path: &Path) -> Result<Self, Error> {
        let client = Client::new(socket_path.to_owned(), TEST_TIMEOUT);
        let inner = client.subscribe_events().await?;
        Ok(Self { inner })
    }

    /// Read the next event from the subscription.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or parsing the event fails.
    pub async fn next_event(&mut self) -> Result<BrowserEvent, Error> {
        self.inner
            .next_event()
            .await
            .map_err(Error::Client)
            .and_then(|opt| {
                opt.ok_or_else(|| {
                    Error::Client(browser_controller_client::Error::CommandFailed(
                        "event stream closed".to_owned(),
                    ))
                })
            })
    }
}
