//! `Message` — the T3 read-only subset plus the T5 task-stream subset.
//!
//! Task-spawning request variants (`CreateRequested`, `UpgradeRequested`, …)
//! still arrive with their owner page tasks (T6–T12); what lands here is the
//! task *lifecycle* (`TaskMsg`), which T5's subscription produces. Remaining
//! domains (packages, snapshots, backups, terminals, config, dialogs) follow
//! the same rule as before: no dead variants behind the exhaustive match.

use gosh_distrobox_core::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use gosh_distrobox_core::{CoreFailure, EnvGuard, EnvMode, TaskId};

/// Top-level message. `Message::NavSelect` is delivered by libcosmic as
/// `cosmic::Action::Cosmic`, not produced by our own widgets.
#[derive(Clone, Debug)]
pub enum Message {
    /// Nav-bar selection. Constructed by libcosmic (`cosmic::Action::Cosmic`
    /// → `on_nav_select`), never by our own widgets — hence no local producer.
    #[allow(dead_code)]
    NavSelect(cosmic::widget::nav_bar::Id),
    Containers(ContainerMsg),
    Apps(AppMsg),
    Images(ImageMsg),
    Stats(StatsMsg),
    Tasks(TaskMsg),
    /// Re-probe result path (§2.3 draft). No producer until T9/T12.
    #[allow(dead_code)]
    Env(EnvMsg),
    Ui(UiMsg),
}

/// Task lifecycle (§2.3 draft, T5 subset). `Started`'s `Err` means the task
/// could not even be started; `Output` batches stream lines into
/// `TaskView.output` (ring-buffered); `Expired` is TTL-sweep evidence from
/// core so the UI drops its mirror entry.
///
/// Request variants have no producers until their owner pages land (T6–T12
/// drive `Started` via spawn calls; T11 drives cancel/clear). The scoped
/// allows mark exactly that — T6 removes `Started`'s when the first spawn
/// call site lands, T11 the rest. No global allow: a variant still dead
/// after its owner task is a real finding.
#[derive(Clone, Debug)]
pub enum TaskMsg {
    #[allow(dead_code)]
    Started {
        label: String,
        result: Result<TaskId, CoreFailure>,
    },
    Output {
        id: TaskId,
        lines: Vec<String>,
    },
    Completed {
        id: TaskId,
        success: bool,
    },
    #[allow(dead_code)]
    CancelRequested(TaskId),
    /// Fired back after the registry confirms cancel (the id rides along so
    /// the T11 log can mark the row without re-reading the registry).
    Cancelled(#[allow(dead_code)] TaskId),
    #[allow(dead_code)]
    ClearCompleted,
    Expired(Vec<TaskId>),
    /// The 30 s sweep tick. Carries nothing (subscription builders cannot
    /// borrow the registry); the `update` arm sweeps via `BACKEND` and drops
    /// the evicted mirror entries. Separate from `Expired(ids)` — the tick
    /// is the clock, `Expired` is the evidence.
    ExpiredTick,
}

/// Union of the Dart `refresh()` + `loadContainers()`.
#[derive(Clone, Debug)]
pub enum ContainerMsg {
    RefreshRequested,
    Loaded(Result<Vec<ContainerInfo>, CoreFailure>),
    Selected(Option<ContainerInfo>),
    /// Result of a short (non-task) mutation; `Err` becomes a toast.
    /// (No producer in T3 — kept so the match arm shape matches the draft;
    /// producers land with T6's action buttons.)
    #[allow(dead_code)]
    ActionFinished(Result<String, CoreFailure>),
}

#[derive(Clone, Debug)]
pub enum AppMsg {
    /// Explicit reload (button lands with the Apps page in T12).
    #[allow(dead_code)]
    LoadRequested(String),
    Loaded(Result<Vec<AppInfo>, CoreFailure>),
    /// Explicit reload (button lands with the Apps page in T12).
    #[allow(dead_code)]
    BinariesLoadRequested(String),
    BinariesLoaded(Result<Vec<ExportedBinary>, CoreFailure>),
}

#[derive(Clone, Debug)]
pub enum ImageMsg {
    /// Explicit reload (refresh button lands with the Images page in T7).
    #[allow(dead_code)]
    LoadRequested,
    Loaded(Result<Vec<String>, CoreFailure>),
}

#[derive(Clone, Debug)]
pub enum StatsMsg {
    /// Explicit reload (refresh button lands with the Stats view in T6).
    #[allow(dead_code)]
    LoadRequested(String),
    Loaded(Result<ContainerStats, CoreFailure>),
}

#[derive(Clone, Debug)]
pub enum EnvMsg {
    /// Re-probe result. No producer in T3 (probe runs in `init`); re-probe
    /// buttons land in T9/T12.
    #[allow(dead_code)]
    Probed(EnvGuard),
}

#[derive(Clone, Debug)]
pub enum UiMsg {
    DismissError,
}

/// Derived view helper: containers currently `Up` (Dart
/// `runningContainersCount`). First caller lands with the Dashboard in T6.
#[allow(dead_code)]
pub fn running_count(containers: &[ContainerInfo]) -> usize {
    containers
        .iter()
        .filter(|c| matches!(c.status, gosh_distrobox_core::models::Status::Up(_)))
        .count()
}

/// Whether the env guard blocks all backend access.
pub fn is_blocked(env: &EnvGuard) -> bool {
    env.mode == EnvMode::Blocked
}
