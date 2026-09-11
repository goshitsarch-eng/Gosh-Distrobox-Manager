pub mod api;
pub mod app_state;
pub mod backends;
pub mod env;
pub mod error;
pub mod fakers;
mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
pub mod models;
pub mod service;
pub mod task_runtime;

/// Re-exports for the `app` crate: the env guard, the UI-boundary error, and
/// the backend handle. (The §2.3 draft shows them under `models`; they live in
/// their own modules — `env`, `error`, `service` — and are re-exported here so
/// the app has one import root.)
pub use env::{BLOCKED_MESSAGE, EnvGuard, EnvMode};
pub use error::{CoreError, CoreFailure};
pub use service::Backend;
