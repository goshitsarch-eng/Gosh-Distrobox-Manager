use crate::app_state::AppState;
use flutter_rust_bridge::frb;
use std::sync::LazyLock;
use tokio::sync::broadcast;
use futures::{AsyncReadExt};
use crate::models::Task;
use crate::frb_generated::StreamSink;

pub use crate::backends::ContainerInfo;
pub use crate::backends::CreateArgs;
pub use crate::backends::Volume;
pub use crate::backends::VolumeMode;
pub use crate::backends::Status;
pub use crate::models::KnownDistro;
pub use crate::models::known_distros::PackageManager;
pub use crate::backends::CreateArgName;
pub use crate::backends::desktop_file::DesktopEntry;
pub use crate::backends::PackageInfo;
pub use crate::backends::SnapshotInfo;
pub use crate::backends::ContainerStats;

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

static STATE: LazyLock<AppState> = LazyLock::new(|| AppState::new());

#[frb(init)]
pub fn init_app() {
    let _ = tracing_subscriber::fmt::try_init();
}

pub async fn get_distrobox_version() -> anyhow::Result<String> {
    STATE.distrobox.version().await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn is_distrobox_installed() -> bool {
    // Create a temporary instance to check installation
    // This avoids potentially initializing the heavy global STATE if not needed immediately
    let runner = crate::fakers::CommandRunner::new_real();
    let runner = if std::path::Path::new("/.flatpak-info").exists() {
        runner.map_cmd(crate::backends::flatpak::map_flatpak_spawn_host)
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
    let map = STATE.distrobox.list().await.map_err(|e| anyhow::anyhow!(e))?;
    Ok(map.into_values().collect())
}

pub async fn create_container(args: CreateArgs) -> anyhow::Result<String> {
    let name = args.name.to_string();
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let args_clone = args.clone();
    
    let handle = tokio::spawn(async move {
        let res = distrobox.create(args_clone).await;
        match res {
            Ok(mut child) => {
                 let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                 let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                 
                 let tx_out = tx_clone.clone();
                 let tx_err = tx_clone.clone();
                 
                 let out_task = tokio::spawn(async move {
                     let mut buf = [0u8; 1024];
                     loop {
                         match stdout.read(&mut buf).await {
                             Ok(0) => break,
                             Ok(n) => {
                                 let s = String::from_utf8_lossy(&buf[..n]);
                                 let _ = tx_out.send(s.to_string());
                             }
                             Err(_) => break,
                         }
                     }
                 });
                 
                 let err_task = tokio::spawn(async move {
                     let mut buf = [0u8; 1024];
                     loop {
                         match stderr.read(&mut buf).await {
                             Ok(0) => break,
                             Ok(n) => {
                                 let s = String::from_utf8_lossy(&buf[..n]);
                                 let _ = tx_err.send(s.to_string());
                             }
                             Err(_) => break,
                         }
                     }
                 });
                 
                 let status = child.wait().await?;
                 let _ = out_task.await;
                 let _ = err_task.await;
                 
                 if status.success() {
                     let _ = tx_clone.send("Task completed successfully".into());
                     Ok(())
                 } else {
                     let _ = tx_clone.send("Task failed".into());
                     Err(anyhow::anyhow!("Task failed"))
                 }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error starting task: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), format!("Create {}", name), handle, tx));
    }
    
    Ok(task_id)
}

pub fn stream_task_output(task_id: String, sink: StreamSink<String>) -> anyhow::Result<()> {
    let tasks = STATE.tasks.read().unwrap();
    if let Some(task) = tasks.get(&task_id) {
        let mut rx = task.tx.subscribe();
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if sink.add(msg).is_err() {
                    break;
                }
            }
        });
        Ok(())
    } else {
        Err(anyhow::anyhow!("Task not found"))
    }
}

// ============================================================================
// Container Management APIs
// ============================================================================

/// Remove/delete a container
pub async fn remove_container(name: String) -> anyhow::Result<String> {
    STATE.distrobox.remove(&name).await.map_err(|e| anyhow::anyhow!(e))
}

/// Stop a running container
pub async fn stop_container(name: String) -> anyhow::Result<String> {
    STATE.distrobox.stop(&name).await.map_err(|e| anyhow::anyhow!(e))
}

/// Stop all running containers
pub async fn stop_all_containers() -> anyhow::Result<String> {
    STATE.distrobox.stop_all().await.map_err(|e| anyhow::anyhow!(e))
}

/// Upgrade packages in a container (returns task_id for streaming output)
pub async fn upgrade_container(name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let name_clone = name.clone();
    
    let handle = tokio::spawn(async move {
        let res = distrobox.upgrade(&name_clone);
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Upgrade completed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Upgrade failed".into());
                    Err(anyhow::anyhow!("Upgrade failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error starting upgrade: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), format!("Upgrade {}", name), handle, tx));
    }
    
    Ok(task_id)
}

/// Clone a container to create a new one
pub async fn clone_container(source_name: String, args: CreateArgs) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let new_name = args.name.to_string();
    
    let handle = tokio::spawn(async move {
        let res = distrobox.clone_from(&source_name, args).await;
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Clone completed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Clone failed".into());
                    Err(anyhow::anyhow!("Clone failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error starting clone: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), format!("Clone to {}", new_name), handle, tx));
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
    let apps = STATE.distrobox.list_apps(&container_name).await.map_err(|e| anyhow::anyhow!(e))?;
    Ok(apps.into_iter().map(|app| AppInfo {
        name: app.entry.name,
        exec: app.entry.exec,
        icon: app.entry.icon,
        desktop_file_path: app.desktop_file_path,
        is_exported: app.exported,
    }).collect())
}

/// Export an application from a container to the host
pub async fn export_app(container_name: String, desktop_file_path: String) -> anyhow::Result<String> {
    STATE.distrobox.export_app(&container_name, &desktop_file_path).await.map_err(|e| anyhow::anyhow!(e))
}

/// Unexport an application from the host
pub async fn unexport_app(container_name: String, desktop_file_path: String) -> anyhow::Result<String> {
    STATE.distrobox.unexport_app(&container_name, &desktop_file_path).await.map_err(|e| anyhow::anyhow!(e))
}

/// List exported binaries from a container
pub async fn list_exported_binaries(container_name: String) -> anyhow::Result<Vec<ExportedBinary>> {
    let binaries = STATE.distrobox.get_exported_binaries(&container_name).await.map_err(|e| anyhow::anyhow!(e))?;
    Ok(binaries.into_iter().map(|b| ExportedBinary {
        name: b.name,
        source_path: b.source_path,
        exported_path: b.exported_path,
    }).collect())
}

/// Export a binary from a container to the host
pub async fn export_binary(container_name: String, binary_path: String) -> anyhow::Result<String> {
    STATE.distrobox.export_binary(&container_name, &binary_path).await.map_err(|e| anyhow::anyhow!(e))
}

/// Unexport a binary from the host
pub async fn unexport_binary(container_name: String, binary_path: String) -> anyhow::Result<String> {
    STATE.distrobox.unexport_binary(&container_name, &binary_path).await.map_err(|e| anyhow::anyhow!(e))
}

// ============================================================================
// Image Management APIs
// ============================================================================

/// List available/compatible images for container creation
pub async fn list_available_images() -> anyhow::Result<Vec<String>> {
    STATE.distrobox.list_images().await.map_err(|e| anyhow::anyhow!(e))
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
        !task.handle.is_finished()
    } else {
        false
    }
}

/// Cancel/abort a running task (if possible)
pub fn cancel_task(task_id: String) -> bool {
    let tasks = STATE.tasks.read().unwrap();
    if let Some(task) = tasks.get(&task_id) {
        task.handle.abort();
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
    STATE.distrobox.detect_package_manager(&container_name).await.map_err(|e| anyhow::anyhow!(e))
}

/// List installed packages in a container
pub async fn list_installed_packages(container_name: String) -> anyhow::Result<Vec<PackageInfo>> {
    STATE.distrobox.list_installed_packages(&container_name).await.map_err(|e| anyhow::anyhow!(e))
}

/// Search for packages in a container's repositories
pub async fn search_packages(container_name: String, query: String) -> anyhow::Result<Vec<PackageInfo>> {
    STATE.distrobox.search_packages(&container_name, &query).await.map_err(|e| anyhow::anyhow!(e))
}

/// Install a package in a container (returns task_id for streaming output)
pub async fn install_package(container_name: String, package_name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Install {} in {}", package_name, container_name);
    
    let handle = tokio::spawn(async move {
        let res = distrobox.install_package(&container_name, &package_name);
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Package installed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Package installation failed".into());
                    Err(anyhow::anyhow!("Package installation failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), task_description, handle, tx));
    }
    
    Ok(task_id)
}

/// Remove a package from a container (returns task_id for streaming output)
pub async fn remove_package(container_name: String, package_name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Remove {} from {}", package_name, container_name);
    
    let handle = tokio::spawn(async move {
        let res = distrobox.remove_package(&container_name, &package_name);
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Package removed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Package removal failed".into());
                    Err(anyhow::anyhow!("Package removal failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), task_description, handle, tx));
    }
    
    Ok(task_id)
}

// ============================================================================
// Snapshot/Backup APIs
// ============================================================================

/// Create a snapshot of a container
pub async fn create_snapshot(container_name: String, snapshot_name: String) -> anyhow::Result<String> {
    STATE.distrobox.create_snapshot(&container_name, &snapshot_name).await.map_err(|e| anyhow::anyhow!(e))
}

/// List available snapshots (optionally filtered by prefix)
pub async fn list_snapshots(filter_prefix: Option<String>) -> anyhow::Result<Vec<SnapshotInfo>> {
    STATE.distrobox.list_snapshots(filter_prefix.as_deref()).await.map_err(|e| anyhow::anyhow!(e))
}

/// Delete a snapshot
pub async fn delete_snapshot(snapshot_name_or_id: String) -> anyhow::Result<String> {
    STATE.distrobox.delete_snapshot(&snapshot_name_or_id).await.map_err(|e| anyhow::anyhow!(e))
}

/// Restore a container from a snapshot (creates a new container)
pub async fn restore_from_snapshot(snapshot_name: String, new_container_name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Restore {} to {}", snapshot_name, new_container_name);
    
    let handle = tokio::spawn(async move {
        let res = distrobox.restore_from_snapshot(&snapshot_name, &new_container_name).await;
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Restore completed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Restore failed".into());
                    Err(anyhow::anyhow!("Restore failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), task_description, handle, tx));
    }
    
    Ok(task_id)
}

// ============================================================================
// Container Export/Import APIs
// ============================================================================

/// Export a container to a tar archive (returns task_id for streaming output)
pub async fn export_container_to_file(container_name: String, output_path: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Export {} to {}", container_name, output_path);
    
    let handle = tokio::spawn(async move {
        let res = distrobox.export_container(&container_name, &output_path);
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Export completed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Export failed".into());
                    Err(anyhow::anyhow!("Export failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), task_description, handle, tx));
    }
    
    Ok(task_id)
}

/// Import a container from a tar archive (returns task_id for streaming output)
pub async fn import_container_from_file(archive_path: String, image_name: String) -> anyhow::Result<String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = broadcast::channel(100);
    let tx_clone = tx.clone();
    
    let distrobox = STATE.distrobox.clone();
    let task_description = format!("Import {} as {}", archive_path, image_name);
    
    let handle = tokio::spawn(async move {
        let res = distrobox.import_container(&archive_path, &image_name);
        match res {
            Ok(mut child) => {
                let mut stdout = child.take_stdout().ok_or(anyhow::anyhow!("No stdout"))?;
                let mut stderr = child.take_stderr().ok_or(anyhow::anyhow!("No stderr"))?;
                
                let tx_out = tx_clone.clone();
                let tx_err = tx_clone.clone();
                
                let out_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stdout.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_out.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let err_task = tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stderr.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let s = String::from_utf8_lossy(&buf[..n]);
                                let _ = tx_err.send(s.to_string());
                            }
                            Err(_) => break,
                        }
                    }
                });
                
                let status = child.wait().await?;
                let _ = out_task.await;
                let _ = err_task.await;
                
                if status.success() {
                    let _ = tx_clone.send("Import completed successfully".into());
                    Ok(())
                } else {
                    let _ = tx_clone.send("Import failed".into());
                    Err(anyhow::anyhow!("Import failed"))
                }
            }
            Err(e) => {
                let _ = tx_clone.send(format!("Error: {}", e));
                Err(anyhow::anyhow!(e))
            }
        }
    });
    
    {
        let mut tasks = STATE.tasks.write().unwrap();
        tasks.insert(task_id.clone(), Task::new(task_id.clone(), task_description, handle, tx));
    }
    
    Ok(task_id)
}

// ============================================================================
// Resource Monitoring APIs
// ============================================================================

/// Get resource usage statistics for a container
pub async fn get_container_stats(container_name: String) -> anyhow::Result<ContainerStats> {
    STATE.distrobox.get_container_stats(&container_name).await.map_err(|e| anyhow::anyhow!(e))
}

/// Run an arbitrary command inside a container
pub async fn run_command_in_container(container_name: String, command: String) -> anyhow::Result<String> {
    STATE.distrobox.run_in_container(&container_name, &command).await.map_err(|e| anyhow::anyhow!(e))
}
