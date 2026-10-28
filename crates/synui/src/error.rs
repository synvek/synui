//! SynUI error types.

use thiserror::Error;

/// Defined failures returned by the stable v0.2 App-context API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AppError {
    /// A handle was supplied to a context other than the one that created it.
    #[error("resource belongs to a different App context")]
    ContextMismatch,
    /// The App context is draining or has already been destroyed.
    #[error("App context has been destroyed")]
    ContextDestroyed,
    /// The resource slot or generation recorded by a handle is no longer live.
    #[error("resource handle is stale")]
    StaleHandle,
    /// A direct UI operation was attempted from a non-UI thread.
    #[error("operation must run on the App UI thread")]
    ThreadAffinity,
}

#[derive(Debug, Error)]
pub enum SynUiError {
    #[error("event loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("render error: {0}")]
    Render(String),
}
