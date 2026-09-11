//! `Message` — one variant per page/domain, each carrying its own sub-enum.
//!
//! Every page task has now landed (T3–T12), so no domain is still waiting
//! for its owner: the read-only browser (T3), task lifecycle (`TaskMsg`,
//! T5), dashboard/containers/details (T6), wizard/images (T7), packages
//! (T8), updates/terminal (T9), backups (T10), activity (T11) and
//! settings/apps/config (T12) are all present and handled. Same rule as
//! before: no dead variants behind the exhaustive match — every variant
//! added here has a producer.
//!
//! Two namespaces are reserved and carry no producer yet, each marked
//! `#[allow(dead_code)]` with its reason at the variant: `NavSelect` (built
//! by libcosmic) and `Updates(msg)` (the page reads shared state).
//! `Env(EnvMsg::Probed)` is reserved for the T13+ re-probe path.

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
    Backups(BackupsMsg),
    Activity(ActivityMsg),
    Settings(SettingsMsg),
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
/// `Cancelled` payload closes a swept drawer watching the id (B2 class).
/// No global allow: a variant still dead after its owner task is a real
/// finding.
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
    Cancelled(TaskId),
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
    DeleteSnapshot(String),
    DeleteAllContainers,
}

#[derive(Clone, Debug)]
pub enum AppMsg {
    /// Container-tagged (same reason as `BackupsMsg::SnapshotsLoaded`): an
    /// export re-sync runs alongside whatever the user navigates to next,
    /// so an untagged reply could land after the page already shows another
    /// container — the grid would then list container A's apps under
    /// container B's header and every toggle would export A's desktop file
    /// into B.
    Loaded(String, Result<Vec<AppInfo>, CoreFailure>),
    BinariesLoaded(String, Result<Vec<ExportedBinary>, CoreFailure>),
    /// Search keystrokes (row #174).
    SearchChanged(String),
    /// Manual binary export dialog open (row #175).
    BinaryDialogRequested,
    BinaryDialogClosed,
    BinaryPathChanged(String),
    BinaryExportConfirmed,
    /// Reload apps + binaries (Retry/header refresh).
    ReloadRequested(String),
    /// Export toggle ON (row #178).
    ExportRequested(String, String),
    /// Export toggle OFF.
    UnexportRequested(String, String),
    /// Export / unexport / binary-export result (container name, outcome).
    /// Distinct from `ContainerMsg::ActionFinished` because the refresh it
    /// owes the user is the APPS list, not just the container list — the
    /// toggle's own EXPORTED label lives in `app.is_exported` (#178), so
    /// reloading containers alone would leave the switch it just flipped
    /// showing its old state.
    ActionFinished(String, Result<String, CoreFailure>),
}

/// Settings page (T12, rows #163–#171 + config §5).
#[derive(Clone, Debug)]
pub enum SettingsMsg {
    VersionReloadRequested,
    VersionLoaded(Result<String, CoreFailure>),
    RefreshAllRequested,
    StopAllRequested,
    UpgradeAllRequested,
    ClearCompleted,
    DeleteAllRequested,
    /// Preference writes (row #171 — best-effort, failures toast).
    TerminalSelected(usize),
    ConfirmToggled(bool),
    SnapshotPrefixChanged(String),
    ExportDirChanged(String),
    /// About links (row #169 — URL open results toast on failure).
    OpenUrl(String),
    /// Reactive config reload from `watch_config` (§5.1-2: external edits
    /// land live; in-session writes already match, so no-op then).
    ConfigChanged(gosh_distrobox_core::AppConfig),
    /// One-time DistroShelf legacy import finished (D11/PKG-9): applied only
    /// for keys our own config does not already set.
    LegacyImported(gosh_distrobox_core::LegacyImport),
}

/// Backups page (T10, rows #133–#151): picker, tabs, snapshot CRUD,
/// export/import/clone dialogs, portal choosers (P0 §4.3).
#[derive(Clone, Debug)]
pub enum BackupsMsg {
    /// Picker index selected (row #136).
    ContainerSelected(usize),
    /// Snapshots vs transfer tab (row #133).
    TabSelected(bool),
    /// Reload snapshots (Retry + refresh).
    ReloadRequested,
    /// Snapshot list result, scoped to a container (stale-response guard:
    /// only the current container sticks — same as InstalledLoaded).
    SnapshotsLoaded(
        String,
        Result<Vec<gosh_distrobox_core::models::SnapshotInfo>, CoreFailure>,
    ),
    /// Create dialog open (row #140, prefilled name).
    CreateDialogRequested,
    CreateNameChanged(String),
    CreateConfirmed,
    /// Create result (toast + reload — without the reload the page
    /// strands "No snapshots yet" inviting a duplicate).
    CreateFinished(Result<String, CoreFailure>),
    /// Delete result (row #141 green/red toasts + reload).
    DeleteFinished(Result<String, CoreFailure>),
    /// Delete → confirm (row #141).
    DeleteRequested(String),
    /// Restore dialog open (row #142, prefilled new name).
    RestoreDialogRequested(String),
    RestoreNameChanged(String),
    RestoreConfirmed,
    /// Export dialog open (row #143).
    ExportDialogRequested,
    ExportPathChanged(String),
    /// Portal save picker finished (P0 — replaces free-text path).
    ExportPathPicked(Result<String, String>),
    ExportBrowseRequested,
    ExportConfirmed,
    /// Import dialog open (row #144).
    ImportDialogRequested,
    ImportPathChanged(String),
    ImportImageChanged(String),
    /// Portal open picker finished.
    ImportPathPicked(Result<String, String>),
    ImportBrowseRequested,
    ImportConfirmed,
    /// Clone dialog open (row #145): opens the SHARED details-clone
    /// dialog directly (§4.5 unification — no page-local clone state).
    CloneDialogRequested,
    /// Any backups dialog cancelled.
    DialogCancelled,
}

/// Activity log (T11, rows #152–#162): search, filter, expanded drawer.
#[derive(Clone, Debug)]
pub enum ActivityMsg {
    SearchChanged(String),
    FilterSelected(crate::activity::ActivityFilter),
    /// Open the full-output drawer for this task (row #158).
    Expanded(gosh_distrobox_core::TaskId),
    /// Close the drawer.
    DrawerClosed,
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
