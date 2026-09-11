//! `Message` — the T3 subset of the architecture.md §2.3 draft.
//!
//! Only the read-only browser domains land here: nav selection, containers,
//! images, apps, stats, env, and UI errors. Task-spawning variants (`Create`,
//! `Upgrade`, `Clone`, `Tasks(Started/Output/Completed/…)`, packages,
//! snapshots, backups, terminals, config, dialogs) arrive with their owner
//! tasks (T5–T12); adding them now would be dead code behind an exhaustive
//! match.

use gosh_distrobox_core::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use gosh_distrobox_core::{CoreFailure, EnvGuard, EnvMode};

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
    /// Re-probe result path (§2.3 draft). No producer until T9/T12.
    #[allow(dead_code)]
    Env(EnvMsg),
    Ui(UiMsg),
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
