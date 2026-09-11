//! Task registry, output capture, and child-process streaming.
//!
//! Moved out of `api.rs` by S4 (architecture.md §1.3) so that the FRB shim layer
//! holds nothing but thin wrappers over it. S4 is a **pure relocation**: the
//! `TaskEvent` channel redesign (B9, §3.3), the cancel-kill fix (B8) and
//! `spawn_task` (§4.1) all land in the task-runtime build task, not here, and the
//! 500-line cap / 600 s TTL behaviour is unchanged because the Dart side observes it.
//!
//! # Runtime
//! [`run_child_task`] `tokio::spawn`s its stream readers. That is deliberate and
//! stays inside this module and its `api.rs` wrappers only -- `Distrobox` methods
//! must never `tokio::spawn`, because the backend tests drive them under
//! `smol::block_on` (architecture.md §0.2).

use crate::app_state::AppState;
use crate::fakers::Child;
use crate::models::Task;
use futures::{AsyncRead, AsyncReadExt};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};

/// The task registry `AppState` holds: task id -> task.
///
/// §3.3 turns this into a `TaskRegistry` struct owning `RwLock<HashMap<TaskId, Task>>`
/// with the TTL sweep and per-task subscriptions; S4 only gives the alias a home.
pub type TaskRegistry = Arc<RwLock<HashMap<String, Task>>>;

pub fn new_registry() -> TaskRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

/// The process-wide application state the FRB shim layer talks to. S6 replaces
/// this global with `Backend` (architecture.md §6); until then it is the single
/// owner of the task registry.
pub static STATE: LazyLock<AppState> = LazyLock::new(AppState::new);

pub const MAX_TASK_OUTPUT_LINES: usize = 500;
pub const COMPLETED_TASK_TTL: Duration = Duration::from_secs(600);

pub fn push_task_output(task_id: &str, line: String) {
    let mut tasks = STATE.tasks.write().unwrap();
    if let Some(task) = tasks.get_mut(task_id) {
        task.push_output(line.clone(), MAX_TASK_OUTPUT_LINES);
        if let Some(tx) = task.tx.as_ref() {
            let _ = tx.send(line);
        }
    }
}

pub fn finish_task(task_id: &str, success: bool) {
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

pub async fn stream_reader_to_task_output(mut reader: impl AsyncRead + Unpin, task_id: String) {
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

pub async fn run_child_task(
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
