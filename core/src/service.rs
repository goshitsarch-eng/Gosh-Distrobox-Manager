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

use crate::backends::distrobox::command::default_cmd_factory;
use crate::backends::{ContainerList, Distrobox, ParseIssue};
use crate::env::{EnvGuard, detect_host};
use crate::error::{CoreError, CoreFailure};
use crate::fakers::CommandRunner;
use crate::models::{AppInfo, ContainerStats, CreateArgs, ExportedBinary};
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
    /// The terminal list the UI indexes into, owned HERE rather than
    /// re-derived per call site (T12): a `Vec` per render would let a
    /// custom terminal imported mid-session shift the index under a
    /// picker's own selection.
    terminals: std::sync::Mutex<crate::backends::TerminalRepository>,
}

impl Backend {
    /// Build from an already env-mapped runner (tests inject a
    /// `NullCommandRunner`-backed runner here; the app passes `detect_host`'s).
    pub fn new(runner: CommandRunner, env: EnvGuard) -> Self {
        let distrobox = Distrobox::new(runner.clone(), default_cmd_factory());
        let terminals = crate::backends::TerminalRepository::new(runner.clone());
        Self {
            inner: Arc::new(BackendInner {
                runner,
                distrobox,
                tasks: TaskRegistry::new(),
                env,
                terminals: std::sync::Mutex::new(terminals),
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

    /// Install the persisted/imported `AppConfig::custom_terminals` into the
    /// terminal list (T12). Called whenever the config changes, so the list
    /// the pickers index is always the same list the ids resolve against.
    pub fn set_custom_terminals(&self, customs: Vec<crate::backends::Terminal>) {
        *self.inner.terminals.lock().unwrap() =
            crate::backends::TerminalRepository::with_customs(self.inner.runner.clone(), customs);
    }

    /// The ONE terminal list (built-ins + customs): every picker renders it
    /// and every index resolves against it.
    pub fn terminals(&self) -> Vec<crate::backends::Terminal> {
        self.inner.terminals.lock().unwrap().all_terminals()
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

    /// Today's `get_containers` (`list()`), typed. Returns the B3 wrapper
    /// whole — `skipped` is what `show_skipped_lines` surfaces
    /// (architecture.md §6.4, row B3).
    pub async fn containers(&self) -> Result<ContainerList, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .list()
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
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
        // B3: the per-container peers stay `Vec<T>` at this boundary — the
        // DTO mapping below and `show_skipped_lines` (specified against
        // `ContainerList.skipped`) both want the items only, so the skips are
        // logged here rather than threaded to the app.
        log_skipped(&apps.skipped, "container_apps");
        Ok(apps
            .items
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
        log_skipped(&binaries.skipped, "exported_binaries");
        Ok(binaries
            .items
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

    /// Today's `get_enter_command` (row #69): the `distrobox enter` argv
    /// for display (selectable, monospace) + terminal launch.
    pub fn enter_command(&self, name: &str) -> Vec<String> {
        let cmd = self.inner.distrobox.enter_cmd(name);
        let mut argv = vec![cmd.program.to_string_lossy().to_string()];
        argv.extend(cmd.args.iter().map(|a| a.to_string_lossy().to_string()));
        argv
    }

    /// Env-mapped runner handle (T12 legacy import): probes run host-side
    /// under Flatpak through the same mapping as everything else.
    pub fn command_runner(&self) -> &crate::fakers::CommandRunner {
        &self.inner.runner
    }

    /// Terminal launch (D8): spawn `terminal` attached to `container`
    /// through the env-mapped runner. Fire-and-forget (no output
    /// subscription — the terminal owns its window).
    pub fn launch_terminal(
        &self,
        container: &str,
        terminal: &crate::backends::Terminal,
    ) -> Result<(), CoreFailure> {
        self.guard_ok()?;
        let enter = self.inner.distrobox.enter_cmd(container);
        // Runner = the env-mapped one Distrobox holds (flatpak-spawn/host-exec
        // mapping applies to the terminal too).
        terminal
            .launch(self.inner.distrobox.runner(), &enter)
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// B5 `start` (typed): a true start leaving the container `Up`.
    /// Callers refresh `list()` afterwards — the transition is observed.
    pub async fn start_container(&self, name: &str) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .start(name)
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

    /// App export toggle (typed, short).
    pub async fn export_app(
        &self,
        container: &str,
        desktop_file: &str,
    ) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .export_app(container, desktop_file)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// App unexport (typed, short).
    pub async fn unexport_app(
        &self,
        container: &str,
        desktop_file: &str,
    ) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .unexport_app(container, desktop_file)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Binary export (typed, short).
    pub async fn export_binary(
        &self,
        container: &str,
        binary: &str,
    ) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .export_binary(container, binary)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Open a URL in the host browser (row #169): `xdg-open` through the
    /// env-mapped runner (flatpak-spawn/host-exec mapping applies — a bare
    /// spawn would break sandboxed users, ARCH-Q10/D25).
    ///
    /// Fire-and-forget: `Ok` means the launcher STARTED, not that a browser
    /// opened. A host with no URL handler registers one only once the child
    /// runs (`xdg-open` exits non-zero) and that exit is not observed here —
    /// awaiting it would equally mean blocking the UI on handlers that stay
    /// in the foreground. So the caller's toast means "could not launch a
    /// handler", never "the link opened".
    pub fn open_url(&self, url: &str) -> Result<(), CoreFailure> {
        self.guard_ok()?;
        use crate::fakers::Command;
        let mut cmd = Command::new("xdg-open");
        cmd.arg(url);
        self.inner
            .distrobox
            .runner()
            .spawn(cmd)
            .map(|_| ())
            .map_err(|e| {
                CoreFailure::from(CoreError::Spawn {
                    command: "xdg-open".to_string(),
                    source: e,
                })
            })
    }

    /// Snapshot list (typed).
    pub async fn list_snapshots(&self) -> Result<Vec<crate::models::SnapshotInfo>, CoreFailure> {
        self.guard_ok()?;
        let out = self
            .inner
            .distrobox
            .list_snapshots(None)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)?;
        log_skipped(&out.skipped, "list_snapshots");
        Ok(out.items)
    }

    /// Snapshot create (typed, short — `podman/docker commit` returns promptly).
    pub async fn create_snapshot(
        &self,
        container: &str,
        snapshot: &str,
    ) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .create_snapshot(container, snapshot)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)
    }

    /// Snapshot delete (typed, short).
    pub async fn delete_snapshot(&self, snapshot: &str) -> Result<String, CoreFailure> {
        self.guard_ok()?;
        self.inner
            .distrobox
            .delete_snapshot(snapshot)
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
        let out = self
            .inner
            .distrobox
            .list_installed_packages(container)
            .await
            .map_err(CoreError::from)
            .map_err(CoreFailure::from)?;
        log_skipped(&out.skipped, "installed_packages");
        Ok(out.items)
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

/// B3 at the boundary: the four per-container peers hand back only their
/// items (their callers map to DTOs and `show_skipped_lines` is specified
/// against `ContainerList.skipped`), so a drop that used to be silent is
/// recorded here at least once. The individual `warn!`s live at the parse
/// site; this is the "how many" for the journal.
///
/// "At least once" is exact, not rhetorical: this is the *`Backend`* path.
/// The FRB shim that used to bypass it (`api.rs`, deleted in T14) called
/// `Distrobox` directly and never got here, so it called this itself rather
/// than dropping `skipped` on the floor. With that module gone every caller
/// is now a `Backend` method, and this is the single place the count is
/// recorded — a silent drop would be the very thing B3 exists to remove.
pub fn log_skipped(skipped: &[ParseIssue], source: &str) {
    if !skipped.is_empty() {
        tracing::warn!(
            target: "gosh_distrobox",
            count = skipped.len(),
            source,
            "rows skipped while parsing"
        );
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
