// `#[frb(init)]` below expands to code gated on `cfg(frb_expand)`, a cfg name the
// toolchain does not know about. The allow is module-scoped, not item-scoped: the
// lint is raised from inside the `frb` attribute macro's expansion, where an
// `#[allow]` on the annotated item does not reach it. Scoping it to this file keeps
// the allow over exactly the FRB surface, which S7 (T14) deletes whole -- and the
// allow with it.
#![allow(unexpected_cfgs)]

use crate::app_state::AppState;
use crate::fakers::Child;
use crate::frb_generated::StreamSink;
use crate::models::Task;
use flutter_rust_bridge::frb;
use futures::{AsyncRead, AsyncReadExt};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub use crate::backends::ContainerInfo;
pub use crate::backends::ContainerStats;
pub use crate::backends::CreateArgName;
pub use crate::backends::CreateArgs;
pub use crate::backends::PackageInfo;
pub use crate::backends::SnapshotInfo;
pub use crate::backends::Status;
pub use crate::backends::Volume;
pub use crate::backends::VolumeMode;
pub use crate::backends::desktop_file::DesktopEntry;
pub use crate::models::KnownDistro;
pub use crate::models::known_distros::PackageManager;

/// Represents an application that can be exported from a container
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub desktop_file_path: String,
    pub is_exported: bool,
}

/// Represents a binary that has been exported from a container
#[derive(Debug, Clone)]
pub struct ExportedBinary {
    pub name: String,
    pub source_path: String,
    pub exported_path: String,
}

static STATE: LazyLock<AppState> = LazyLock::new(AppState::new);
const MAX_TASK_OUTPUT_LINES: usize = 500;
const COMPLETED_TASK_TTL: Duration = Duration::from_secs(600);

fn push_task_output(task_id: &str, line: String) {
    let mut tasks = STATE.tasks.write().unwrap();
    if let Some(task) = tasks.get_mut(task_id) {
        task.push_output(line.clone(), MAX_TASK_OUTPUT_LINES);
        if let Some(tx) = task.tx.as_ref() {
            let _ = tx.send(line);
        }
    }
}

fn finish_task(task_id: &str, success: bool) {
    let mut tasks = STATE.tasks.write().unwrap();
    if let Some(task) = tasks.get_mut(task_id) {
        task.completed = true;
        task.success = success;
        task.completed_at = Some(Instant::now());
        task.tx.take();
    }

    let now = Instant::now();
    tasks.retain(|_, task| {
        if task.completed {
            match task.completed_at {
                Some(when) => now.duration_since(when) < COMPLETED_TASK_TTL,
                None => true,
            }
        } else {
            true
        }
    });
}

async fn stream_reader_to_task_output(mut reader: impl AsyncRead + Unpin, task_id: String) {
    let mut buf = [0u8; 1024];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let s = String::from_utf8_lossy(&buf[..n]).to_string();
                push_task_output(&task_id, s);
            }
            Err(_) => break,
        }
    }
}

async fn run_child_task(
    task_id: String,
    mut child: Box<dyn Child + Send>,
    success_message: &'static str,
    failure_message: &'static str,
) -> anyhow::Result<()> {
    let stdout = match child.take_stdout() {
        Some(stdout) => stdout,
        None => {
            push_task_output(&task_id, "Error: No stdout".into());
            return Err(anyhow::anyhow!("No stdout"));
        }
    };
    let stderr = match child.take_stderr() {
        Some(stderr) => stderr,
        None => {
            push_task_output(&task_id, "Error: No stderr".into());
            return Err(anyhow::anyhow!("No stderr"));
        }
    };

    let out_task = tokio::spawn(stream_reader_to_task_output(stdout, task_id.clone()));
    let err_task = tokio::spawn(stream_reader_to_task_output(stderr, task_id.clone()));

    let status = child.wait().await?;
    let _ = out_task.await;
    let _ = err_task.await;

    if status.success() {
        push_task_output(&task_id, success_message.to_string());
        Ok(())
    } else {
        push_task_output(&task_id, failure_message.to_string());
        Err(anyhow::anyhow!(failure_message))
    }
}

#[frb(init)]
pub fn init_app() {
    let _ = tracing_subscriber::fmt::try_init();
}

pub async fn get_distrobox_version() -> anyhow::Result<String> {
    STATE
        .distrobox
        .version()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

pub async fn is_distrobox_installed() -> bool {
    // Create a temporary instance to check installation
    // This avoids potentially initializing the heavy global STATE if not needed immediately
    let runner = crate::fakers::CommandRunner::new_real();
    let runner = if std::path::Path::new("/.flatpak-info").exists() {
        runner.map_cmd(crate::backends::flatpak::map_flatpak_spawn_host)
    } else if crate::backends::host_exec::is_distrobox_container()
        && crate::backends::host_exec::has_distrobox_host_exec()
    {
        runner.map_cmd(crate::backends::host_exec::map_distrobox_host_exec)
    } else {
        runner
    };

    let distrobox = crate::backends::Distrobox::new(
        runner,
        crate::backends::distrobox::command::default_cmd_factory(),
    );

    distrobox.version().await.is_ok()
}

pub async fn get_containers() -> anyhow::Result<Vec<ContainerInfo>> {
    let map = STATE
        .distrobox
        .list()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(map.into_values().collect())
}

pub async fn create_container(args: CreateArgs) -> anyhow::Result<String> {
    let name = args.name.to_string();
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let args_clone = args.clone();

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.create(args_clone).await {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Task completed successfully",
                        "Task failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error starting task: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), format!("Create {}", name), handle, tx),
        );
    }

    Ok(task_id)
}

pub fn stream_task_output(task_id: String, sink: StreamSink<String>) -> anyhow::Result<()> {
    let (output, rx) = {
        let tasks = STATE.tasks.read().unwrap();
        if let Some(task) = tasks.get(&task_id) {
            let output = task.output.clone();
            let rx = task.tx.as_ref().map(|tx| tx.subscribe());
            (output, rx)
        } else {
            return Err(anyhow::anyhow!("Task not found"));
        }
    };

    let sink = sink;
    for line in output {
        if sink.add(line).is_err() {
            return Ok(());
        }
    }

    if let Some(mut rx) = rx {
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sink.add(msg).is_err() {
                    break;
                }
            }
        });
    }

    Ok(())
}

// ============================================================================
// Container Management APIs
// ============================================================================

/// Remove/delete a container
pub async fn remove_container(name: String) -> anyhow::Result<String> {
    STATE
        .distrobox
        .remove(&name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Stop a running container
pub async fn stop_container(name: String) -> anyhow::Result<String> {
    STATE
        .distrobox
        .stop(&name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Stop all running containers
pub async fn stop_all_containers() -> anyhow::Result<String> {
    STATE
        .distrobox
        .stop_all()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Upgrade packages in a container (returns task_id for streaming output)
pub async fn upgrade_container(name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let name_clone = name.clone();

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.upgrade(&name_clone) {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Upgrade completed successfully",
                        "Upgrade failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error starting upgrade: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), format!("Upgrade {}", name), handle, tx),
        );
    }

    Ok(task_id)
}

/// Clone a container to create a new one
pub async fn clone_container(source_name: String, args: CreateArgs) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let new_name = args.name.to_string();

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.clone_from(&source_name, args).await {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Clone completed successfully",
                        "Clone failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error starting clone: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(
                task_id.clone(),
                format!("Clone to {}", new_name),
                handle,
                tx,
            ),
        );
    }

    Ok(task_id)
}

/// Get the command to enter a container (for terminal integration)
pub fn get_enter_command(name: String) -> Vec<String> {
    let cmd = STATE.distrobox.enter_cmd(&name);
    let mut args = vec![cmd.program.to_string_lossy().to_string()];
    args.extend(cmd.args.iter().map(|a| a.to_string_lossy().to_string()));
    args
}

// ============================================================================
// Application Export APIs
// ============================================================================

/// List all applications in a container
pub async fn list_container_apps(container_name: String) -> anyhow::Result<Vec<AppInfo>> {
    let apps = STATE
        .distrobox
        .list_apps(&container_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
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

/// Export an application from a container to the host
pub async fn export_app(
    container_name: String,
    desktop_file_path: String,
) -> anyhow::Result<String> {
    STATE
        .distrobox
        .export_app(&container_name, &desktop_file_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Unexport an application from the host
pub async fn unexport_app(
    container_name: String,
    desktop_file_path: String,
) -> anyhow::Result<String> {
    STATE
        .distrobox
        .unexport_app(&container_name, &desktop_file_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// List exported binaries from a container
pub async fn list_exported_binaries(container_name: String) -> anyhow::Result<Vec<ExportedBinary>> {
    let binaries = STATE
        .distrobox
        .get_exported_binaries(&container_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(binaries
        .into_iter()
        .map(|b| ExportedBinary {
            name: b.name,
            source_path: b.source_path,
            exported_path: b.exported_path,
        })
        .collect())
}

/// Export a binary from a container to the host
pub async fn export_binary(container_name: String, binary_path: String) -> anyhow::Result<String> {
    STATE
        .distrobox
        .export_binary(&container_name, &binary_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Unexport a binary from the host
pub async fn unexport_binary(
    container_name: String,
    binary_path: String,
) -> anyhow::Result<String> {
    STATE
        .distrobox
        .unexport_binary(&container_name, &binary_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

// ============================================================================
// Image Management APIs
// ============================================================================

/// List available/compatible images for container creation
pub async fn list_available_images() -> anyhow::Result<Vec<String>> {
    STATE
        .distrobox
        .list_images()
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

// ============================================================================
// Task Management APIs
// ============================================================================

/// Get a list of all active task IDs
pub fn get_active_tasks() -> Vec<String> {
    let tasks = STATE.tasks.read().unwrap();
    tasks.keys().cloned().collect()
}

/// Check if a task is still running
pub fn is_task_running(task_id: String) -> bool {
    let tasks = STATE.tasks.read().unwrap();
    if let Some(task) = tasks.get(&task_id) {
        !task.handle.is_finished() && !task.completed
    } else {
        false
    }
}

/// Cancel/abort a running task (if possible)
pub fn cancel_task(task_id: String) -> bool {
    let should_abort = {
        let tasks = STATE.tasks.read().unwrap();
        if let Some(task) = tasks.get(&task_id) {
            task.handle.abort();
            true
        } else {
            false
        }
    };

    if should_abort {
        push_task_output(&task_id, "Task cancelled".into());
        finish_task(&task_id, false);
        true
    } else {
        false
    }
}

// ============================================================================
// Package Management APIs
// ============================================================================

/// Detect the package manager used in a container (apt, dnf, pacman, etc.)
pub async fn detect_package_manager(container_name: String) -> anyhow::Result<String> {
    STATE
        .distrobox
        .detect_package_manager(&container_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// List installed packages in a container
pub async fn list_installed_packages(container_name: String) -> anyhow::Result<Vec<PackageInfo>> {
    STATE
        .distrobox
        .list_installed_packages(&container_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Search for packages in a container's repositories
pub async fn search_packages(
    container_name: String,
    query: String,
) -> anyhow::Result<Vec<PackageInfo>> {
    STATE
        .distrobox
        .search_packages(&container_name, &query)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Install a package in a container (returns task_id for streaming output)
pub async fn install_package(
    container_name: String,
    package_name: String,
) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Install {} in {}", package_name, container_name);

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.install_package(&container_name, &package_name) {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Package installed successfully",
                        "Package installation failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), task_description, handle, tx),
        );
    }

    Ok(task_id)
}

/// Remove a package from a container (returns task_id for streaming output)
pub async fn remove_package(
    container_name: String,
    package_name: String,
) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Remove {} from {}", package_name, container_name);

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.remove_package(&container_name, &package_name) {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Package removed successfully",
                        "Package removal failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), task_description, handle, tx),
        );
    }

    Ok(task_id)
}

// ============================================================================
// Snapshot/Backup APIs
// ============================================================================

/// Create a snapshot of a container
pub async fn create_snapshot(
    container_name: String,
    snapshot_name: String,
) -> anyhow::Result<String> {
    STATE
        .distrobox
        .create_snapshot(&container_name, &snapshot_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// List available snapshots (optionally filtered by prefix)
pub async fn list_snapshots(filter_prefix: Option<String>) -> anyhow::Result<Vec<SnapshotInfo>> {
    STATE
        .distrobox
        .list_snapshots(filter_prefix.as_deref())
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Delete a snapshot
pub async fn delete_snapshot(snapshot_name_or_id: String) -> anyhow::Result<String> {
    STATE
        .distrobox
        .delete_snapshot(&snapshot_name_or_id)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Restore a container from a snapshot (creates a new container)
pub async fn restore_from_snapshot(
    snapshot_name: String,
    new_container_name: String,
) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Restore {} to {}", snapshot_name, new_container_name);

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox
                .restore_from_snapshot(&snapshot_name, &new_container_name)
                .await
            {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Restore completed successfully",
                        "Restore failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), task_description, handle, tx),
        );
    }

    Ok(task_id)
}

// ============================================================================
// Container Export/Import APIs
// ============================================================================

/// Export a container to a tar archive (returns task_id for streaming output)
pub async fn export_container_to_file(
    container_name: String,
    output_path: String,
) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Export {} to {}", container_name, output_path);

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.export_container(&container_name, &output_path) {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Export completed successfully",
                        "Export failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), task_description, handle, tx),
        );
    }

    Ok(task_id)
}

/// Import a container from a tar archive (returns task_id for streaming output)
pub async fn import_container_from_file(
    archive_path: String,
    image_name: String,
) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);

    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Import {} as {}", archive_path, image_name);

    let task_id_clone = task_id.clone();
    let handle = tokio::spawn(async move {
        let task_id_for_run = task_id_clone.clone();
        let result: anyhow::Result<()> = async {
            match distrobox.import_container(&archive_path, &image_name) {
                Ok(child) => {
                    run_child_task(
                        task_id_for_run.clone(),
                        child,
                        "Import completed successfully",
                        "Import failed",
                    )
                    .await
                }
                Err(e) => {
                    push_task_output(&task_id_for_run, format!("Error: {}", e));
                    Err(anyhow::anyhow!(e))
                }
            }
        }
        .await;

        finish_task(&task_id_clone, result.is_ok());
        result
    });

    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(
            task_id.clone(),
            Task::new(task_id.clone(), task_description, handle, tx),
        );
    }

    Ok(task_id)
}

// ============================================================================
// Resource Monitoring APIs
// ============================================================================

/// Get resource usage statistics for a container
pub async fn get_container_stats(container_name: String) -> anyhow::Result<ContainerStats> {
    STATE
        .distrobox
        .get_container_stats(&container_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}

/// Run an arbitrary command inside a container
pub async fn run_command_in_container(
    container_name: String,
    command: String,
) -> anyhow::Result<String> {
    STATE
        .distrobox
        .run_in_container(&container_name, &command)
        .await
        .map_err(|e| anyhow::anyhow!(e))
}
