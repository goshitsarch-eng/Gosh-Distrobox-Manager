pub mod api;
pub mod app_state;
pub mod backends;
pub mod config;
pub mod env;
pub mod error;
pub mod fakers;
mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
pub mod models;
pub mod service;
pub mod task_runtime;
pub mod tasks;

pub use config::{
    AppConfig, CONFIG_ID, CONFIG_VERSION, LegacyImport, apply_legacy, import_legacy,
    resolve_terminal,
};
/// Re-exports for the `app` crate: the env guard, the UI-boundary error, the
/// backend handle, and the task surface (`TaskId`, `TaskRegistry`,
/// `TaskEvent`, `spawn_task`). (The §2.3 draft shows them under `models`;
/// they live in their own modules and are re-exported here so the app has
/// one import root.)
pub use env::{BLOCKED_MESSAGE, EnvGuard, EnvMode};
pub use error::{CoreError, CoreFailure};
pub use service::Backend;
pub use tasks::{
    COMPLETED_TASK_TTL, MAX_TASK_OUTPUT_LINES, SpawnTask, TaskEvent, TaskId, TaskRegistry,
    spawn_task,
};
