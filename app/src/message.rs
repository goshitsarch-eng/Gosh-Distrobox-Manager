//! `Message` — the T3 read-only subset plus the T5 task-stream subset plus
//! the T6 dashboard/containers/details requests.
//!
//! Task-spawning request variants for later domains (`CreateRequested`,
//! packages, snapshots, backups, terminals, config) still arrive with their
//! owner page tasks (T7–T12); what lands here is the task *lifecycle*
//! (`TaskMsg`), which T5's subscription produces, plus the T6 container
//! actions (stop/remove/stop-all/upgrade/clone-terminal-open) and dialog
//! control. Same rule as before: no dead variants behind the exhaustive
//! match — every variant added here has a producer in this task.

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
    Details(DetailsMsg),
    Dialog(DialogMsg),
    Wizard(WizardMsg),
    Images(ImageMsg),
    Packages(PackagesMsg),
    /// Reserved namespace (T9+: the Updates page reads shared state and
    /// needs no page-local messages today).
    #[allow(dead_code)]
    Updates(UpdatesMsg),
    Terminal(TerminalMsg),
    Apps(AppMsg),
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
/// `ClearCompleted` / `Cancelled` payload have no producers until T11
/// drives them. No global allow: a variant still dead after its owner task
/// is a real finding.
#[derive(Clone, Debug)]
pub enum TaskMsg {
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

/// Union of the Dart `refresh()` + `loadContainers()`, plus the T6
/// container actions (stop/remove/stop-all/upgrade — rows #25/#30/#50/#51).
/// Short mutations (`Stop/Remove/StopAll`) run synchronously and report via
/// `ActionFinished` (`Err` becomes a toast); `UpgradeRequested` spawns a
/// task and reports via `TaskMsg::Started`.
#[derive(Clone, Debug)]
pub enum ContainerMsg {
    RefreshRequested,
    Loaded(Result<Vec<ContainerInfo>, CoreFailure>),
    Selected(Option<ContainerInfo>),
    /// Result of a short (non-task) mutation; `Err` becomes a toast.
    ActionFinished(Result<String, CoreFailure>),
    StopRequested(String),
    RemoveRequested(String),
    StopAllRequested,
    UpgradeRequested(String),
    /// B5 start (typed): true start leaving the container `Up`; callers
    /// refresh `list()` so the transition is observed, not assumed.
    StartRequested(String),
    /// "View all" (row #23, dead in Flutter): switch to the Containers tab.
    ViewAllRequested,
    /// Dashboard "New Container" (row #28, dead in Flutter): opens a
    /// blank wizard (preselected images come from the Images page, #95).
    NewContainerRequested,
    /// Dashboard "Upgrade All" (row #29, snackbar redirect in Flutter):
    /// confirm, then spawn an upgrade task per running container.
    UpgradeAllRequested,
}

/// Details page (rows #53–#65): nested page under Containers, pushed from a
/// row tap, popped with the header back button.
#[derive(Clone, Debug)]
pub enum DetailsMsg {
    /// Open the details page for this container.
    OpenRequested(ContainerInfo),
    /// Back button → pop to the list.
    Closed,
    StopRequested(String),
    RemoveRequested(String),
    UpgradeRequested(String),
    CloneRequested(String),
    /// Clone dialog keystrokes (name field).
    CloneNameChanged(String),
    /// Clone dialog confirmed with the new name.
    CloneConfirmed {
        source: String,
        name: String,
    },
    /// Copy the image URL to the clipboard (row #57).
    CopyImageRequested(String),
    /// "Applications" tile (row #61): jump to the Apps page for this
    /// container (full Apps page lands in T12; T6 selects + switches).
    AppsRequested(String),
    /// "Open Terminal" tile/header (rows #54/#63): terminal page lands in
    /// T9 — T6 records the request as a toast pointing there.
    TerminalRequested(String),
}

/// Single active modal (§3.3): `Application::dialog()` owns one slot, so at
/// most one of these is `Some` at a time. Confirm dialogs share copy through
/// the constructor (fixing the card-vs-details divergence, row #52).
#[derive(Clone, Debug)]
pub enum DialogMsg {
    Cancelled,
    Confirmed,
}

/// Shared confirm spec (§3.3): title + consequence body + verb label +
/// destructive class. One copy per action — no per-page drift.
#[derive(Clone, Debug)]
pub struct ConfirmSpec {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub destructive: bool,
    /// The follow-up to re-dispatch on confirm (plain enum, not boxed —
    /// the payloads are small data).
    pub action: ConfirmAction,
}

/// Follow-up for a confirmed dialog. `Clone` (Message: Clone) — the payloads
/// are data, re-dispatched as fresh messages on confirm.
#[derive(Clone, Debug)]
pub enum ConfirmAction {
    RemoveContainer(String),
    StopAll,
    UpgradeAll,
    InstallPackage { container: String, package: String },
    RemovePackage { container: String, package: String },
    UpgradeContainer(String),
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

/// Updates page (T9, rows #123–#132): upgrade-all confirm state lives in
/// the shared `ConfirmAction::UpgradeAll` dialog; no page-local messages
/// beyond refresh (header buttons reuse `ContainerMsg`).
#[derive(Clone, Debug)]
pub enum UpdatesMsg {
    /// Placeholder for page-local state (none today — the page reads the
    /// shared container list + task mirror). Kept so the page owns a
    /// message namespace when T13+ needs one.
    #[allow(dead_code)]
    Noop,
}

/// Terminal-launch page (T9, rows #66–#77 + D8 revival).
#[derive(Clone, Debug)]
pub enum TerminalMsg {
    /// Open the terminal page for this container.
    OpenRequested(String),
    /// Back button → pop to the previous page.
    Closed,
    /// Reload the enter-command display (Retry).
    CommandReloadRequested(String),
    /// Enter-command loaded (argv for display + launch).
    CommandLoaded(String, Result<Vec<String>, CoreFailure>),
    /// Copy the enter command (rows #70–#72).
    CopyRequested(String),
    /// Terminal picker selection (index into Backend terminal list).
    TerminalSelected(usize),
    /// Launch the selected terminal attached to the container (D8).
    LaunchRequested(String),
    /// Terminal launched (toast reports; no output subscription).
    /// (Kept for draft shape — launch is synchronous today, so the toast
    /// fires inline; a future async launcher would route through this.)
    #[allow(dead_code)]
    LaunchFinished(Result<String, CoreFailure>),
}

/// Package manager (T8, rows #106–#122): picker, tabs, search, install /
/// remove / upgrade-all confirms, manual command for `Unknown` PM.
#[derive(Clone, Debug)]
pub enum PackagesMsg {
    /// Picker index selected (row #109).
    ContainerSelected(usize),
    /// Tab switch (row #107).
    TabSelected(bool),
    /// Search box keystrokes.
    QueryChanged(String),
    /// Search submitted (Enter) — runs only when running (#112).
    SearchSubmitted,
    /// Clear search → back to Installed tab.
    SearchCleared,
    /// Reload installed list (Retry + refresh).
    ReloadRequested(String),
    /// PM detected (typed B1 — never a bare string).
    ManagerDetected(
        String,
        Result<gosh_distrobox_core::models::PackageManager, CoreFailure>,
    ),
    /// Installed list result.
    InstalledLoaded(
        String,
        Result<Vec<gosh_distrobox_core::models::PackageInfo>, CoreFailure>,
    ),
    /// Search results.
    SearchLoaded(Result<Vec<gosh_distrobox_core::models::PackageInfo>, CoreFailure>),
    /// Install-from-search-box button (#115 — disabled when empty).
    InstallFromBox,
    /// Package row Install → confirm (#120).
    InstallRequested(String),
    /// Package row Remove → confirm (#120).
    RemoveRequested(String),
    /// Upgrade All → confirm (#116).
    UpgradeAllRequested,
    /// Manual command box (B1 `Unknown`).
    /// Manual command box (B1 `Unknown`).
    ManualCmdChanged(String),
    ManualRunRequested,
}

/// Images page: load domain (T3) + page UI (T7, rows #97–#105).
#[derive(Clone, Debug)]
pub enum ImageMsg {
    LoadRequested,
    Loaded(Result<Vec<String>, CoreFailure>),
    SearchChanged(String),
    CustomChanged(String),
    /// Custom-URL arrow → wizard with preselected image (#103, dead in Flutter).
    CustomSubmitted,
    /// Image card "+" / tap → details dialog (#102, dead in Flutter).
    DetailsRequested(String),
    DetailsClosed,
    /// Details dialog "Create Container" → wizard with preselected image.
    CreateWithImage(String),
}

/// Create wizard (rows #78–#96): every control on steps 0–2.
#[derive(Clone, Debug)]
pub enum WizardMsg {
    /// Close the wizard (Cancel / Done). Open paths: header New Container
    /// (blank), Images card Select + custom URL (preselected, #95) —
    /// preselected images arrive ONLY from the Images page.
    Closed,
    ImageSelected(String),
    CustomChanged(String),
    SearchChanged(String),
    NextFromImage,
    NameChanged(String),
    InitToggled(bool),
    NvidiaToggled(bool),
    AdvancedToggled,
    HomeChanged(String),
    VolumeAddRequested,
    VolumeDialogHostChanged(String),
    VolumeDialogContainerChanged(String),
    VolumeDialogReadOnlyToggled(bool),
    VolumeDialogConfirmed,
    VolumeDialogCancelled,
    VolumeRemoved(usize),
    BackToImage,
    /// Create pressed: validate (`CreateArgName` inline, #91/#96) then spawn
    /// `create_container` → progress step on `TaskMsg::Started`.
    CreateRequested,
    /// Progress step Cancel: cancel the create task (T5 registry).
    ProgressCancelRequested,
    /// Progress step Done/Close: leave the wizard (list refreshes behind).
    /// BackToImage pops step 1 → 0 (Back on config); no BackToConfig
    /// exists (progress never goes back — Cancel/Done only).
    ProgressDone,
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
    /// Toast closed (toaster `on_close`).
    ToastClosed(cosmic::widget::toaster::ToastId),
    /// Copy completed — surface the transient confirmation as a toast.
    CopiedToClipboard(String),
}

/// Derived view helpers (Dart `runningContainersCount` /
/// `stoppedContainersCount`).
pub fn running_count(containers: &[ContainerInfo]) -> usize {
    containers
        .iter()
        .filter(|c| matches!(c.status, gosh_distrobox_core::models::Status::Up(_)))
        .count()
}

pub fn stopped_count(containers: &[ContainerInfo]) -> usize {
    containers.len() - running_count(containers)
}

/// Whether the env guard blocks all backend access.
pub fn is_blocked(env: &EnvGuard) -> bool {
    env.mode == EnvMode::Blocked
}
