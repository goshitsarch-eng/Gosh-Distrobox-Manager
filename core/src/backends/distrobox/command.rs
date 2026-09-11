use crate::fakers::Command;
use std::sync::Arc;

/// A factory that returns a base `Command` value for running `distrobox`.
/// Invariant: the factory must return the base program only (e.g. `Command::new("distrobox")`).
/// Any Flatpak wrapping should be applied by `CommandRunner` implementations.
pub type CmdFactory = Arc<dyn Fn() -> Command + Send + Sync + 'static>;

pub fn default_cmd_factory() -> CmdFactory {
    Arc::new(|| Command::new("distrobox"))
}
