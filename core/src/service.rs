//! `Backend`: the owned handle the app crate talks to.
//!
//! S6 (architecture.md §1.3): a struct owning the env-mapped `CommandRunner`,
//! the `Distrobox`, and the task registry — i.e. today's `AppState` + the
//! `api.rs` functions — with methods returning `Result<T, CoreError>` instead
//! of `anyhow`.
//!
//! T3 wires only the **read-only** domains (`containers`, `images`, `apps`,
//! `stats` — §7 row 2). The task-spawning call sites (`create`, `upgrade`,
//! `clone_from`, …) stay on `api.rs`'s copies until T5's `spawn_task` lands;
//! `Backend` does not grow `tokio::spawn` here (§0.2: spawning entry points
//! must be awaited from a `Task`/`Subscription` future, which do not exist yet).

use crate::backends::Distrobox;
use crate::backends::distrobox::command::default_cmd_factory;
use crate::env::{EnvGuard, detect_host};
use crate::error::{CoreError, CoreFailure};
use crate::fakers::CommandRunner;
use crate::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use crate::task_runtime::TaskRegistry;
use std::sync::Arc;

/// Shared backend handle. Cheap to clone (`Arc` inside); the app holds one and
/// clones it into `Task` futures.
///
/// No `Debug`: neither `CommandRunner` nor `Distrobox` implements it, and a
/// hand-written impl would leak command history into logs.
#[derive(Clone)]
pub struct Backend {
    inner: Arc<BackendInner>,
}

struct BackendInner {
    /// Kept so the env-mapped runner has a named owner; all execution goes
    /// through `distrobox`, which holds its own clone.
    #[allow(dead_code)]
    runner: CommandRunner,
    distrobox: Distrobox,
    tasks: TaskRegistry,
    env: EnvGuard,
}

impl Backend {
    /// Build from an already env-mapped runner (tests inject a
    /// `NullCommandRunner`-backed runner here; the app passes `detect_host`'s).
    pub fn new(runner: CommandRunner, env: EnvGuard) -> Self {
        let distrobox = Distrobox::new(runner.clone(), default_cmd_factory());
        Self {
            inner: Arc::new(BackendInner {
                runner,
                distrobox,
                tasks: crate::task_runtime::new_registry(),
                env,
            }),
        }
    }

    /// Production constructor: detect the environment once, map the runner,
    /// and keep the guard for the blocked-screen / setup-guidance UI.
    pub fn new_host() -> Self {
        let (runner, env) = detect_host(&CommandRunner::new_real());
        Self::new(runner, env)
    }

    pub fn env(&self) -> &EnvGuard {
        &self.inner.env
    }

    pub fn tasks(&self) -> &TaskRegistry {
        &self.inner.tasks
    }

    /// Today's `is_distrobox_installed`, without the throwaway runner +
    /// `Distrobox` per call: the probe ran once in `detect`.
    pub fn is_distrobox_installed(&self) -> bool {
        self.inner.env.distrobox_installed
    }

    /// Today's `get_distrobox_version`, typed.
    pub async fn distrobox_version(&self) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .version()
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `get_containers` (`list()`), typed.
    pub async fn containers(&self) -> Result<Vec<ContainerInfo>, CoreFailure> {
        self.guard_ok()?;
        let map = self
            .inner
            .distrobox
            .list()
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)?;
        Ok(map.into_values().collect())
    }

    /// Today's `list_available_images`, typed.
    pub async fn images(&self) -> Result<Vec<String>, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .list_images()
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `list_container_apps`, mapped to the `AppInfo` DTO.
    pub async fn container_apps(&self, container: &str) -> Result<Vec<AppInfo>, CoreFailure> {
        self.guard_ok()?;
        let apps = self
            .inner
            .distrobox
            .list_apps(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)?;
        Ok(apps
            .into_iter()
            .map(|app| AppInfo {
                name: app.entry.name,
                exec: app.entry.exec,
                icon: app.entry.icon,
                desktop_file_path: app.desktop_file_path,
                is_exported: app.exported,
            })
            .collect())
    }

    /// Today's `list_exported_binaries`, mapped to the `ExportedBinary` DTO.
    pub async fn exported_binaries(
        &self,
        container: &str,
    ) -> Result<Vec<ExportedBinary>, CoreFailure> {
        self.guard_ok()?;
        let binaries = self
            .inner
            .distrobox
            .get_exported_binaries(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)?;
        Ok(binaries
            .into_iter()
            .map(|b| ExportedBinary {
                name: b.name,
                source_path: b.source_path,
                exported_path: b.exported_path,
            })
            .collect())
    }

    /// Today's `get_container_stats`, typed.
    pub async fn container_stats(&self, container: &str) -> Result<ContainerStats, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .get_container_stats(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    fn guard_ok(&self) -> Result<(), CoreFailure> {
        match self.inner.env.mode {
            crate::env::EnvMode::Blocked => Err(CoreFailure::from(CoreError::BlockedEnvironment)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{EnvGuard, EnvMode};
    use crate::fakers::NullCommandRunnerBuilder;

    fn backend_with(runner: CommandRunner) -> Backend {
        Backend::new(
            runner,
            EnvGuard {
                mode: EnvMode::Native,
                message: None,
                distrobox_installed: true,
            },
        )
    }

    #[test]
    fn blocked_env_refuses_reads() {
        let runner = NullCommandRunnerBuilder::new().build();
        let backend = Backend::new(
            runner,
            EnvGuard {
                mode: EnvMode::Blocked,
                message: Some("blocked".into()),
                distrobox_installed: false,
            },
        );
        assert!(!backend.is_distrobox_installed());
        let err = smol::block_on(backend.containers()).unwrap_err();
        assert!(matches!(&*err.0, CoreError::BlockedEnvironment), "{err:?}");
    }

    #[test]
    fn installed_flag_comes_from_guard() {
        let runner = NullCommandRunnerBuilder::new().build();
        assert!(backend_with(runner).is_distrobox_installed());
    }
}
