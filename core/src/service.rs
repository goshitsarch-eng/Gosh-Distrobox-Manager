//! `Backend`: the owned handle the app crate talks to.
//!
//! S6 (architecture.md §1.3): a struct owning the env-mapped `CommandRunner`,
//! the `Distrobox`, and the task registry — i.e. today's `AppState` + the
//! `api.rs` functions — with methods returning `Result<T, CoreError>` instead
//! of `anyhow`.
//!
//! T3 wired the **read-only** domains (`containers`, `images`, `apps`,
//! `stats` — §7 row 2). T5 adds the eight task-spawning operations over
//! `tasks::spawn_task` (§4.1); the `api.rs` copies stay until S7 (T14).
//! Every `spawn_*` method requires an entered tokio runtime — await from a
//! `Task`/`Subscription` future, never synchronously (§0.2).

use crate::backends::Distrobox;
use crate::backends::distrobox::command::default_cmd_factory;
use crate::env::{EnvGuard, detect_host};
use crate::error::{CoreError, CoreFailure};
use crate::fakers::CommandRunner;
use crate::models::{AppInfo, ContainerInfo, ContainerStats, CreateArgs, ExportedBinary};
use crate::tasks::{SpawnTask, TaskId, TaskRegistry, spawn_task};
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
                tasks: TaskRegistry::new(),
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

    // ---- short mutations (T6): stop / remove / stop-all ---------------------
    //
    // Non-task operations: `distrobox stop/rm` return promptly (no child to
    // stream), so they stay synchronous `Result<String, CoreFailure>` — the
    // same shape as the read-only domains, and the same shape `api.rs`'s
    // `remove_container`/`stop_container`/`stop_all_containers` had. The UI
    // refreshes the container list after each (T6 `ActionFinished` arm).

    /// Today's `remove_container`, typed.
    pub async fn remove_container(&self, name: &str) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .remove(name)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `stop_container`, typed.
    pub async fn stop_container(&self, name: &str) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .stop(name)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `stop_all_containers`, typed.
    pub async fn stop_all_containers(&self) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .stop_all()
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    // ---- task-spawning operations (T5) ---------------------------------------
    //
    // Each is one `spawn_task` call: the label, start-error prefix, and
    // success/failure lines are the only per-op content (the eight `api.rs`
    // bodies differed in exactly these four substitutions). Sync `Distrobox`
    // call sites wrap in `|| async { … }`; async ones pass their future.
    // All require an entered tokio runtime — i.e. await from a `Task` future
    // (§0.2); `Backend::run` in the app is the only caller.

    /// Today's `create_container`, typed (`TaskId` instead of `String`).
    pub async fn create_container(&self, args: CreateArgs) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Create {}", args.name);
        let distrobox = self.inner.distrobox.clone();
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error starting task: ".to_string(),
                success_message: "Task completed successfully".to_string(),
                failure_message: "Task failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                async move { distrobox.create(args).await }
            },
        )
        .await
    }

    /// Today's `upgrade_container`, typed.
    pub async fn upgrade_container(&self, name: &str) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Upgrade {name}");
        let distrobox = self.inner.distrobox.clone();
        let name = name.to_string();
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error starting upgrade: ".to_string(),
                success_message: "Upgrade completed successfully".to_string(),
                failure_message: "Upgrade failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let name = name.clone();
                async move { distrobox.upgrade(&name) }
            },
        )
        .await
    }

    /// Today's `clone_container`, typed.
    pub async fn clone_container(
        &self,
        source_name: &str,
        args: CreateArgs,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Clone to {}", args.name);
        let distrobox = self.inner.distrobox.clone();
        let source_name = source_name.to_string();
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error starting clone: ".to_string(),
                success_message: "Clone completed successfully".to_string(),
                failure_message: "Clone failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                async move { distrobox.clone_from(&source_name, args).await }
            },
        )
        .await
    }

    /// Today's `detect_package_manager`, typed (B1 — not `String`).
    pub async fn detect_package_manager(
        &self,
        container: &str,
    ) -> Result<crate::models::PackageManager, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .detect_package_manager(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `list_installed_packages`, typed.
    pub async fn installed_packages(
        &self,
        container: &str,
    ) -> Result<Vec<crate::models::PackageInfo>, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .list_installed_packages(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `search_packages`, typed.
    pub async fn search_packages(
        &self,
        container: &str,
        query: &str,
    ) -> Result<Vec<crate::models::PackageInfo>, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .search_packages(container, query)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Today's `install_package`, typed.
    pub async fn install_package(
        &self,
        container: &str,
        package: &str,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Install {package} in {container}");
        let distrobox = self.inner.distrobox.clone();
        let (container, package) = (container.to_string(), package.to_string());
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error: ".to_string(),
                success_message: "Package installed successfully".to_string(),
                failure_message: "Package installation failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let (container, package) = (container.clone(), package.clone());
                async move { distrobox.install_package(&container, &package) }
            },
        )
        .await
    }

    /// Today's `remove_package`, typed.
    pub async fn remove_package(
        &self,
        container: &str,
        package: &str,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Remove {package} from {container}");
        let distrobox = self.inner.distrobox.clone();
        let (container, package) = (container.to_string(), package.to_string());
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error: ".to_string(),
                success_message: "Package removed successfully".to_string(),
                failure_message: "Package removal failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let (container, package) = (container.clone(), package.clone());
                async move { distrobox.remove_package(&container, &package) }
            },
        )
        .await
    }

    /// Today's `restore_from_snapshot`, typed.
    pub async fn restore_from_snapshot(
        &self,
        snapshot: &str,
        new_container: &str,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Restore {snapshot} to {new_container}");
        let distrobox = self.inner.distrobox.clone();
        let (snapshot, new_container) = (snapshot.to_string(), new_container.to_string());
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error: ".to_string(),
                success_message: "Restore completed successfully".to_string(),
                failure_message: "Restore failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let (snapshot, new_container) = (snapshot.clone(), new_container.clone());
                async move {
                    distrobox
                        .restore_from_snapshot(&snapshot, &new_container)
                        .await
                }
            },
        )
        .await
    }

    /// Today's `export_container_to_file`, typed.
    pub async fn export_container(
        &self,
        container: &str,
        output_path: &str,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Export {container} to {output_path}");
        let distrobox = self.inner.distrobox.clone();
        let (container, output_path) = (container.to_string(), output_path.to_string());
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error: ".to_string(),
                success_message: "Export completed successfully".to_string(),
                failure_message: "Export failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let (container, output_path) = (container.clone(), output_path.clone());
                async move { distrobox.export_container(&container, &output_path) }
            },
        )
        .await
    }

    /// Today's `import_container_from_file`, typed.
    pub async fn import_container(
        &self,
        archive_path: &str,
        image_name: &str,
    ) -> Result<TaskId, CoreFailure> {
        self.guard_ok()?;
        let label = format!("Import {archive_path} as {image_name}");
        let distrobox = self.inner.distrobox.clone();
        let (archive_path, image_name) = (archive_path.to_string(), image_name.to_string());
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label,
                start_error_prefix: "Error: ".to_string(),
                success_message: "Import completed successfully".to_string(),
                failure_message: "Import failed".to_string(),
            },
            move || {
                let distrobox = distrobox.clone();
                let (archive_path, image_name) = (archive_path.clone(), image_name.clone());
                async move { distrobox.import_container(&archive_path, &image_name) }
            },
        )
        .await
    }

    /// Today's `cancel_task`, typed (`TaskId`) with real child kill (B6).
    pub fn cancel_task(&self, id: TaskId) -> bool {
        self.inner.tasks.cancel(id)
    }

    /// Today's `get_active_tasks`, typed.
    pub fn active_tasks(&self) -> Vec<TaskId> {
        self.inner.tasks.active_ids()
    }

    /// Today's `is_task_running`, typed.
    pub fn is_task_running(&self, id: TaskId) -> bool {
        self.inner.tasks.is_running(id)
    }

    /// TTL sweep evidence for the app's `TaskMsg::Expired` mirror-drop.
    pub fn sweep_expired(&self) -> Vec<TaskId> {
        self.inner.tasks.sweep_expired()
    }

    /// Test seam: spawn `start` through the full runner path (event channel,
    /// cancel select, completion marking) without a `Distrobox` call.
    /// `#[cfg(test)]` — production callers use the eight operations above.
    /// Covered by `service::tests::spawn_test_child_*` below (the seam
    /// itself would otherwise be dead code the suite never executes).
    #[cfg(test)]
    pub async fn spawn_test_child<F, Fut>(
        &self,
        label: &str,
        start: F,
    ) -> Result<TaskId, CoreFailure>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<
                Output = Result<
                    Box<dyn crate::fakers::Child + Send>,
                    crate::backends::distrobox::Error,
                >,
            > + Send,
    {
        spawn_task(
            &self.inner.tasks,
            SpawnTask {
                label: label.to_string(),
                start_error_prefix: "Error: ".to_string(),
                success_message: "done".to_string(),
                failure_message: "failed".to_string(),
            },
            start,
        )
        .await
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

    /// The `spawn_test_child` seam executes the full runner path (spawn →
    /// events → finish) without a `Distrobox` call. `#[tokio::test]`: the
    /// seam internally `tokio::spawn`s (§0.2).
    #[tokio::test]
    async fn spawn_test_child_completes_successfully() {
        use crate::tasks::TaskEvent;
        let runner = NullCommandRunnerBuilder::new().build();
        let backend = backend_with(runner);
        let id = backend
            .spawn_test_child("probe", || async {
                // `upgrade` on a null-runner `Distrobox` returns an
                // instantly-ready stub child (exit 0, empty streams) — the
                // fastest path through `run_child_task`.
                let distrobox = crate::backends::Distrobox::new(
                    crate::backends::Distrobox::null_command_runner(&[]),
                    crate::backends::distrobox::command::default_cmd_factory(),
                );
                distrobox.upgrade("probe-box")
            })
            .await
            .expect("spawn succeeds");
        assert!(backend.is_task_running(id));
        let rx = backend.tasks().subscribe(id).expect("subscribes");
        let mut finished = false;
        while let Ok(ev) = rx.recv().await {
            if matches!(ev, TaskEvent::Finished { success: true }) {
                finished = true;
                break;
            }
        }
        assert!(finished, "terminal event arrives");
        assert!(!backend.is_task_running(id));
    }

    #[tokio::test]
    async fn spawn_test_child_start_error_is_typed() {
        let runner = NullCommandRunnerBuilder::new().build();
        let backend = backend_with(runner);
        let id = backend
            .spawn_test_child("probe", || async {
                Err::<Box<dyn crate::fakers::Child + Send>, crate::backends::distrobox::Error>(
                    crate::backends::distrobox::Error::ParseOutput("instant".to_string()),
                )
            })
            .await
            .expect("spawn returns an id even when start fails");
        // Start failure still records completion (unsuccessful) so the task
        // never reads "running" forever.
        tokio::task::yield_now().await;
        assert!(!backend.is_task_running(id));
    }
}
