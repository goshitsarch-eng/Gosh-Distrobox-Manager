//! The typed error that crosses the core → app boundary.
//!
//! Today `api.rs` flattens all 35 backend call sites into `anyhow`
//! (`map_err(|e| anyhow::anyhow!(e))`), which is exactly the information the UI
//! needs and cannot get: "distrobox not installed" (offer setup guidance) vs
//! "podman socket permission denied" (offer a diagnostic) are indistinguishable
//! as strings (architecture.md §4.3). `CoreError` preserves the kind.
//!
//! `anyhow` stays as a dependency but is confined to core internals;
//! `CoreError` is what crosses into `app`, enforced by the type system because
//! every `Backend` method returns `Result<T, CoreError>`.

use crate::backends::distrobox::Error as DistroboxError;
use std::io;
use std::sync::Arc;

/// Typed backend failure for the UI boundary.
///
/// Not `Clone` (`anyhow::Error` / `io::Error` payloads cannot be): messages
/// carry failures as [`CoreFailure`] (`Arc`-wrapped), which is cheap to clone
/// as `Application::Message: Clone` requires.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A distrobox/podman/docker command ran and failed. Keeps `stderr` and
    /// `exit_code` — the Flutter UI showed only a string, and the most common
    /// real failure (`podman: permission denied` on a rootless socket) is
    /// indistinguishable from `distrobox: no such container` without them.
    #[error("distrobox command failed ({exit_code:?}): {stderr}")]
    CommandFailed {
        exit_code: Option<i32>,
        command: String,
        stderr: String,
    },

    #[error("could not start {command}: {source}")]
    Spawn {
        command: String,
        #[source]
        source: io::Error,
    },

    #[error("could not parse output: {0}")]
    ParseOutput(String),

    #[error("invalid container name {name:?}: {reason}")]
    InvalidName { name: String, reason: String },

    #[error("running inside a Distrobox container without distrobox-host-exec")]
    BlockedEnvironment,

    #[error("configuration unavailable: {0}")]
    Config(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<DistroboxError> for CoreError {
    fn from(e: DistroboxError) -> Self {
        match e {
            DistroboxError::CommandFailed {
                exit_code,
                command,
                stderr,
            } => CoreError::CommandFailed {
                exit_code,
                command,
                stderr,
            },
            DistroboxError::Spawn { source, command } => CoreError::Spawn { command, source },
            DistroboxError::ParseOutput(msg) => CoreError::ParseOutput(msg),
            DistroboxError::InvalidField(name, reason) => CoreError::InvalidName { name, reason },
            DistroboxError::StdoutRead(source) => CoreError::Spawn {
                command: "<stdout read>".to_string(),
                source,
            },
            DistroboxError::ResolveHostPath(msg) => CoreError::ParseOutput(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_failed_preserves_stderr_and_exit_code() {
        let e = CoreError::from(DistroboxError::CommandFailed {
            exit_code: Some(125),
            command: "podman stats".into(),
            stderr: "permission denied".into(),
        });
        let msg = e.to_string();
        assert!(msg.contains("permission denied"), "{msg}");
        assert!(msg.contains("125"), "{msg}");
    }

    #[test]
    fn anyhow_converts_transparently() {
        let e = CoreError::from(anyhow::anyhow!("boom"));
        assert!(e.to_string().contains("boom"));
    }

    #[test]
    fn question_mark_operator_carries_kind() {
        fn fails() -> Result<String, CoreError> {
            Err(DistroboxError::ParseOutput("bad row".into()))?;
            Ok(String::new())
        }
        assert!(matches!(fails(), Err(CoreError::ParseOutput(_))));
    }
}

/// Shareable backend failure for `Message` payloads.
///
/// `Application::Message` must be `Clone`, but `CoreError` cannot be (its
/// `Other`/`Spawn` variants hold non-cloneable errors). The `Arc` keeps the
/// matchable kind — handlers match on `&*failure`, not on a flattened string.
#[derive(Clone, Debug)]
pub struct CoreFailure(pub Arc<CoreError>);

impl std::fmt::Display for CoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<CoreError> for CoreFailure {
    fn from(e: CoreError) -> Self {
        CoreFailure(Arc::new(e))
    }
}
