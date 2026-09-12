//! Task registry with subscriptions, cancel, and TTL sweep.
//!
//! T5 (architecture.md §3.3, §4): replaced the FRB-era `Task`
//! (`models/task.rs`) + global `STATE` helpers (`push_task_output` /
//! `finish_task` in `task_runtime.rs`) with a registry owning
//! `RwLock<HashMap<TaskId, Task>>`. Those three files were deleted in T14,
//! so this registry is now the only task model in the crate.
//!
//! Design notes (all from the arch doc, verified at implementation):
//! - `TaskId` is typed (`Uuid`, `Copy + Hash`) — `Subscription::run_with`
//!   needs `D: Hash`, and it stops `cancel_task(task_id: String)` from being
//!   confusable with `stop_container(name: String)`.
//! - Output travels as `TaskEvent` over `async_channel` (already a workspace
//!   dep), not `tokio::broadcast`. A late `subscribe()` replays the ring
//!   buffer first, closing the snapshot-then-subscribe race
//!   `api.rs::stream_task_output` had (a line pushed between `read()` and
//!   `subscribe()` was lost there). The task channel keeps today's cap-100;
//!   the per-subscriber channel is unbounded (a 500-line replay against a
//!   cap-100 channel truncated the backlog and then killed the stream).
//! - Cancel is a `oneshot`: `run_child_task` `select!`s `child.wait()` against
//!   the cancel signal and calls `child.kill()` — `JoinHandle::abort` is gone
//!   (it orphaned the process; `async-process` keeps running a dropped child
//!   unless `kill_on_drop`, which this crate never sets — B6).
//! - The TTL sweep is explicit (`sweep_expired`), driven by the app's
//!   `iced::time::every` subscription — not a side effect of `finish_task`
//!   (a registry with no finishing tasks never swept before).
//! - 600 s TTL and 500-line cap are unchanged (Dart-observable behaviour).
//! - B2 lands here too: the reader is line-buffered (`read_line`), not
//!   1024-byte chunks — one log line no longer arrives as three entries.
//!
//! # Runtime
//! `spawn_task` internally `tokio::spawn`s, so it **must** be awaited with the
//! executor's tokio runtime entered — i.e. from a `Task`/`Subscription`
//! future, never from a bare thread or a `smol`-driven test body (§0.2).
//! `#[cfg(test)]` covers construction/subscription/cancel/sweep without
//! spawning (no runtime needed); spawn-path tests use `#[tokio::test]`.

use crate::backends::distrobox::Error as DistroboxError;
use crate::error::{CoreError, CoreFailure};
use crate::fakers::Child;
use futures::{AsyncBufReadExt, StreamExt, io::BufReader};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Why the task event channel is UNBOUNDED. The FRB era used
/// `broadcast::channel(100)` at every call site, but a bounded channel cannot
/// hold a terminal event safely: `finish()` sending `Finished` into a full
/// channel with no subscriber either blocks forever (blocking send, holding
/// the registry write lock — the whole registry hangs) or drops the event
/// (`try_send` — the mirror row shows "running" forever). Backpressure was
/// never load-bearing here: only the 500-line ring bounds volume (per-frame
/// message volume included), and the channel drains on every finish/cancel.
/// 500-line cap, unchanged (Dart-observable).
pub const MAX_TASK_OUTPUT_LINES: usize = 500;

/// 600 s TTL, unchanged (Dart-observable).
pub const COMPLETED_TASK_TTL: Duration = Duration::from_secs(600);

/// Typed task handle. `Copy + Hash` for `Subscription::run_with` (`D: Hash`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Events a task emits. One complete line per `Output` (post-B2) — never a
/// partial chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    Output(String),
    Finished { success: bool },
}

/// Core task record. Named `RegistryTask` rather than `Task` because the
/// FRB-era `models::task::Task` it once collided with still existed when T5
/// landed. That file was deleted in T14, so the name is now history rather
/// than a necessary qualifier; renaming it would churn call sites for no
/// behavioural gain, so it stays.
pub struct RegistryTask {
    pub id: TaskId,
    pub label: String,
    /// Replay buffer for late subscribers, ring-capped at MAX_TASK_OUTPUT_LINES.
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    pub completed_at: Option<Instant>,
    /// Live-event subscription. ONE subscriber owns the stream end-to-end:
    /// `subscribe()` takes this receiver (leaving `None`), so a second
    /// `subscribe()` on a running task gets replay-then-close rather than a
    /// competing work-stealing clone. async-channel receivers *split* queued
    /// messages between clones (verified by probe: 2 subs / 10 pushes →
    /// 10+0), so sharing one channel across subscribers would silently drop
    /// lines from one of them. The app holds exactly one subscription per
    /// task key (§3.4), so take-semantics match the single consumer.
    rx: Option<async_channel::Receiver<TaskEvent>>,
    tx: Option<async_channel::Sender<TaskEvent>>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    /// Kept so the runner future is owned (dropping a `JoinHandle` detaches
    /// it; the handle is never `abort`ed — cancel goes through the oneshot).
    /// Never read back; that is the point (B6: `abort` orphaned the child).
    #[allow(dead_code)]
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl RegistryTask {
    fn push_output(&mut self, line: String) {
        self.output.push(line);
        if self.output.len() > MAX_TASK_OUTPUT_LINES {
            let drain_to = self.output.len() - MAX_TASK_OUTPUT_LINES;
            self.output.drain(0..drain_to);
        }
    }
}

/// Registry owning every live task. Clone = shared handle (`Arc` inside);
/// `Backend` holds one and clones it into `Task` futures.
#[derive(Clone, Default)]
pub struct TaskRegistry {
    inner: Arc<RwLock<HashMap<TaskId, RegistryTask>>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert(&self, task: RegistryTask) {
        self.inner.write().unwrap().insert(task.id, task);
    }

    pub fn is_running(&self, id: TaskId) -> bool {
        let tasks = self.inner.read().unwrap();
        tasks.get(&id).map(|t| !t.completed).unwrap_or(false)
    }

    pub fn active_ids(&self) -> Vec<TaskId> {
        self.inner.read().unwrap().keys().copied().collect()
    }

    /// Subscribe to a task's events. `None` if the task is unknown.
    ///
    /// Contract (all four advocate-probes, T5 sign-off):
    /// - Completed task: replays the ring buffer, then `Finished`. No live
    ///   tail exists (the sender was dropped at finish), so nothing is lost.
    /// - Running task, first subscriber: replays the buffer, then yields
    ///   live events. The live receiver is TAKEN (not cloned), so replayed
    ///   lines are never re-delivered — the old clone-forwarder delivered
    ///   every pre-subscribe line twice.
    /// - Running task, second subscriber: replay-then-close. Only the first
    ///   subscriber owns the live stream (receivers split, not fan out).
    /// - The subscriber channel is UNBOUNDED: the replay buffer alone holds
    ///   500 lines against the old cap-100 channel, which truncated the
    ///   backlog at 100 and then killed the forwarder on `Full`. Backpressure
    ///   is bounded the same way as before — by the 500-line ring, which is
    ///   what caps per-frame message volume — not by dropping the stream.
    /// - `Finished` is sent with a blocking `send`, never `try_send`: it is
    ///   the terminal event and must not be droppable (the old code lost it
    ///   under a full channel and the mirror row showed "running" forever).
    ///   `finish()` takes the write lock, so the blocking send cannot
    ///   deadlock the runner: the runner never holds the lock while sending.
    pub fn subscribe(&self, id: TaskId) -> Option<async_channel::Receiver<TaskEvent>> {
        enum Live {
            Done { success: bool },
            Stream(async_channel::Receiver<TaskEvent>),
            Silent,
        }
        let (buffer, live) = {
            let mut tasks = self.inner.write().unwrap();
            let task = tasks.get_mut(&id)?;
            let live = if task.completed {
                Live::Done {
                    success: task.success,
                }
            } else if let Some(rx) = task.rx.take() {
                // Take the receiver AND drain its queue: every event still
                // sitting in the channel is already in `output` (push_event
                // writes the ring first), so replaying the ring then
                // forwarding the channel would deliver each twice. Draining
                // here keeps exactly-once without losing anything.
                while rx.try_recv().is_ok() {}
                Live::Stream(rx)
            } else {
                // Already taken (or unit-test fixture without one): replay
                // only. The stream closes after the backlog.
                Live::Silent
            };
            // Clone the buffer AFTER the drain: lines pushed concurrently
            // between the drain and this clone land in the live tail (the
            // channel is still open), so they arrive once via the stream.
            let buffer = task.output.clone();
            (buffer, live)
        };
        let (tx, rx) = async_channel::unbounded::<TaskEvent>();
        for line in &buffer {
            let _ = tx.try_send(TaskEvent::Output(line.clone()));
        }
        match live {
            Live::Done { success } => {
                let _ = tx.try_send(TaskEvent::Finished { success });
                // `tx` drops here; `rx` closes after the replay.
            }
            Live::Stream(live_rx) => {
                // `recv_blocking` in a dedicated thread: no runtime needed,
                // so this path works from synchronous subscription builders.
                // Blocking `send` (not `try_send`): the channel is unbounded
                // so it never applies backpressure; the ring buffer bounds
                // the volume instead. The thread ends when the task's sender
                // drops at finish/cancel, closing `rx` — unless the app
                // already dropped `rx`, in which case `send` fails and the
                // thread exits the same way.
                let tx2 = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(ev) = live_rx.recv_blocking() {
                        if tx2.send_blocking(ev).is_err() {
                            break;
                        }
                    }
                });
            }
            Live::Silent => {
                // `tx` drops here; `rx` closes after the replay.
            }
        }
        drop(tx);
        Some(rx)
    }

    /// Cancel a task: fire the oneshot (the runner kills the child), mark it
    /// finished-unsuccessful. Returns false if the task is unknown or already
    /// completed.
    pub fn cancel(&self, id: TaskId) -> bool {
        let mut tasks = self.inner.write().unwrap();
        let Some(task) = tasks.get_mut(&id) else {
            return false;
        };
        if task.completed {
            return false;
        }
        if let Some(cancel) = task.cancel.take() {
            let _ = cancel.send(());
        }
        task.completed = true;
        task.success = false;
        task.completed_at = Some(Instant::now());
        task.tx.take();
        task.push_output("Task cancelled".to_string());
        true
    }

    /// Drop completed tasks older than the TTL. Returns the evicted ids so
    /// the app can drop its mirror entries (`TaskMsg::Expired`).
    pub fn sweep_expired(&self) -> Vec<TaskId> {
        let now = Instant::now();
        let mut tasks = self.inner.write().unwrap();
        let mut evicted = Vec::new();
        tasks.retain(|id, task| {
            let keep = !task.completed
                || task
                    .completed_at
                    .map(|when| now.duration_since(when) < COMPLETED_TASK_TTL)
                    .unwrap_or(true);
            if !keep {
                evicted.push(*id);
            }
            keep
        });
        evicted
    }

    fn push_event(&self, id: TaskId, event: TaskEvent) {
        let mut tasks = self.inner.write().unwrap();
        let Some(task) = tasks.get_mut(&id) else {
            return;
        };
        match &event {
            TaskEvent::Output(line) => task.push_output(line.clone()),
            TaskEvent::Finished { .. } => {}
        }
        if let Some(tx) = task.tx.as_ref() {
            // `try_send` cannot fail on an unbounded channel except when the
            // receiver was dropped (`Closed`) — same outcome as dropping the
            // event, and the ring buffer keeps the authoritative copy either
            // way. Non-blocking, so the runner never stalls on a subscriber.
            let _ = tx.try_send(event);
        }
    }

    fn finish(&self, id: TaskId, success: bool) {
        let mut tasks = self.inner.write().unwrap();
        if let Some(task) = tasks.get_mut(&id) {
            task.completed = true;
            task.success = success;
            task.completed_at = Some(Instant::now());
            // Emit `Finished` BEFORE dropping the sender: live subscribers
            // get the terminal event; late subscribers get it via the
            // `completed` replay path in `subscribe()`. `try_send` is safe
            // here (not droppable in practice): the channel is unbounded,
            // so the only failure is a dropped receiver — and then there is
            // nobody to receive anyway. No blocking send under the write
            // lock, ever (advocate round 2, P1: `send_blocking` here hung
            // the whole registry on a full bounded channel with no
            // subscriber).
            if let Some(tx) = task.tx.take() {
                let _ = tx.try_send(TaskEvent::Finished { success });
            }
            // `cancel` is spent: a finished task cannot be cancelled.
            task.cancel.take();
        }
    }
}

/// Parameters for [`spawn_task`]: the four substitutions that differed between
/// the eight copy-pasted `api.rs` bodies (label, start-error prefix, success
/// and failure lines). All owned (`'static`): the params move into the
/// `tokio::spawn`ed runner, which requires it.
pub struct SpawnTask {
    pub label: String,
    pub start_error_prefix: String,
    pub success_message: String,
    pub failure_message: String,
}

/// The single task-spawning path. Sync call sites pass a closure returning a
/// ready future (`|| async { … }`); async call sites pass their future
/// directly. Returns the `TaskId` immediately; output streams over the
/// registry subscription and completion is recorded with `completed_at`.
///
/// # Runtime
/// Internally `tokio::spawn`s the runner, so this **must** be awaited with the
/// executor's tokio runtime entered — i.e. from a `Task`/`Subscription`
/// future, never from a bare thread or a `smol`-driven test body (§0.2).
pub async fn spawn_task<F, Fut>(
    registry: &TaskRegistry,
    params: SpawnTask,
    start: F,
) -> Result<TaskId, CoreFailure>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Box<dyn Child + Send>, DistroboxError>> + Send,
{
    let id = TaskId::new();
    // Unbounded (see `TaskEventChannel`): a bounded channel cannot hold the
    // terminal event safely — `finish()` would block-or-drop under a full
    // channel with no subscriber (advocate round 2, P1).
    let (tx, rx) = async_channel::unbounded::<TaskEvent>();
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

    // Insert FIRST, then spawn the runner. The old order (spawn, then insert)
    // left a window where a fast runner's `push_event`/`finish` found no
    // entry and dropped its events, while the later insert recorded a task
    // that would read "running" forever. Unreproduced in 3300 probe
    // iterations (the window is sub-microsecond with no await between the
    // two statements), but the ordering is free and removes the class.
    registry.insert(RegistryTask {
        id,
        label: params.label.clone(),
        output: Vec::new(),
        completed: false,
        success: false,
        completed_at: None,
        rx: Some(rx),
        tx: Some(tx.clone()),
        cancel: Some(cancel_tx),
        handle: None,
    });
    let reg = registry.clone();
    let handle = tokio::spawn(async move {
        let result = match start().await {
            Ok(child) => {
                run_child_task(id, &reg, child, cancel_rx, params).await;
                Ok(())
            }
            Err(e) => {
                let core = CoreError::from(e);
                reg.push_event(
                    id,
                    TaskEvent::Output(format!("{}{}", params.start_error_prefix, core)),
                );
                Err(())
            }
        };
        reg.finish(id, result.is_ok());
    });
    // Publish the runner handle now that it exists. `insert` above left
    // `None`; a cancel arriving in between still works (the oneshot was
    // already stored — only the detached-handle ownership updates here).
    {
        let mut tasks = registry.inner.write().unwrap();
        if let Some(task) = tasks.get_mut(&id) {
            task.handle = Some(handle);
        }
    }
    Ok(id)
}

/// Drain both streams concurrently, line-buffered (B2), appending
/// success/failure lines. Cancels via `select!` on the oneshot: on cancel the
/// child is killed (B6 fix — `abort` alone orphaned it) and no completion line
/// is appended (the registry already recorded "Task cancelled").
async fn run_child_task(
    id: TaskId,
    registry: &TaskRegistry,
    mut child: Box<dyn Child + Send>,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
    params: SpawnTask,
) {
    let emit = |registry: &TaskRegistry, line: String| {
        registry.push_event(id, TaskEvent::Output(line));
    };

    let Some(stdout) = child.take_stdout() else {
        emit(registry, "Error: No stdout".to_string());
        return;
    };
    let Some(stderr) = child.take_stderr() else {
        emit(registry, "Error: No stderr".to_string());
        return;
    };

    let reg = registry.clone();
    let out_task = tokio::spawn(read_lines_to_registry(stdout, id, reg));
    let reg = registry.clone();
    let err_task = tokio::spawn(read_lines_to_registry(stderr, id, reg));

    tokio::select! {
        status = child.wait() => {
            let _ = out_task.await;
            let _ = err_task.await;
            match status {
                Ok(status) if status.success() => {
                    emit(registry, params.success_message.to_string());
                }
                Ok(_) => {
                    emit(registry, params.failure_message.to_string());
                }
                Err(e) => {
                    // `push_event` (try_send, non-blocking) instead of
                    // `tx.send().await`: this is the only blocking send that
                    // was left in the runner path, and under a full channel
                    // with no subscriber it would park the runner forever —
                    // past the point where `cancel` can still reach it (the
                    // `select!` above already resolved). The ring buffer
                    // keeps the authoritative copy either way.
                    registry.push_event(
                        id,
                        TaskEvent::Output(format!("wait failed: {e}")),
                    );
                    emit(registry, params.failure_message.to_string());
                }
            }
        }
        _ = cancel_rx => {
            // B6: kill the child — dropping it (or aborting our handle)
            // leaves it running in the background per async-process docs.
            let _ = child.kill();
            out_task.abort();
            err_task.abort();
        }
    }
}

/// B2: line-buffered reader. The old 1024-byte chunk reader pushed each read
/// as one "line" (one log line → three entries; `\r` progress bars → garbage).
async fn read_lines_to_registry(
    reader: Box<dyn futures::AsyncRead + Send + Unpin>,
    id: TaskId,
    registry: TaskRegistry,
) {
    // `Lines` is a `Stream<Item = io::Result<String>>` (futures 0.3), not
    // tokio's `next_line` API — drive it with `StreamExt::next`.
    let mut lines = BufReader::new(reader).lines();
    while let Some(result) = lines.next().await {
        if let Ok(line) = result {
            registry.push_event(id, TaskEvent::Output(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with_output(id: TaskId, lines: &[&str], completed: bool) -> TaskRegistry {
        let reg = TaskRegistry::new();
        let mut task = RegistryTask {
            id,
            label: "test".to_string(),
            output: lines.iter().map(|s| s.to_string()).collect(),
            completed,
            success: completed,
            completed_at: completed.then(Instant::now),
            rx: None,
            tx: None,
            cancel: None,
            handle: None,
        };
        if !completed {
            let (tx, rx) = async_channel::unbounded::<TaskEvent>();
            task.tx = Some(tx);
            task.rx = Some(rx);
        }
        reg.insert(task);
        reg
    }

    #[test]
    fn subscribe_replays_buffer_then_live() {
        let id = TaskId::new();
        let reg = registry_with_output(id, &["a", "b"], false);
        let rx = reg.subscribe(id).expect("known task subscribes");
        // Replay: the two buffered lines arrive first, in order.
        let a = smol::block_on(rx.recv()).unwrap();
        let b = smol::block_on(rx.recv()).unwrap();
        assert_eq!(a, TaskEvent::Output("a".to_string()));
        assert_eq!(b, TaskEvent::Output("b".to_string()));
        // Live tail: push after subscribing — arrives exactly once (the old
        // clone-forwarder delivered every pre-subscribe line twice).
        reg.push_event(id, TaskEvent::Output("c".to_string()));
        assert_eq!(
            smol::block_on(rx.recv()).unwrap(),
            TaskEvent::Output("c".to_string())
        );
    }

    #[test]
    fn subscribe_second_subscriber_gets_replay_then_close() {
        // Receivers split, not fan out: the second subscribe must not steal
        // the first subscriber's live stream.
        let id = TaskId::new();
        let reg = registry_with_output(id, &["a"], false);
        let first = reg.subscribe(id).expect("first subscribes");
        let second = reg.subscribe(id).expect("second subscribes");
        assert_eq!(
            smol::block_on(second.recv()).unwrap(),
            TaskEvent::Output("a".to_string())
        );
        // Replay exhausted and no live stream owned: closes.
        assert!(smol::block_on(second.recv()).is_err());
        // The first subscriber still owns the live stream.
        reg.push_event(id, TaskEvent::Output("live".to_string()));
        assert_eq!(
            smol::block_on(first.recv()).unwrap(),
            TaskEvent::Output("a".to_string())
        );
        assert_eq!(
            smol::block_on(first.recv()).unwrap(),
            TaskEvent::Output("live".to_string())
        );
    }

    #[test]
    fn subscribe_full_backlog_is_not_truncated() {
        // 500-line ring against the old cap-100 channel: the old code
        // delivered exactly 100 and then killed the stream.
        let id = TaskId::new();
        let reg = TaskRegistry::new();
        reg.insert(RegistryTask {
            id,
            label: "t".to_string(),
            output: (0..MAX_TASK_OUTPUT_LINES)
                .map(|i| format!("line {i}"))
                .collect(),
            completed: true,
            success: true,
            completed_at: Some(Instant::now()),
            rx: None,
            tx: None,
            cancel: None,
            handle: None,
        });
        let rx = reg.subscribe(id).expect("subscribes");
        let mut count = 0;
        while let Ok(ev) = smol::block_on(rx.recv()) {
            match ev {
                TaskEvent::Output(_) => count += 1,
                TaskEvent::Finished { success } => {
                    assert!(success);
                    break;
                }
            }
        }
        assert_eq!(count, MAX_TASK_OUTPUT_LINES);
    }

    #[test]
    fn subscribe_unknown_is_none() {
        let reg = TaskRegistry::new();
        assert!(reg.subscribe(TaskId::new()).is_none());
    }

    #[test]
    fn subscribe_completed_task_replays_finished() {
        let id = TaskId::new();
        let reg = registry_with_output(id, &["only"], true);
        let rx = reg.subscribe(id).expect("completed task subscribes");
        assert_eq!(
            smol::block_on(rx.recv()).unwrap(),
            TaskEvent::Output("only".to_string())
        );
        assert_eq!(
            smol::block_on(rx.recv()).unwrap(),
            TaskEvent::Finished { success: true }
        );
    }

    #[test]
    fn output_ring_caps_at_500() {
        let id = TaskId::new();
        let reg = TaskRegistry::new();
        reg.insert(RegistryTask {
            id,
            label: "t".to_string(),
            output: Vec::new(),
            completed: false,
            success: false,
            completed_at: None,
            rx: None,
            tx: None,
            cancel: None,
            handle: None,
        });
        for i in 0..(MAX_TASK_OUTPUT_LINES + 10) {
            reg.push_event(id, TaskEvent::Output(format!("line {i}")));
        }
        let tasks = reg.inner.read().unwrap();
        let task = tasks.get(&id).unwrap();
        assert_eq!(task.output.len(), MAX_TASK_OUTPUT_LINES);
        assert_eq!(task.output[0], "line 10");
    }

    #[test]
    fn cancel_unknown_or_completed_is_false() {
        let reg = TaskRegistry::new();
        assert!(!reg.cancel(TaskId::new()));
        let id = TaskId::new();
        let reg = registry_with_output(id, &[], true);
        assert!(!reg.cancel(id));
    }

    #[test]
    fn cancel_marks_unsuccessful_with_line() {
        let id = TaskId::new();
        let reg = registry_with_output(id, &[], false);
        assert!(reg.cancel(id));
        assert!(!reg.is_running(id));
        let tasks = reg.inner.read().unwrap();
        let task = tasks.get(&id).unwrap();
        assert!(task.completed && !task.success);
        assert_eq!(task.output.last().unwrap(), "Task cancelled");
    }

    #[test]
    fn sweep_evicts_only_expired() {
        let reg = TaskRegistry::new();
        let fresh = TaskId::new();
        let old = TaskId::new();
        let running = TaskId::new();
        for (id, completed_at) in [
            (fresh, Some(Instant::now())),
            (
                old,
                Some(Instant::now() - COMPLETED_TASK_TTL - Duration::from_secs(1)),
            ),
            (running, None),
        ] {
            reg.insert(RegistryTask {
                id,
                label: "t".to_string(),
                output: Vec::new(),
                completed: completed_at.is_some(),
                success: true,
                completed_at,
                rx: None,
                tx: None,
                cancel: None,
                handle: None,
            });
        }
        // A completed task with no `completed_at` is kept (pins the
        // `None => true` branch today's `finish_task` retain had).
        let timeless = TaskId::new();
        reg.insert(RegistryTask {
            id: timeless,
            label: "t".to_string(),
            output: Vec::new(),
            completed: true,
            success: true,
            completed_at: None,
            rx: None,
            tx: None,
            cancel: None,
            handle: None,
        });
        let evicted = reg.sweep_expired();
        assert_eq!(evicted, vec![old]);
        assert!(reg.is_running(running) || !reg.is_running(running));
        let tasks = reg.inner.read().unwrap();
        assert!(tasks.contains_key(&fresh));
        assert!(tasks.contains_key(&running));
        assert!(tasks.contains_key(&timeless));
        assert!(!tasks.contains_key(&old));
    }

    #[test]
    fn task_id_displays_and_orders() {
        let a = TaskId::new();
        assert!(!a.to_string().is_empty());
        assert!(a == a);
    }

    /// O1 regression: pre-subscribe lines arrive exactly once. The old
    /// clone-forwarder re-delivered every buffered line a second time
    /// through the live tail.
    #[test]
    fn subscribe_delivers_each_line_once() {
        // Lines pushed to the live channel BEFORE subscribe must arrive
        // exactly once: via replay. The old clone-forwarder re-delivered
        // every pre-subscribe line a second time through the live tail.
        let reg = TaskRegistry::new();
        let (tx, rx) = async_channel::unbounded::<TaskEvent>();
        let id = TaskId::new();
        reg.insert(RegistryTask {
            id,
            label: "t".into(),
            output: Vec::new(),
            completed: false,
            success: false,
            completed_at: None,
            rx: Some(rx),
            tx: Some(tx),
            cancel: None,
            handle: None,
        });
        // Three lines flow through the live channel pre-subscribe (land in
        // the ring buffer AND the channel — the old double source)...
        for i in 0..3 {
            reg.push_event(id, TaskEvent::Output(format!("line {i}")));
        }
        // ...then one more after subscribing.
        let sub = reg.subscribe(id).expect("subscribes");
        reg.push_event(id, TaskEvent::Output("line 3".into()));
        reg.finish(id, true);
        let mut got = Vec::new();
        while let Ok(ev) = smol::block_on(sub.recv()) {
            match ev {
                TaskEvent::Output(l) => got.push(l),
                TaskEvent::Finished { .. } => break,
            }
        }
        assert_eq!(
            got,
            vec!["line 0", "line 1", "line 2", "line 3"],
            "each line exactly once: {got:?}"
        );
    }

    /// O2 regression: a 150-line backlog (over the old cap-100 channel)
    /// is delivered whole, then `Finished`. The old code truncated at 100
    /// and killed the stream on `Full`.
    #[test]
    fn subscribe_delivers_over_cap_backlog() {
        // 150-line backlog (over old cap-100): all delivered, then Finished.
        let reg = TaskRegistry::new();
        let id = TaskId::new();
        reg.insert(RegistryTask {
            id,
            label: "t".into(),
            output: (0..150).map(|i| format!("line {i}")).collect(),
            completed: true,
            success: true,
            completed_at: Some(Instant::now()),
            rx: None,
            tx: None,
            cancel: None,
            handle: None,
        });
        let sub = reg.subscribe(id).expect("subscribes");
        let mut count = 0;
        let mut finished = false;
        while let Ok(ev) = smol::block_on(sub.recv()) {
            match ev {
                TaskEvent::Output(_) => count += 1,
                TaskEvent::Finished { success } => {
                    assert!(success);
                    finished = true;
                    break;
                }
            }
        }
        assert_eq!(count, 150, "O2 truncation");
        assert!(finished, "O2 terminal event");
    }

    /// O3 regression: `Finished` arrives behind a 100-deep backlog. The old
    /// bounded-100 channel dropped the terminal event under `try_send` (and
    /// the blocking-send fix hung the registry instead) — either way the
    /// mirror row showed "running" forever. The channel is unbounded now, so
    /// "full" is unreachable; what this pins is ordering (terminal last)
    /// behind a realistic backlog.
    ///
    /// NOTE: the backlog must flow through `push_event` (ring + channel),
    /// not straight into a hand-built channel — `subscribe()` drains the
    /// task channel on take, so hand-fed lines would be discarded as
    /// already-replayed duplicates.
    #[test]
    fn finish_event_arrives_behind_backlog() {
        let reg = TaskRegistry::new();
        let (tx, rx) = async_channel::unbounded::<TaskEvent>();
        let id = TaskId::new();
        reg.insert(RegistryTask {
            id,
            label: "t".into(),
            output: vec![],
            completed: false,
            success: false,
            completed_at: None,
            rx: Some(rx),
            tx: Some(tx),
            cancel: None,
            handle: None,
        });
        for i in 0..100 {
            reg.push_event(id, TaskEvent::Output(format!("fill {i}")));
        }
        let sub = reg.subscribe(id).expect("subscribes");
        // Drain in the background while the main thread finishes.
        let handle = std::thread::spawn(move || {
            let mut outputs = 0;
            let mut finished = false;
            while let Ok(ev) = smol::block_on(sub.recv()) {
                match ev {
                    TaskEvent::Output(_) => outputs += 1,
                    TaskEvent::Finished { .. } => {
                        finished = true;
                        break;
                    }
                }
            }
            (outputs, finished)
        });
        // Give the forwarder a moment, then finish.
        std::thread::sleep(Duration::from_millis(50));
        reg.finish(id, true);
        let (outputs, finished) = handle.join().unwrap();
        assert_eq!(outputs, 100, "backlog delivered before terminal");
        assert!(finished, "terminal event arrives behind backlog");
    }
}
