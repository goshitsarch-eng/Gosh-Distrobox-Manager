//! `Application` shell: S5 skeleton + S6 read-only browser (T3).
//!
//! Return conventions per architecture.md §2.3: `Task::none()` for no-ops,
//! `Task::done(cosmic::Action::App(m))` for immediate follow-ups, and
//! `cosmic::task::future(f)` as the **only** place `Backend` methods run —
//! never invoked synchronously (§0.2: the same entry points are also reached
//! with no runtime at all, and routing through a `Task` is what carries the
//! result back to the UI).
//!
//! [`Backend`] is held behind `Arc` (cheap to clone into task futures). T3 has
//! no task-output subscriptions yet — those land with T5's `spawn_task`.

use crate::fl;
use crate::icons;
use crate::message::{
    AppMsg, ConfirmAction, ConfirmSpec, ContainerMsg, DetailsMsg, DialogMsg, EnvMsg, ImageMsg,
    Message, StatsMsg, TaskMsg, UiMsg, is_blocked, running_count,
};
use crate::views::{self, Page, active_page};
use cosmic::app::{ApplicationExt, Core, Task};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::toaster::Toasts;
use cosmic::widget::{self, nav_bar};
use gosh_distrobox_core::fakers::CommandRunner;
use gosh_distrobox_core::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use gosh_distrobox_core::{
    Backend, ContainerList, CoreError, CoreFailure, MAX_TASK_OUTPUT_LINES, TaskEvent, TaskId,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Reverse-domain id (D15-adjacent): same string as the gschema id, the
/// metainfo `<id>`, the `.desktop` `Icon=`, and the `cosmic-config` id (§5.2).
pub const APP_ID: &str = "io.github.gosh_distrobox_manager";

/// Active modal slot (§3.3): at most one dialog at a time. `Confirm` shares
/// copy through `ConfirmSpec` (row #52 divergence fixed); `Clone` carries
/// the dialog's editable name.
#[derive(Clone, Debug)]
pub enum ActiveDialog {
    Confirm(ConfirmSpec),
    Clone { source: String, name: String },
}

/// Independent per-domain spinners, mirroring the eight Dart `isLoadingX`
/// getters (§3.1). T3 wires four; the rest arrive with their domains.
#[derive(Default)]
pub struct Loading {
    pub containers: bool,
    pub images: bool,
    pub apps: bool,
    pub binaries: bool,
    pub stats: bool,
}

pub struct App {
    core: Core,
    nav_model: nav_bar::Model,
    /// Owns the env-mapped runner, the `Distrobox`, and the task registry.
    /// Cloned into every `Task` future; never touched synchronously.
    backend: Arc<Backend>,
    /// `Err` message text for the current error banner, if any.
    error: Option<String>,

    /// B3 wrapper: the parsed container list PLUS the rows distrobox
    /// emitted that did not parse. Read-through to `containers` via `Deref`,
    /// so the existing reads below are unchanged; `skipped` is what
    /// `show_skipped_lines` surfaces (architecture.md §6.4, row B3).
    containers: ContainerList,
    selected_container: Option<String>,
    images: Vec<String>,
    /// Images page's OWN load error (O3): the global `error` banner already
    /// reports failures everywhere — passing it into the images view made
    /// every unrelated failure render as "Could not load images". Set on
    /// `Images Loaded(Err)`, cleared on request/success.
    images_error: Option<String>,
    /// Apps page state (T12, rows #174/#179): search + binary dialog.
    apps_search: String,
    apps_binary_path: String,
    apps_binary_error: Option<String>,
    /// Apps page's OWN load error (same reasoning as `images_error`): the
    /// global banner would render every unrelated failure as "Could not
    /// load apps". Set on `Apps Loaded/BinariesLoaded(Err)`, cleared on
    /// request and on success.
    apps_error: Option<String>,
    apps_binary_dialog: bool,
    /// Images page state (T7, rows #99/#103): live search + custom URL.
    images_search: String,
    images_custom: String,
    /// Image details dialog image URL (row #102).
    image_details: Option<String>,
    /// Create wizard state (T7, rows #78–#96). `Some` = wizard open (pushed
    /// over Containers like details — single level, Back/Close pops).
    wizard: Option<crate::wizard::WizardState>,
    /// Package manager state (T8, rows #106–#122).
    packages: crate::packages::PackagesState,
    /// Backups state (T10, rows #133–#151).
    backups: crate::backups::BackupsState,
    /// Activity state (T11, rows #152–#162).
    activity: crate::activity::ActivityState,
    /// Terminal page state (T9, rows #66–#77 + D8).
    terminal: crate::terminal::TerminalState,
    /// Persisted preferences (T12 §5: cosmic-config, degrade-don't-crash).
    /// `None` = config dir unavailable (defaults render, writes toast).
    config: Option<gosh_distrobox_core::AppConfig>,
    /// D11/PKG-9 one-time gate: `true` while the legacy keys were absent at
    /// load. Cleared on the import result (applied or not) — never re-run,
    /// so a user who deletes an imported key keeps it deleted.
    legacy_import_pending: bool,
    /// Distrobox version display (row #163).
    distrobox_version: String,
    loading_version: bool,
    apps: Vec<AppInfo>,
    exported_binaries: Vec<ExportedBinary>,
    stats: Option<ContainerStats>,
    stats_for: Option<String>,
    loading: Loading,
    /// UI-side mirror of core's registry (§3.1): `output` is a view buffer
    /// appended from `TaskMsg::Output` (ring-capped at
    /// `MAX_TASK_OUTPUT_LINES`); the authoritative buffer lives in core and
    /// `Expired` drops the mirror entry.
    tasks: BTreeMap<TaskId, TaskView>,
    /// Details page stack: `Some` = pushed over Containers (row #53 back
    /// button pops). Single level — details never nests deeper.
    details_for: Option<ContainerInfo>,
    /// Single active modal (§3.3).
    dialog: Option<ActiveDialog>,
    /// Toasts (§3.4): every mutation reports here.
    toasts: Toasts<Message>,
    /// Short-mutation re-press guards (stop/remove per container, stop-all):
    /// a second press while one is in flight is ignored. (No busy LABEL is
    /// rendered — buttons keep their normal text; the guard is behavioural,
    /// not visual.)
    busy: std::collections::BTreeSet<String>,
}

/// The UI-side mirror of a core task (§3.1). `label` renders in task rows
/// (dashboard) and the Activity page (T11); `started_at` gets its reader
/// in T11.
/// What a task *is*, for routing and matching.
///
/// Distinct from [`TaskView::label`], which is user-visible and therefore
/// translated. Three sites used to match on the rendered label
/// (`label.starts_with("Upgrade ")` in the Updates page, `"Create "` twice in
/// the wizard); that worked only because the label happened to be an English
/// literal. Once `fl!` produces it, the prefix is English only in the fallback
/// locale, so every upgrade task would vanish from the Updates page and the
/// wizard would lose its progress task the moment a translation landed. Match
/// on this instead; never on `label`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskKind {
    Upgrade,
    Create,
    Other,
}

pub struct TaskView {
    pub label: String,
    pub kind: TaskKind,
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    pub started_at: std::time::Instant,
}

impl TaskView {
    fn push_lines(&mut self, lines: Vec<String>) {
        self.output.extend(lines);
        if self.output.len() > MAX_TASK_OUTPUT_LINES {
            let drain_to = self.output.len() - MAX_TASK_OUTPUT_LINES;
            self.output.drain(0..drain_to);
        }
    }
}

impl App {
    fn active_page(&self) -> Page {
        active_page(&self.nav_model)
    }

    /// No-op task.
    fn none() -> Task<Message> {
        Task::none()
    }

    /// Synchronous follow-up message (explicit `Action::App` form — never
    /// fights inference, §2.3).
    fn done(m: Message) -> Task<Message> {
        Task::done(cosmic::Action::App(m))
    }

    /// Destructive-confirm gate (§5.3 `confirm_destructive_actions`): open
    /// the dialog, or — when the user disabled confirms — dispatch the
    /// follow-up immediately as if confirmed. Non-destructive specs always
    /// open (the key gates destructive class only).
    fn confirm_or_run(&mut self, spec: ConfirmSpec) -> Task<Message> {
        let destructive = spec.destructive;
        let enabled = self
            .config
            .as_ref()
            .map(|c| c.confirm_destructive_actions)
            .unwrap_or(true);
        if destructive && !enabled {
            self.dialog = None;
            return self.dispatch_confirm_action(spec.action);
        }
        self.dialog = Some(ActiveDialog::Confirm(spec));
        Self::none()
    }

    /// Push a toast; returns the auto-dismiss follow-up task.
    fn toast(&mut self, text: String) -> Task<Message> {
        views::push_toast(&mut self.toasts, text)
    }

    /// Run an async backend call. THE ONLY PLACE `Backend` methods run (§0.2).
    fn run<F>(f: F) -> Task<Message>
    where
        F: std::future::Future<Output = Message> + Send + 'static,
    {
        cosmic::task::future(f)
    }

    fn refresh_containers(backend: &Arc<Backend>) -> Task<Message> {
        let backend = Arc::clone(backend);
        Self::run(async move {
            let result = backend.containers().await;
            Message::Containers(ContainerMsg::Loaded(result))
        })
    }

    fn load_images(backend: &Arc<Backend>) -> Task<Message> {
        let backend = Arc::clone(backend);
        Self::run(async move {
            let result = backend.images().await;
            Message::Images(ImageMsg::Loaded(result))
        })
    }

    fn error_text(err: &CoreFailure) -> String {
        match &*err.0 {
            // `command`/`stderr` are command output → placeables, never part
            // of the message body (Q17).
            CoreError::BlockedEnvironment => {
                fl!("app-blocked-environment")
            }
            CoreError::CommandFailed {
                command, stderr, ..
            } => fl!("app-command-failed", command = command, stderr = stderr),
            other => other.to_string(),
        }
    }

    /// Images page (rows #97–#105): the distro CATALOGUE
    /// (`distrobox create --compatibility`), not local images (row #104 —
    /// the headline says so). Search + grid + custom URL + details dialog.
    fn view_images(&self) -> cosmic::Element<'_, Message> {
        crate::images_view::view_images(
            &self.images,
            self.loading.images,
            self.images_error.clone(),
            &self.images_search,
            &self.images_custom,
        )
    }

    /// Settings page (T12, rows #163–#171): system info, quick actions,
    /// preferences (cosmic-config, best-effort), danger zone, about.
    fn view_settings_page(&self) -> cosmic::Element<'_, Message> {
        use crate::settings as st;
        let mut col = widget::Column::new().spacing(16);
        col = col.push(st::system_info(
            &self.distrobox_version,
            self.loading_version,
            self.containers.len(),
            crate::message::running_count(&self.containers),
            self.backend.is_distrobox_installed(),
        ));
        // Quick actions (rows #165–#168).
        col = col.push(widget::text::caption_heading(fl!("app-quick-actions")));
        col = col.push({
            let row: cosmic::Element<'_, Message> = widget::Row::new()
                .push(
                    widget::button::standard(fl!("app-refresh-all-data")).on_press(
                        Message::Settings(crate::message::SettingsMsg::RefreshAllRequested),
                    ),
                )
                .push(
                    widget::button::standard(fl!("app-stop-all-containers")).on_press(
                        Message::Settings(crate::message::SettingsMsg::StopAllRequested),
                    ),
                )
                .spacing(12)
                .into();
            row
        });
        col = col.push({
            let row: cosmic::Element<'_, Message> = widget::Row::new()
                .push(
                    widget::button::standard(fl!("app-upgrade-all-containers")).on_press(
                        Message::Settings(crate::message::SettingsMsg::UpgradeAllRequested),
                    ),
                )
                .push(
                    widget::button::standard(fl!("app-clear-completed-tasks")).on_press(
                        Message::Settings(crate::message::SettingsMsg::ClearCompleted),
                    ),
                )
                .spacing(12)
                .into();
            row
        });
        // Preferences (row #171): config or degrade notice.
        match self.config.as_ref() {
            Some(cfg) => {
                col = col.push(crate::settings::preferences(cfg, &self.terminal_list()));
            }
            None => {
                col = col.push(crate::settings::config_unavailable());
            }
        }
        // Danger zone (row #170).
        col = col.push(crate::settings::danger_zone(!self.containers.is_empty()));
        // About (row #169, I5): `widget::about()` owns the card.
        col = col.push(widget::text::caption_heading(fl!("app-about-heading")));
        col = col.push(st::about());
        widget::scrollable(col).into()
    }

    /// Apps page (T12, rows #172–#179): full catalogue view over the
    /// T3 mirrors (selected container). Binary dialog through the modal slot.
    fn view_apps_page(&self) -> cosmic::Element<'_, Message> {
        crate::apps_view::view_apps_page(
            self.selected_container.as_deref(),
            &self.apps,
            &self.exported_binaries,
            self.loading.apps || self.loading.binaries,
            self.apps_error.as_deref(),
            &crate::apps_view::AppsViewState {
                search: self.apps_search.clone(),
            },
        )
    }

    fn view_stats(&self) -> cosmic::Element<'_, Message> {
        let Some(selected) = self.selected_container.clone() else {
            return widget::container(widget::text::body(fl!("app-stats-select")))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        if self.loading.stats {
            return widget::container(widget::text::body(fl!(
                "app-stats-loading",
                name = selected
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }
        let Some(stats) = &self.stats else {
            return widget::container(widget::text::body(fl!("app-stats-none", name = selected)))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        widget::Column::new()
            .push(widget::text::title3(fl!(
                "app-stats-title",
                name = selected
            )))
            // The measured values are formatted HERE and passed as placeables;
            // no `{:.1}` lives inside a translatable body.
            .push(widget::text::body(fl!(
                "app-stat-cpu",
                percent = format!("{:.1}", stats.cpu_percent)
            )))
            .push(widget::text::body(fl!(
                "app-stat-memory",
                used = stats.memory_usage.as_str(),
                limit = stats.memory_limit.as_str(),
                percent = format!("{:.1}", stats.memory_percent)
            )))
            .push(widget::text::body(fl!(
                "app-stat-network",
                value = stats.network_io.as_str()
            )))
            .push(widget::text::body(fl!(
                "app-stat-block",
                value = stats.block_io.as_str()
            )))
            .spacing(4)
            .into()
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    /// `init` runs before any subscription is polled; the env probe happens
    /// once here (not once per refresh), and the first task loads containers.
    fn init(core: Core, _: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut nav_model = nav_bar::Model::default();
        for page in Page::ALL {
            nav_model.insert().text(page.title()).data(page);
        }
        nav_model.activate_position(0);

        let backend = Arc::new(Backend::new_host());
        let blocked = is_blocked(backend.env());
        // `init` runs before any subscription is polled, so publishing the
        // backend here is race-free (§3.4: the same pattern as libcosmic's
        // `text_context_menu` OnceLock sender).
        let _ = BACKEND.set(Arc::clone(&backend));

        // Config load (T12 §5): an unavailable config dir degrades to
        // `None` (defaults render, writes toast) rather than crashing.
        let (config, legacy_import_pending) = match crate::settings::load_entry() {
            Some((cfg, needs_import)) => (Some(cfg), needs_import),
            None => (None, false),
        };
        // The legacy import probes the host; without a config handle to
        // persist into, importing would invent settings that vanish on
        // restart. Arm the gate only when there is somewhere to write.
        let legacy_import_pending = legacy_import_pending && config.is_some();

        let mut app = App {
            core,
            nav_model,
            backend: Arc::clone(&backend),
            error: None,
            containers: ContainerList::default(),
            selected_container: None,
            images: Vec::new(),
            images_error: None,
            apps_search: String::new(),
            apps_binary_path: String::new(),
            apps_binary_error: None,
            apps_error: None,
            apps_binary_dialog: false,
            packages: crate::packages::PackagesState::default(),
            backups: crate::backups::BackupsState::default(),
            activity: crate::activity::ActivityState::default(),
            terminal: crate::terminal::TerminalState::default(),
            config,
            legacy_import_pending,
            distrobox_version: fl!("app-version-unknown"),
            loading_version: true,
            images_search: String::new(),
            images_custom: String::new(),
            image_details: None,
            wizard: None,
            apps: Vec::new(),
            exported_binaries: Vec::new(),
            stats: None,
            stats_for: None,
            loading: Loading::default(),
            tasks: BTreeMap::new(),
            details_for: None,
            dialog: None,
            toasts: Toasts::new(|id| Message::Ui(UiMsg::ToastClosed(id))),
            busy: std::collections::BTreeSet::new(),
        };
        app.sync_terminals();
        app.core_mut()
            .set_header_title("Gosh Distrobox Manager".to_string());

        if blocked {
            app.error = backend.env().message.clone();
            return (app, Self::none());
        }
        app.loading.containers = true;
        app.loading_version = true;
        let b1 = Arc::clone(&backend);
        let b2 = Arc::clone(&backend);
        let mut tasks = vec![
            Self::refresh_containers(&b1),
            Self::run(async move {
                let result = b2.distrobox_version().await;
                Message::Settings(crate::message::SettingsMsg::VersionLoaded(result))
            }),
        ];
        // D11/PKG-9 one-time import: the probes are async (`gsettings`,
        // host file read) and must run through the env-mapped runner, so
        // this is a `Task` like every other backend call (§0.2) — never a
        // synchronous probe in `init`.
        if legacy_import_pending {
            let b3 = Arc::clone(&backend);
            tasks.push(Self::run(async move {
                // Owned (T17): `command_runner()` clones out from under the
                // reprobe lock, so bind it before borrowing across `.await`.
                let runner = b3.command_runner();
                let legacy = gosh_distrobox_core::import_legacy(&runner).await;
                Message::Settings(crate::message::SettingsMsg::LegacyImported(legacy))
            }));
        }
        let task = Task::batch(tasks);
        (app, task)
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav_model.activate(id);
        // Leaving Containers pops the pushed stacks (details + terminal —
        // single-level each; O2: a stale terminal.container leaks the page
        // past the tab AND dead-ends the New button).
        if self.active_page() != Page::Containers {
            self.details_for = None;
            self.terminal = crate::terminal::TerminalState::default();
        }
        // Lazy-load each domain on first visit; containers load at init.
        match self.active_page() {
            Page::Dashboard | Page::Containers => {
                if self.containers.is_empty() && !self.loading.containers {
                    self.loading.containers = true;
                    return Self::refresh_containers(&self.backend);
                }
                Self::none()
            }
            Page::Images => {
                if self.images.is_empty() && !self.loading.images {
                    self.loading.images = true;
                    return Self::load_images(&self.backend);
                }
                Self::none()
            }
            Page::Activity => Self::none(),
            Page::Settings => Self::none(),
            Page::Backups => {
                // First visit: pick a container and load snapshots.
                if self.backups.container.is_none()
                    && let Some(name) = self.containers.first().map(|c| c.name.clone())
                {
                    self.backups.container = Some(name.clone());
                    self.backups.loading = true;
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend.list_snapshots().await;
                        Message::Backups(crate::message::BackupsMsg::SnapshotsLoaded(name, result))
                    });
                }
                Self::none()
            }
            Page::Packages => {
                // First visit: pick a container (first running, else first)
                // and kick detect + list. Later visits keep state (row #7).
                if self.packages.container.is_none()
                    && let Some(name) = self
                        .containers
                        .iter()
                        .find(|c| crate::icons::is_running(&c.status))
                        .or_else(|| self.containers.first())
                        .map(|c| c.name.clone())
                {
                    return self.select_package_container(name);
                }
                Self::none()
            }
            Page::Updates => {
                // Updates reads the shared container list (loaded at init /
                // Dashboard) — refresh when empty like the other list pages.
                if self.containers.is_empty() && !self.loading.containers {
                    self.loading.containers = true;
                    return Self::refresh_containers(&self.backend);
                }
                Self::none()
            }
            Page::Apps | Page::Stats => Self::none(),
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::NavSelect(id) => return self.on_nav_select(id),
            Message::Containers(msg) => match msg {
                ContainerMsg::RefreshRequested => {
                    // T17 (row #13): a refresh from a gated state re-probes
                    // first — re-running `containers()` against a frozen
                    // guard is what made "Check Again" inert. The healthy
                    // path below is byte-identical to before.
                    if is_blocked(self.backend.env()) || !self.backend.is_distrobox_installed() {
                        return self.update(Message::Env(EnvMsg::ReprobeRequested));
                    }
                    self.loading.containers = true;
                    self.error = None;
                    return Self::refresh_containers(&self.backend);
                }
                ContainerMsg::Loaded(result) => {
                    self.loading.containers = false;
                    match result {
                        Ok(containers) => {
                            // Drop a selection that no longer exists.
                            if let Some(sel) = &self.selected_container
                                && !containers.iter().any(|c| &c.name == sel)
                            {
                                self.selected_container = None;
                                self.apps.clear();
                                self.exported_binaries.clear();
                                self.stats = None;
                                self.stats_for = None;
                            }
                            // B2: reconcile the details page against the fresh
                            // list — it renders a frozen clone, so without
                            // this a Stop/Delete leaves stale status and an
                            // enabled Stop button (or a page for a deleted
                            // container). Re-point by name; close when gone.
                            if let Some(viewing) = self.details_for.take()
                                && let Some(fresh) =
                                    containers.iter().find(|c| c.name == viewing.name).cloned()
                            {
                                self.details_for = Some(fresh);
                            }
                            // Same B2 class for the packages picker: a deleted
                            // container leaves it stuck on "Not Running".
                            // Clear everything (the Packages first-visit arm
                            // re-picks on next visit).
                            if let Some(selected) = self.packages.container.clone()
                                && !containers.iter().any(|c| c.name == selected)
                            {
                                self.packages.container = None;
                                self.packages.installed.clear();
                                self.packages.results.clear();
                                self.packages.manager = None;
                                self.packages.error = None;
                            }
                            // Same class for the backups picker + snapshot
                            // list (a deleted container leaves a dead
                            // selection and a stale list).
                            if let Some(selected) = self.backups.container.clone()
                                && !containers.iter().any(|c| c.name == selected)
                            {
                                self.backups.container = None;
                                self.backups.snapshots.clear();
                                self.backups.error = None;
                            }
                            self.containers = containers;
                            self.error = None;
                        }
                        Err(e) => self.error = Some(Self::error_text(&e)),
                    }
                }
                ContainerMsg::Selected(sel) => {
                    let name = sel.map(|c| c.name.clone());
                    // Selecting a different container invalidates the apps /
                    // binaries / stats mirrors; the loads below repopulate.
                    if name != self.selected_container {
                        self.selected_container = name.clone();
                        self.apps.clear();
                        self.exported_binaries.clear();
                        self.apps_error = None;
                        self.stats = None;
                        self.stats_for = None;
                        if let Some(container) = name {
                            self.loading.apps = true;
                            self.loading.binaries = true;
                            self.loading.stats = true;
                            let b1 = Arc::clone(&self.backend);
                            let b2 = Arc::clone(&self.backend);
                            let b3 = Arc::clone(&self.backend);
                            let c1 = container.clone();
                            let c2 = container.clone();
                            let c3 = container.clone();
                            return Task::batch(vec![
                                Self::run(async move {
                                    let result = b1.container_apps(&c1).await;
                                    Message::Apps(AppMsg::Loaded(c1, result))
                                }),
                                Self::run(async move {
                                    let result = b2.exported_binaries(&c2).await;
                                    Message::Apps(AppMsg::BinariesLoaded(c2, result))
                                }),
                                Self::run(async move {
                                    let result = b3.container_stats(&c3).await;
                                    Message::Stats(StatsMsg::Loaded(result))
                                }),
                            ]);
                        }
                    }
                }
                ContainerMsg::ActionFinished(result) => {
                    // Short-mutation result: clear busy flags, refresh the
                    // list on success, toast on error (§3.4 — no silent
                    // failures; Flutter's pkg page showed nothing on null).
                    self.busy.clear();
                    match result {
                        Ok(msg) => {
                            self.loading.containers = true;
                            let toast = self.toast(msg);
                            let refresh = Self::refresh_containers(&self.backend);
                            return Task::batch(vec![toast, refresh]);
                        }
                        Err(e) => {
                            let toast = self.toast(fl!("app-failed", error = Self::error_text(&e)));
                            return toast;
                        }
                    }
                }
                ContainerMsg::StopRequested(name) => {
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    if !self.busy.insert(format!("stop:{name}")) {
                        return Self::none();
                    }
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend
                            .stop_container(&name)
                            .await
                            .map(|_| fl!("app-container-stopped", name = name.as_str()));
                        Message::Containers(ContainerMsg::ActionFinished(result))
                    });
                }
                ContainerMsg::StartRequested(name) => {
                    // B5 (rows #68/#111/#130): true `podman start` (+ docker
                    // fallback), then
                    // the ActionFinished arm refreshes `list()` so the
                    // `Created|Exited → Up` transition is OBSERVED.
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    if !self.busy.insert(format!("start:{name}")) {
                        return Self::none();
                    }
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend
                            .start_container(&name)
                            .await
                            .map(|_| fl!("app-container-started", name = name.as_str()));
                        Message::Containers(ContainerMsg::ActionFinished(result))
                    });
                }
                ContainerMsg::RemoveRequested(name) => {
                    // Destructive → shared confirm (§3.3), not a direct run.
                    return self.confirm_or_run(ConfirmSpec {
                        title: fl!("app-delete-container"),
                        body: fl!("app-confirm-delete-container", name = name.as_str()),
                        confirm_label: fl!("app-delete"),
                        destructive: true,
                        action: ConfirmAction::RemoveContainer(name),
                    });
                }
                ContainerMsg::StopAllRequested => {
                    let count = running_count(&self.containers);
                    if count == 0 {
                        // Sibling of UpgradeAll's toast (#167). The buttons
                        // that produce this (Containers, Settings) are always
                        // enabled, so a silent return reads as a dead
                        // control.
                        let toast = self.toast(fl!("app-no-running-to-stop"));
                        return toast;
                    }
                    // No guard here: `dispatch_confirm_action` owns the
                    // "stop-all" key (as `RemoveContainer` does). Inserting
                    // it at request time made the confirmed action re-insert
                    // an already-present key, and `BTreeSet::insert` says
                    // `false` for that — so every Stop All, dialog and
                    // no-confirm path alike, bailed before spawning and left
                    // the key stuck, muting the button for the session.
                    return self.confirm_or_run(ConfirmSpec {
                        title: fl!("app-stop-all-containers"),
                        body: fl!("app-confirm-stop-all", count = count),
                        confirm_label: fl!("app-stop-all"),
                        destructive: true,
                        action: ConfirmAction::StopAll,
                    });
                }
                ContainerMsg::UpgradeRequested(name) => {
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    if self.upgrading(&ContainerInfo {
                        id: String::new(),
                        name: name.clone(),
                        status: gosh_distrobox_core::models::Status::Other(String::new()),
                        image: String::new(),
                    }) {
                        return Self::none();
                    }
                    let backend = Arc::clone(&self.backend);
                    // Routing goes by `TaskKind::Upgrade`, not by the label:
                    // the label is translated, so nothing may key off its text
                    // (T15 — the old `updates.rs` prefix filter did).
                    let label = fl!("app-label-upgrade", name = name.as_str());
                    let toast = self.toast(fl!("app-upgrading-container", name = name.as_str()));
                    let spawn = Self::run(async move {
                        let result = backend.upgrade_container(&name).await;
                        Message::Tasks(TaskMsg::Started {
                            label,
                            kind: TaskKind::Upgrade,
                            result,
                        })
                    });
                    return Task::batch(vec![toast, spawn]);
                }
                ContainerMsg::ViewAllRequested => {
                    // Row #23 (dead in Flutter): switch to Containers tab.
                    views::activate_page(&mut self.nav_model, Page::Containers);
                }
                ContainerMsg::NewContainerRequested => {
                    // Rows #28/#44: open a blank wizard (pushed over
                    // Containers like details). Preselected images arrive
                    // via the Images page (#95: card Select + custom URL),
                    // the only producer carrying an image — the
                    // header/dashboard buttons have none (both render only
                    // when details is closed, so a details preselect could
                    // never fire). Clear the terminal stack too (O2: the
                    // terminal page renders first when open, which would
                    // swallow the fresh wizard).
                    self.details_for = None;
                    self.terminal = crate::terminal::TerminalState::default();
                    if self.active_page() != Page::Containers {
                        views::activate_page(&mut self.nav_model, Page::Containers);
                    }
                    self.wizard = Some(crate::wizard::WizardState::default());
                }
                ContainerMsg::UpgradeAllRequested => {
                    // Row #29 (redirect snackbar in Flutter) + #131 (toast
                    // when nothing is running): confirm, then spawn one
                    // upgrade task per running container.
                    let running: Vec<String> = self
                        .containers
                        .iter()
                        .filter(|c| icons::is_running(&c.status))
                        .map(|c| c.name.clone())
                        .collect();
                    if running.is_empty() {
                        // Row #131: Flutter toasted 'No running containers
                        // to upgrade' — silent none would strand the header
                        // button with no feedback.
                        let toast = self.toast(fl!("app-no-running-to-upgrade"));
                        return toast;
                    }
                    return self.confirm_or_run(ConfirmSpec {
                        title: fl!("app-upgrade-all-containers"),
                        body: fl!("app-confirm-upgrade-all", count = running.len()),
                        confirm_label: fl!("app-upgrade-all"),
                        destructive: false,
                        action: ConfirmAction::UpgradeAll,
                    });
                }
            },
            Message::Details(msg) => match msg {
                DetailsMsg::OpenRequested(container) => {
                    // Row #26 (dashboard preview tap → details): the details
                    // page renders under Containers only, so switch there
                    // first — otherwise the tap is a silent no-op AND leaves
                    // a stale `details_for` for the next Containers visit.
                    views::activate_page(&mut self.nav_model, Page::Containers);
                    self.details_for = Some(container.clone());
                    // Selecting also loads apps/binaries/stats mirrors —
                    // reuse the T3 selection fan-out verbatim.
                    return self
                        .update(Message::Containers(ContainerMsg::Selected(Some(container))));
                }
                DetailsMsg::Closed => {
                    self.details_for = None;
                    self.terminal = crate::terminal::TerminalState::default();
                }
                DetailsMsg::StopRequested(name) => {
                    return self.update(Message::Containers(ContainerMsg::StopRequested(name)));
                }
                DetailsMsg::RemoveRequested(name) => {
                    return self.update(Message::Containers(ContainerMsg::RemoveRequested(name)));
                }
                DetailsMsg::UpgradeRequested(name) => {
                    return self.update(Message::Containers(ContainerMsg::UpgradeRequested(name)));
                }
                DetailsMsg::CloneRequested(source) => {
                    self.dialog = Some(ActiveDialog::Clone {
                        source: source.clone(),
                        name: format!("{source}-clone"),
                    });
                }
                DetailsMsg::CloneNameChanged(name) => {
                    if let Some(ActiveDialog::Clone { source, .. }) = &self.dialog {
                        let source = source.clone();
                        self.dialog = Some(ActiveDialog::Clone { source, name });
                    }
                }
                DetailsMsg::CloneConfirmed { source, name } => {
                    // Row #148: `.trim()` like the four siblings (the
                    // backups clone dialog was the one missing it) — a
                    // trailing space errors instead of trimming.
                    let name = name.trim().to_string();
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    match gosh_distrobox_core::models::CreateArgName::new(&name) {
                        Err(_) => {
                            // `{name:?}` used to add surrounding quotes — the
                            // message body now owns the quotes.
                            let toast = self.toast(fl!("app-clone-name-invalid", name = name));
                            return toast;
                        }
                        Ok(arg_name) => {
                            use gosh_distrobox_core::models::CreateArgs;
                            let backend = Arc::clone(&self.backend);
                            let label = fl!("app-label-clone", name = name.as_str());
                            let args = CreateArgs {
                                init: false,
                                nvidia: false,
                                home_path: None,
                                image: String::new(),
                                name: arg_name,
                                volumes: vec![],
                            };
                            self.dialog = None;
                            let toast = self.toast(fl!(
                                "app-cloning-container",
                                source = source.as_str(),
                                name = name.as_str()
                            ));
                            let spawn = Self::run(async move {
                                let result = backend.clone_container(&source, args).await;
                                Message::Tasks(TaskMsg::Started {
                                    label,
                                    kind: TaskKind::Other,
                                    result,
                                })
                            });
                            return Task::batch(vec![toast, spawn]);
                        }
                    }
                }
                DetailsMsg::CopyImageRequested(image) => {
                    // B1: `clipboard::write` is a `task::effect` — dropping
                    // it discards the write while the toast still claims
                    // success. Return the effect so the runtime executes it;
                    // the toast follows as the effect's message. Batch order:
                    // clipboard first, toast second.
                    let write = cosmic::iced::clipboard::write(image.clone());
                    let toast = Self::done(Message::Ui(UiMsg::CopiedToClipboard(image)));
                    return Task::batch(vec![write, toast]);
                }
                DetailsMsg::AppsRequested(name) => {
                    // Full Apps page lands in T12 — select + switch to the
                    // Apps tab so the existing Apps view shows this
                    // container's apps (named `activate_page`, never a
                    // positional literal).
                    if let Some(c) = self.containers.iter().find(|c| c.name == *name).cloned() {
                        views::activate_page(&mut self.nav_model, Page::Apps);
                        return self.update(Message::Containers(ContainerMsg::Selected(Some(c))));
                    }
                }
                DetailsMsg::TerminalRequested(name) => {
                    // Rows #54/#63: open the terminal page (pushed over
                    // Containers; Back pops). Reuses the terminal router so
                    // enter-command loading is shared.
                    return self.update_terminal(crate::message::TerminalMsg::OpenRequested(name));
                }
            },
            Message::Dialog(msg) => match msg {
                DialogMsg::Cancelled => {
                    self.dialog = None;
                }
                DialogMsg::Confirmed => {
                    let Some(dialog) = self.dialog.take() else {
                        return Self::none();
                    };
                    match dialog {
                        ActiveDialog::Confirm(spec) => {
                            return self.dispatch_confirm_action(spec.action);
                        }
                        ActiveDialog::Clone { .. } => {
                            // Clone uses its own primary button
                            // (CloneConfirmed); confirming a stale dialog
                            // state is a no-op.
                            return Self::none();
                        }
                    }
                }
            },
            Message::Packages(msg) => return self.update_packages(msg),
            Message::Backups(msg) => return self.update_backups(msg),
            Message::Activity(msg) => return self.update_activity(msg),
            Message::Updates(_) => return Self::none(), // namespace reserved (T9+)
            Message::Terminal(msg) => return self.update_terminal(msg),
            Message::Apps(msg) => match msg {
                AppMsg::Loaded(container, result) => {
                    // Stale-response guard (T8 pattern, and load-bearing here
                    // because every export triggers a re-sync that can land
                    // after the user has moved to another container).
                    if !self.apps_reply_is_current(&container) {
                        return Self::none();
                    }
                    self.loading.apps = false;
                    match result {
                        Ok(apps) => {
                            self.apps = apps;
                            self.apps_error = None;
                        }
                        Err(e) => self.apps_error = Some(Self::error_text(&e)),
                    }
                }
                AppMsg::BinariesLoaded(container, result) => {
                    if !self.apps_reply_is_current(&container) {
                        return Self::none();
                    }
                    self.loading.binaries = false;
                    match result {
                        Ok(bins) => {
                            self.exported_binaries = bins;
                            self.apps_error = None;
                        }
                        Err(e) => self.apps_error = Some(Self::error_text(&e)),
                    }
                }
                AppMsg::SearchChanged(q) => {
                    self.apps_search = q;
                }
                AppMsg::BinaryDialogRequested => {
                    // Row #175: path field starts empty; LOUD on submit.
                    self.apps_binary_path.clear();
                    self.apps_binary_error = None;
                    self.apps_binary_dialog = true;
                }
                AppMsg::BinaryDialogClosed => {
                    self.apps_binary_dialog = false;
                    self.apps_binary_error = None;
                }
                AppMsg::BinaryPathChanged(p) => {
                    self.apps_binary_path = p;
                    self.apps_binary_error = None;
                }
                AppMsg::BinaryExportConfirmed => {
                    // Row #179: LOUD on empty (Flutter silently returned).
                    let (container, path) = match (
                        self.selected_container.clone(),
                        self.apps_binary_path.trim().to_string(),
                    ) {
                        (Some(c), p) if !p.is_empty() => (c, p),
                        _ => {
                            self.apps_binary_error = Some(fl!("app-binary-path-required"));
                            return Self::none();
                        }
                    };
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    self.apps_binary_dialog = false;
                    let backend = Arc::clone(&self.backend);
                    let c = container.clone();
                    return Self::run(async move {
                        let result = backend
                            .export_binary(&container, &path)
                            .await
                            .map(|_| fl!("app-binary-exported", path = path));
                        Message::Apps(AppMsg::ActionFinished(c, result))
                    });
                }
                AppMsg::ReloadRequested(container) => {
                    // Full spinner only when there is nothing to keep. A
                    // re-sync after an export must NOT blank the page: the
                    // header Refresh and the post-export reload both land
                    // here, and `view_apps_page` early-returns on `loading`,
                    // so flipping a toggle would otherwise drop the whole
                    // grid (search box included) to "Loading apps…" and back.
                    let first_load = self.apps.is_empty() && self.apps_error.is_none();
                    self.loading.apps = first_load;
                    self.loading.binaries = first_load;
                    self.apps_error = None;
                    let b1 = Arc::clone(&self.backend);
                    let b2 = Arc::clone(&self.backend);
                    let c1 = container.clone();
                    let c2 = container.clone();
                    return Task::batch(vec![
                        Self::run(async move {
                            let result = b1.container_apps(&c1).await;
                            Message::Apps(AppMsg::Loaded(c1, result))
                        }),
                        Self::run(async move {
                            let result = b2.exported_binaries(&c2).await;
                            Message::Apps(AppMsg::BinariesLoaded(c2, result))
                        }),
                    ]);
                }
                AppMsg::ExportRequested(container, desktop) => {
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    // Same re-press guard as every other short mutation:
                    // flipping the toggle twice would otherwise run two
                    // `distrobox export` invocations for one app.
                    if !self.busy.insert(format!("export:{container}:{desktop}")) {
                        return Self::none();
                    }
                    let backend = Arc::clone(&self.backend);
                    let c = container.clone();
                    return Self::run(async move {
                        let result = backend
                            .export_app(&container, &desktop)
                            .await
                            .map(|_| fl!("app-application-exported"));
                        Message::Apps(AppMsg::ActionFinished(c, result))
                    });
                }
                AppMsg::UnexportRequested(container, desktop) => {
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    if !self.busy.insert(format!("unexport:{container}:{desktop}")) {
                        return Self::none();
                    }
                    let backend = Arc::clone(&self.backend);
                    let c = container.clone();
                    return Self::run(async move {
                        let result = backend
                            .unexport_app(&container, &desktop)
                            .await
                            .map(|_| fl!("app-application-unexported"));
                        Message::Apps(AppMsg::ActionFinished(c, result))
                    });
                }
                AppMsg::ActionFinished(container, result) => {
                    // Mirrors `ContainerMsg::ActionFinished` (clear guards,
                    // toast, no silent failures) but reloads the APP list —
                    // the export toggle's EXPORTED label and count come from
                    // re-reading the container, so containers alone are not
                    // enough (#178). Reload regardless of outcome: on error
                    // the UI re-syncs to the container's real state instead
                    // of keeping the flip the user just made.
                    self.busy.clear();
                    let toast = match result {
                        Ok(msg) => self.toast(msg),
                        Err(e) => self.toast(fl!("app-failed", error = Self::error_text(&e))),
                    };
                    let reload = self.update(Message::Apps(AppMsg::ReloadRequested(container)));
                    return Task::batch(vec![toast, reload]);
                }
            },
            Message::Settings(msg) => return self.update_settings(msg),
            Message::Images(msg) => match msg {
                ImageMsg::LoadRequested => {
                    self.loading.images = true;
                    self.images_error = None;
                    return Self::load_images(&self.backend);
                }
                ImageMsg::Loaded(result) => {
                    self.loading.images = false;
                    match result {
                        Ok(images) => {
                            self.images = images;
                            self.images_error = None;
                        }
                        Err(e) => {
                            self.images_error = Some(Self::error_text(&e));
                        }
                    }
                }
                ImageMsg::SearchChanged(q) => {
                    self.images_search = q;
                }
                ImageMsg::CustomChanged(u) => {
                    self.images_custom = u;
                }
                ImageMsg::CustomSubmitted => {
                    // Row #103 (dead in Flutter): custom URL → wizard with
                    // preselected image. Empty field = inline toast, not a
                    // silent snackbar-to-nowhere.
                    let url = self.images_custom.trim().to_string();
                    if url.is_empty() {
                        let toast = self.toast(fl!("app-custom-image-url-first"));
                        return toast;
                    }
                    self.wizard = Some(crate::wizard::WizardState::with_preselected(url));
                    self.details_for = None;
                    views::activate_containers(&mut self.nav_model);
                }
                ImageMsg::DetailsRequested(image) => {
                    self.image_details = Some(image);
                }
                ImageMsg::DetailsClosed => {
                    self.image_details = None;
                }
                ImageMsg::CreateWithImage(image) => {
                    // Row #102 (dead in Flutter): details dialog "Create
                    // Container" pushes the wizard with preselectedImage.
                    self.image_details = None;
                    self.wizard = Some(crate::wizard::WizardState::with_preselected(image));
                    self.details_for = None;
                    views::activate_containers(&mut self.nav_model);
                }
            },
            Message::Wizard(msg) => return self.update_wizard(msg),
            Message::Stats(msg) => match msg {
                StatsMsg::LoadRequested(container) => {
                    self.loading.stats = true;
                    self.stats_for = Some(container.clone());
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend.container_stats(&container).await;
                        Message::Stats(StatsMsg::Loaded(result))
                    });
                }
                StatsMsg::Loaded(result) => {
                    self.loading.stats = false;
                    match result {
                        Ok(stats) => self.stats = Some(stats),
                        Err(e) => self.error = Some(Self::error_text(&e)),
                    }
                }
            },
            Message::Tasks(msg) => match msg {
                TaskMsg::Started {
                    label,
                    kind,
                    result,
                } => match result {
                    Err(e) => {
                        // Row #122 (ux.md:504 "Route to toaster"): spawn
                        // failures toast AND banner — Flutter showed nothing.
                        let toast = self.toast(fl!(
                            "app-could-not-start",
                            label = label.as_str(),
                            error = Self::error_text(&e)
                        ));
                        // O2 second leg: a spawn failure must not strand the
                        // wizard on Progress (no task exists to drive it).
                        // Step back to Config with the error inline (the
                        // toast above already fired — no silent failure).
                        if let Some(w) = self.wizard.as_mut()
                            && w.step == crate::wizard::WizardStep::Progress
                            && kind == TaskKind::Create
                        {
                            w.step = crate::wizard::WizardStep::Config;
                            w.inline_error = Some(fl!(
                                "app-could-not-start-creation",
                                error = Self::error_text(&e)
                            ));
                            w.task_id = None;
                        } else {
                            self.error = Some(Self::error_text(&e));
                        }
                        return toast;
                    }
                    Ok(id) => {
                        // Wire the wizard progress step (row #93): a create
                        // task whose label matches the wizard's pending label
                        // attaches its mirror id for console + cancel.
                        if let Some(w) = self.wizard.as_mut()
                            && w.step == crate::wizard::WizardStep::Progress
                            && w.task_id.is_none()
                            && kind == TaskKind::Create
                        {
                            w.task_id = Some(id);
                        }
                        self.tasks.insert(
                            id,
                            TaskView {
                                label,
                                kind,
                                output: Vec::new(),
                                completed: false,
                                success: false,
                                started_at: std::time::Instant::now(),
                            },
                        );
                    }
                },
                TaskMsg::Output { id, lines } => {
                    if let Some(view) = self.tasks.get_mut(&id) {
                        view.push_lines(lines);
                    }
                }
                TaskMsg::Completed { id, success } => {
                    // Rows #20/#21 + §3.4: a failed task must not render a
                    // success check — toast the outcome so failure is
                    // visible even before T11's activity log.
                    if let Some(view) = self.tasks.get_mut(&id) {
                        view.completed = true;
                        view.success = success;
                    }
                    // O2 latch: the wizard progress step survives sweep.
                    if let Some(w) = self.wizard.as_mut()
                        && w.task_id == Some(id)
                    {
                        w.task_completed = true;
                        w.task_success = success;
                    }
                    if !success {
                        let label = self
                            .tasks
                            .get(&id)
                            .map(|v| v.label.clone())
                            .unwrap_or_else(|| fl!("app-task"));
                        let toast = self.toast(fl!("app-task-failed", label = label.as_str()));
                        return toast;
                    }
                }
                TaskMsg::CancelRequested(id) => {
                    // Synchronous registry call — `cancel` takes no lock
                    // across blocking calls, so this is safe in `update`.
                    // Shared latch helper: every cancellation path marks the
                    // mirror AND latches wizard completion, so no trigger
                    // (row Cancel, wizard Cancel, sweep) strands the UI.
                    if self.backend.cancel_task(id) {
                        self.latch_cancelled(id);
                        return Self::done(Message::Tasks(TaskMsg::Cancelled(id)));
                    }
                }
                TaskMsg::Cancelled(id) => {
                    // Mirror already marked in `CancelRequested`. If the
                    // task was swept before cancel landed, close a drawer
                    // watching the gone id (B2 class).
                    if self.activity.expanded == Some(id) {
                        self.activity.expanded = None;
                    }
                }
                TaskMsg::ClearCompleted => {
                    self.tasks.retain(|_, v| !v.completed);
                    // A drawer open on a cleared task closes (same B2
                    // class as Expired/Cancelled above).
                    if let Some(id) = self.activity.expanded
                        && !self.tasks.contains_key(&id)
                    {
                        self.activity.expanded = None;
                    }
                }
                TaskMsg::Expired(ids) => {
                    for id in ids {
                        self.tasks.remove(&id);
                        // A swept drawer closes (same B2 class — the guard
                        // above only covers the cancel path).
                        if self.activity.expanded == Some(id) {
                            self.activity.expanded = None;
                        }
                        // O2: the sweep must not orphan the wizard progress
                        // step — a swept create task would flip success back
                        // to "Creating…" with only a dead Cancel. The mirror
                        // entry is gone but completion already arrived via
                        // `Completed` (latched below), so just drop the id.
                        if let Some(w) = self.wizard.as_mut()
                            && w.task_id == Some(id)
                        {
                            w.task_id = None;
                        }
                    }
                }
                TaskMsg::ExpiredTick => {
                    // The subscription tick cannot carry the sweep result
                    // (plain-`fn` builders capture nothing), so sweep here in
                    // `update` — synchronous, lock-free across blocking calls
                    // — and route the evidence through `Expired`, the same
                    // message a future core-driven sweep would send. One arm
                    // handles both, so the evidence path is tested even
                    // before any second producer exists.
                    let evicted = match BACKEND.get() {
                        Some(b) => b.sweep_expired(),
                        None => Vec::new(),
                    };
                    if !evicted.is_empty() {
                        return Self::done(Message::Tasks(TaskMsg::Expired(evicted)));
                    }
                }
            },
            Message::Env(msg) => match msg {
                EnvMsg::ReprobeRequested => {
                    // Row #13: the gate's "Check Again" (+ any gated
                    // Refresh). The blocking `reprobe()` runs in the `Task`
                    // future (§0.2), never synchronously — the answer comes
                    // back as `Probed`.
                    self.loading.containers = true;
                    self.error = None;
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let guard = backend.reprobe(&CommandRunner::new_real());
                        Message::Env(EnvMsg::Probed(guard))
                    });
                }
                EnvMsg::Probed(guard) => {
                    self.loading.containers = false;
                    match views::reprobe_outcome(&guard) {
                        views::ReprobeOutcome::Recovered => {
                            self.error = None;
                            self.loading.containers = true;
                            self.loading_version = true;
                            let b1 = Arc::clone(&self.backend);
                            let b2 = Arc::clone(&self.backend);
                            return Task::batch(vec![
                                Self::refresh_containers(&b1),
                                Self::run(async move {
                                    let result = b2.distrobox_version().await;
                                    Message::Settings(crate::message::SettingsMsg::VersionLoaded(
                                        result,
                                    ))
                                }),
                            ]);
                        }
                        views::ReprobeOutcome::StillGated { error } => {
                            self.error = error;
                        }
                    }
                }
            },
            Message::Ui(msg) => match msg {
                UiMsg::DismissError => self.error = None,
                UiMsg::ToastClosed(id) => {
                    self.toasts.remove(id);
                }
                UiMsg::CopiedToClipboard(what) => {
                    let toast = self.toast(fl!("app-copied-to-clipboard", what = what));
                    return toast;
                }
            },
        }
        Self::none()
    }

    /// T5/T12 subscriptions (§3.4): (a) per-task output streams, keyed by
    /// `TaskId` so iced tears each down when the task leaves `self.tasks`;
    /// (b) the TTL sweep tick; (c) the config watch (§5.1-2). The tick
    /// carries nothing — iced subscriptions cannot borrow `self.backend`
    /// (the builder is a plain `fn`), so the sweep runs in the `ExpiredTick`
    /// arm via `BACKEND`, and the resulting ids flow back as
    /// `TaskMsg::Expired`.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            Subscription::batch(self.tasks.keys().copied().map(task_output_subscription)),
            cosmic::iced::time::every(std::time::Duration::from_secs(TASK_SWEEP_INTERVAL_SECS))
                .map(|_| Message::Tasks(TaskMsg::ExpiredTick)),
            // §5.1-2 reactive reload: the watcher emits the full entry after
            // any key change (including ours). `ConfigChanged` no-ops on an
            // equal value, so our own writes do not echo. `watch_config` is
            // the documented entry point (architecture §5, D23): with
            // `dbus-config` off it resolves to cosmic-config's notify-based
            // `config_subscription`, never the absent settings daemon.
            self.watch_config::<crate::settings::PrefsEntry>(crate::settings::config_id())
                .map(|update| {
                    let cfg = gosh_distrobox_core::AppConfig::from(&update.config);
                    Message::Settings(crate::message::SettingsMsg::ConfigChanged(cfg))
                }),
        ])
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        // D17 readiness signal: the smoke test greps the journal for this
        // line, proving the app rendered (liveness alone passes for a blank
        // frozen window). Once per process — `view` runs on every state
        // change, so a std::sync::Once keeps the log to one line. Harmless in
        // production: a single info line at startup.
        use std::sync::OnceLock;
        static READY: OnceLock<()> = OnceLock::new();
        READY.get_or_init(|| {
            tracing::info!(
                "SMOKE_READY app_id={} page={:?}",
                APP_ID,
                self.active_page()
            );
        });
        // Global gates (§3.4): blocked / not-installed render INSTEAD of
        // the page, once at the shell level — every page inherits them.
        let blocked = if is_blocked(self.backend.env()) {
            self.backend.env().message.clone()
        } else {
            None
        };
        if let Some(page) = views::gate(
            blocked,
            self.backend.is_distrobox_installed(),
            self.loading.containers,
        ) {
            return views::with_toasts(&self.toasts, page);
        }
        let page = match self.active_page() {
            // Details + wizard push over Containers (rows #53/T7 back pops).
            Page::Dashboard => self.view_dashboard(),
            Page::Containers => {
                if self.terminal.container.is_some() {
                    return self.view_terminal_page();
                }
                if let Some(wizard) = &self.wizard {
                    // O2: prefer the live mirror; fall back to the latched
                    // completion when the mirror was TTL-swept (or never
                    // attached). Without the latch the success screen flips
                    // back to "Creating…" with only a dead Cancel.
                    let (output, completed, success) = wizard
                        .task_id
                        .and_then(|id| self.tasks.get(&id))
                        .map(|v| (Some(v.output.as_slice()), v.completed, v.success))
                        .unwrap_or((None, wizard.task_completed, wizard.task_success));
                    crate::wizard_view::view_wizard(
                        wizard,
                        &self.images,
                        self.loading.images,
                        output,
                        completed,
                        success,
                    )
                } else {
                    match &self.details_for {
                        Some(container) => {
                            views::view_details(container, self.upgrading(container))
                        }
                        None => self.view_containers_page(),
                    }
                }
            }
            Page::Images => self.view_images(),
            Page::Packages => self.view_packages(),
            Page::Backups => self.view_backups(),
            Page::Activity => self.view_activity_page(),
            Page::Updates => self.view_updates(),
            Page::Apps => self.view_apps_page(),
            Page::Settings => self.view_settings_page(),
            Page::Stats => self.view_stats(),
        };
        let body: cosmic::Element<'_, Self::Message> = match self.error.clone() {
            None => page,
            Some(err) => widget::Column::new()
                .push(
                    widget::Row::new()
                        .push(widget::text::body(err).width(Length::Fill))
                        .push(
                            widget::button::standard(fl!("app-dismiss"))
                                .on_press(Message::Ui(UiMsg::DismissError)),
                        )
                        .spacing(8),
                )
                .push(page)
                .spacing(8)
                .into(),
        };
        let padded = widget::container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16);
        views::with_toasts(&self.toasts, padded.into())
    }

    fn header_start(&self) -> Vec<cosmic::Element<'_, Self::Message>> {
        // Row #53 (+ terminal Back): back button on pushed pages; Refresh otherwise.
        if self.active_page() == Page::Containers
            && (self.details_for.is_some() || self.terminal.container.is_some())
        {
            // Terminal page Back clears the terminal state (its own message);
            // details Back clears details (which also resets terminal).
            let msg = if self.terminal.container.is_some() {
                Message::Terminal(crate::message::TerminalMsg::Closed)
            } else {
                Message::Details(DetailsMsg::Closed)
            };
            return vec![
                widget::button::standard(fl!("action-back"))
                    .on_press(msg)
                    .into(),
            ];
        }
        // Row #97 (T17): the header Refresh is page-aware — Images reloads
        // the catalogue (`backend.images()`), every other page reloads
        // containers. Updates and Apps own their own header actions (O6, row
        // #123; #172 — a containers reload changes nothing on a page showing
        // one container's apps), so the generic button hides there. The
        // routing decision itself is `views::header_refresh`, pinned per
        // page in `parity_rows.rs`; the pushed-stack Back above takes
        // precedence on Containers.
        match views::header_refresh(self.active_page()) {
            Some(views::HeaderRefresh::Images) => vec![
                widget::button::standard(fl!("action-refresh"))
                    .on_press(Message::Images(ImageMsg::LoadRequested))
                    .into(),
            ],
            Some(views::HeaderRefresh::Containers) => vec![
                widget::button::standard(fl!("action-refresh"))
                    .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                    .into(),
            ],
            None => vec![],
        }
    }

    /// Row #44 (FAB → header): the create affordance lives here, not in a
    /// floating button. "New Container" on the Containers page (the T7
    /// wizard consumes the request); Refresh beside it on every page.
    /// `header_end` (not `header_start`) per the shell contract — Refresh
    /// in start is the T3 leftover this replaces.
    /// Header actions (row #123): Updates gets Refresh + Upgrade All
    /// (`header_end` two buttons); Containers keeps New Container.
    fn header_end(&self) -> Vec<cosmic::Element<'_, Self::Message>> {
        match self.active_page() {
            Page::Containers if self.details_for.is_none() && self.wizard.is_none() => {
                vec![
                    widget::button::suggested(fl!("app-new-container"))
                        .on_press(Message::Containers(ContainerMsg::NewContainerRequested))
                        .into(),
                ]
            }
            Page::Updates => vec![
                widget::button::standard(fl!("action-refresh"))
                    .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                    .into(),
                widget::button::suggested(fl!("app-upgrade-all"))
                    .on_press(Message::Containers(ContainerMsg::UpgradeAllRequested))
                    .into(),
            ],
            // Row #134: the FAB's all-states bug disappears with the move —
            // header-only affordance (no empty-list dependence).
            Page::Activity => vec![
                widget::button::standard(fl!("app-clear-completed"))
                    .on_press(Message::Tasks(TaskMsg::ClearCompleted))
                    .into(),
            ],
            Page::Backups => vec![
                widget::button::suggested(fl!("app-new-snapshot"))
                    .on_press(Message::Backups(
                        crate::message::BackupsMsg::CreateDialogRequested,
                    ))
                    .into(),
            ],
            // Row #172: header refresh, enabled only with a container to
            // reload (the page itself renders "Select a container" without
            // one, so a press would have nothing to act on).
            Page::Apps => vec![
                widget::button::standard(fl!("action-refresh"))
                    .on_press_maybe(
                        self.selected_container
                            .clone()
                            .map(|c| Message::Apps(AppMsg::ReloadRequested(c))),
                    )
                    .into(),
            ],
            _ => vec![],
        }
    }

    fn context_drawer(&self) -> Option<cosmic::app::ContextDrawer<'_, Self::Message>> {
        // Row #158: full-output drawer over the activity page. Only when
        // the Activity tab is active and a task is expanded.
        if self.active_page() == Page::Activity {
            return self.activity_drawer();
        }
        None
    }

    fn dialog(&self) -> Option<cosmic::Element<'_, Self::Message>> {
        // Single modal slot (§3.3): binary export, backups dialogs, wizard
        // volume, image details, then the page-level dialog.
        if self.apps_binary_dialog {
            return Some(
                widget::dialog()
                    .title(fl!("app-export-binary"))
                    .control(crate::apps_view::binary_dialog_body(
                        &self.apps_binary_path,
                        self.apps_binary_error.as_deref(),
                    ))
                    .primary_action({
                        let export: cosmic::Element<'_, Message> =
                            widget::button::suggested(fl!("app-export"))
                                .on_press(Message::Apps(AppMsg::BinaryExportConfirmed))
                                .into();
                        export
                    })
                    .secondary_action({
                        let cancel: cosmic::Element<'_, Message> =
                            widget::button::standard(fl!("action-cancel"))
                                .on_press(Message::Apps(AppMsg::BinaryDialogClosed))
                                .into();
                        cancel
                    })
                    .into(),
            );
        }
        if self.backups.dialog.is_some() {
            return self.backups_dialog();
        }
        if let Some(wizard) = &self.wizard
            && let Some(volume) = &wizard.volume_dialog
        {
            return Some(
                widget::dialog()
                    .title(fl!("app-add-volume"))
                    .control(crate::wizard_view::volume_dialog_body(volume))
                    .primary_action({
                        let add: cosmic::Element<'_, Message> =
                            widget::button::suggested(fl!("app-add"))
                                .on_press(Message::Wizard(
                                    crate::message::WizardMsg::VolumeDialogConfirmed,
                                ))
                                .into();
                        add
                    })
                    .secondary_action({
                        let cancel: cosmic::Element<'_, Message> =
                            widget::button::standard(fl!("action-cancel"))
                                .on_press(Message::Wizard(
                                    crate::message::WizardMsg::VolumeDialogCancelled,
                                ))
                                .into();
                        cancel
                    })
                    .into(),
            );
        }
        if let Some(image) = &self.image_details {
            let (close, create) = crate::images_view::image_details_actions(image.clone());
            return Some(
                widget::dialog()
                    .title(crate::wizard::WizardState::image_display_name(image))
                    .control(crate::images_view::image_details_dialog(image))
                    .primary_action(create)
                    .secondary_action(close)
                    .into(),
            );
        }
        views::dialog_view(&self.dialog)
    }
}

impl App {
    /// Select a package container (T8): set picker + manager (Detecting…
    /// until probed) and kick detect + installed-list loads. Returns the
    /// batch of follow-up tasks.
    fn select_package_container(&mut self, name: String) -> Task<Message> {
        self.packages.container = Some(name.clone());
        self.packages.manager = None;
        self.packages.error = None;
        self.packages.search_tab = false;
        self.packages.searching = false;
        self.packages.loading = true;
        let b1 = Arc::clone(&self.backend);
        let b2 = Arc::clone(&self.backend);
        let n1 = name.clone();
        let n2 = name.clone();
        Task::batch(vec![
            Self::run(async move {
                let result = b1.detect_package_manager(&n1).await;
                Message::Packages(crate::message::PackagesMsg::ManagerDetected(n1, result))
            }),
            Self::run(async move {
                let result = b2.installed_packages(&n2).await;
                Message::Packages(crate::message::PackagesMsg::InstalledLoaded(n2, result))
            }),
        ])
    }

    /// Updates page (T9, rows #123–#132): summary + running/stopped
    /// sections + upgrade task rows from the shared mirror.
    fn view_updates(&self) -> cosmic::Element<'_, Message> {
        let task_rows = crate::updates::upgrade_task_rows(&self.tasks);
        crate::updates::view_updates(&self.containers, &|c| self.upgrading(c), task_rows)
    }

    /// Terminal page (T9, rows #66–#77): pushes over Containers like
    /// details (Back pops). Renders the open container's page, or falls
    /// back to Containers when none is open.
    fn view_terminal_page(&self) -> cosmic::Element<'_, Message> {
        if let Some(name) = &self.terminal.container
            && let Some(container) = self.containers.iter().find(|c| &c.name == name).cloned()
        {
            let terminals = self.terminal_list();
            return crate::terminal::view_terminal(
                &container,
                &self.terminal,
                &terminals,
                self.upgrading(&container),
            );
        }
        self.view_containers_page()
    }

    /// Activity page (T11, rows #152–#162) + its router + drawer.
    fn view_activity_page(&self) -> cosmic::Element<'_, Message> {
        crate::activity::view_activity(&self.tasks, &self.activity)
    }

    /// Activity message router (T11).
    fn update_activity(&mut self, msg: crate::message::ActivityMsg) -> Task<Message> {
        use crate::message::ActivityMsg;
        match msg {
            ActivityMsg::SearchChanged(s) => {
                self.activity.search = s;
                Self::none()
            }
            ActivityMsg::FilterSelected(f) => {
                self.activity.filter = f;
                Self::none()
            }
            ActivityMsg::Expanded(id) => {
                // Row #158: open the full-output drawer (only for known
                // tasks — a swept id is ignored, not crashed on).
                if self.tasks.contains_key(&id) {
                    self.activity.expanded = Some(id);
                }
                Self::none()
            }
            ActivityMsg::DrawerClosed => {
                self.activity.expanded = None;
                Self::none()
            }
        }
    }

    /// Full-output drawer (row #158): `context_drawer` over the activity
    /// page when a task is expanded. The draggable 0.5–0.95 resize range is
    /// lost (accepted per ux.md) — the drawer is fixed width.
    fn activity_drawer(&self) -> Option<cosmic::app::ContextDrawer<'_, Message>> {
        let id = self.activity.expanded?;
        let view = self.tasks.get(&id)?;
        Some(cosmic::app::context_drawer(
            crate::activity::output_drawer(view),
            Message::Activity(crate::message::ActivityMsg::DrawerClosed),
        ))
    }

    /// Backups page (T10, rows #133–#151): picker + tabs + snapshots /
    /// transfer + dialogs (rendered through the single modal slot).
    fn view_backups(&self) -> cosmic::Element<'_, Message> {
        use crate::backups as bk;
        if self.containers.is_empty() {
            // B3: an all-rows-failed list is not an empty account — see
            // `view_containers_page`. This page has no skipped caption of its
            // own, so the shared copy is the only place the reason appears.
            let (icon, title, body) = crate::views::container_list_copy(
                &self.containers,
                &fl!("app-backups-no-containers"),
            );
            return crate::views::empty_state(icon, title, body, None);
        }
        let st = &self.backups;
        let mut col = widget::Column::new().spacing(12);
        col = col.push(crate::packages::container_picker_mapped(
            &self.containers,
            st.container.as_deref(),
            |i| Message::Backups(crate::message::BackupsMsg::ContainerSelected(i)),
        ));
        // Tabs (row #133): Snapshots vs Export/Import.
        col =
            col.push(
                widget::Row::new()
                    .push(widget::button::standard(fl!("app-snapshots")).on_press(
                        Message::Backups(crate::message::BackupsMsg::TabSelected(false)),
                    ))
                    .push(widget::button::standard(fl!("app-export-import")).on_press(
                        Message::Backups(crate::message::BackupsMsg::TabSelected(true)),
                    ))
                    .spacing(12),
            );
        if st.transfer_tab {
            col = col.push(bk::transfer_tab(
                st.container.as_deref(),
                st.container.is_some(),
            ));
        } else {
            col = col.push(bk::snapshots_tab(
                st.loading,
                st.error.as_deref(),
                &st.snapshots,
                st.container.as_deref(),
            ));
        }
        widget::scrollable(col).into()
    }

    /// Install the CURRENT config's custom terminals into the backend's list
    /// (T12). Called after every load / watch update / legacy import, before
    /// anything can index the list. Cheap and idempotent: the constructor
    /// rebuilds from built-ins + customs.
    fn sync_terminals(&mut self) {
        self.backend.set_custom_terminals(
            self.config
                .as_ref()
                .map(|c| c.custom_terminals.clone())
                .unwrap_or_default(),
        );
    }

    /// Backups message router (T10, rows #133–#151).
    fn update_backups(&mut self, msg: crate::message::BackupsMsg) -> Task<Message> {
        use crate::backups::{BackupsDialog, BackupsState};
        use crate::message::BackupsMsg;
        match msg {
            BackupsMsg::ContainerSelected(i) => {
                if let Some(c) = self.containers.get(i).map(|c| c.name.clone()) {
                    self.backups.container = Some(c.clone());
                    self.backups.loading = true;
                    self.backups.error = None;
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend.list_snapshots().await;
                        Message::Backups(BackupsMsg::SnapshotsLoaded(c, result))
                    });
                }
                Self::none()
            }
            BackupsMsg::TabSelected(transfer) => {
                self.backups.transfer_tab = transfer;
                Self::none()
            }
            BackupsMsg::DeleteFinished(result) => {
                // Row #141 green/red result toasts + list reload.
                let toast = match &result {
                    Ok(_) => self.toast(fl!("app-snapshot-deleted")),
                    Err(e) => self.toast(fl!(
                        "app-could-not-delete-snapshot",
                        error = Self::error_text(e)
                    )),
                };
                self.backups.loading = true;
                let name = self.backups.container.clone().unwrap_or_default();
                let backend = Arc::clone(&self.backend);
                let reload = Self::run(async move {
                    let result = backend.list_snapshots().await;
                    Message::Backups(BackupsMsg::SnapshotsLoaded(name, result))
                });
                Task::batch(vec![toast, reload])
            }
            BackupsMsg::ReloadRequested => {
                self.backups.loading = true;
                self.backups.error = None;
                let name = self.backups.container.clone().unwrap_or_default();
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let result = backend.list_snapshots().await;
                    Message::Backups(BackupsMsg::SnapshotsLoaded(name, result))
                })
            }
            BackupsMsg::SnapshotsLoaded(name, result) => {
                // Stale-response guard (T8 InstalledLoaded pattern): only
                // the current container sticks — an out-of-order response
                // must not render A's snapshots under B.
                if self.backups.container.as_deref() != Some(&name) {
                    return Self::none();
                }
                self.backups.loading = false;
                match result {
                    Ok(list) => {
                        self.backups.snapshots = list;
                        self.backups.error = None;
                    }
                    Err(e) => {
                        self.backups.error = Some(Self::error_text(&e));
                    }
                }
                Self::none()
            }
            BackupsMsg::CreateDialogRequested => {
                // Row #140: prefilled name + helper text live in the dialog;
                // empty-name submit is LOUD (not a silent return).
                let container = match self.backups.container.clone() {
                    Some(c) => c,
                    None => {
                        let toast = self.toast(fl!("app-select-container-first"));
                        return toast;
                    }
                };
                self.backups.create_name = BackupsState::default_snapshot_name(
                    &self
                        .config
                        .as_ref()
                        .map(|c| c.snapshot_prefix.clone())
                        .unwrap_or_default(),
                    &container,
                );
                self.backups.create_error = None;
                self.backups.dialog = Some(BackupsDialog::Create);
                Self::none()
            }
            BackupsMsg::CreateNameChanged(n) => {
                self.backups.create_name = n;
                self.backups.create_error = None;
                Self::none()
            }
            BackupsMsg::CreateConfirmed => {
                // Row #140/#146: LOUD on empty (Flutter silently returned).
                let (container, name) = match (
                    self.backups.container.clone(),
                    self.backups.create_name.trim().to_string(),
                ) {
                    (Some(c), n) if !n.is_empty() => (c, n),
                    _ => {
                        self.backups.create_error = Some(fl!("app-snapshot-name-required"));
                        return Self::none();
                    }
                };
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                self.backups.dialog = None;
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let result = backend.create_snapshot(&container, &name).await;
                    Message::Backups(BackupsMsg::CreateFinished(result))
                })
            }
            BackupsMsg::CreateFinished(result) => {
                // The snapshot list reloads after create (delete already
                // did — an omitted reload strands "No snapshots yet"
                // inviting a duplicate).
                let toast = match &result {
                    Ok(_) => self.toast(fl!("app-snapshot-created")),
                    Err(e) => self.toast(fl!(
                        "app-could-not-create-snapshot",
                        error = Self::error_text(e)
                    )),
                };
                self.backups.loading = true;
                let name = self.backups.container.clone().unwrap_or_default();
                let backend = Arc::clone(&self.backend);
                let reload = Self::run(async move {
                    let result = backend.list_snapshots().await;
                    Message::Backups(BackupsMsg::SnapshotsLoaded(name, result))
                });
                Task::batch(vec![toast, reload])
            }
            BackupsMsg::DeleteRequested(id) => {
                // Row #141: shared destructive confirm (§3.3).
                let name = self
                    .backups
                    .snapshots
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.name.clone())
                    .unwrap_or(id.clone());
                self.confirm_or_run(ConfirmSpec {
                    title: fl!("app-delete-snapshot"),
                    body: fl!("app-confirm-delete-snapshot", name = name),
                    confirm_label: fl!("app-delete"),
                    destructive: true,
                    action: ConfirmAction::DeleteSnapshot(id),
                })
            }
            BackupsMsg::RestoreDialogRequested(snapshot) => {
                // Row #142: prefilled new name.
                self.backups.restore = Some((snapshot.clone(), "restored-container".to_string()));
                self.backups.restore_error = None;
                self.backups.dialog = Some(BackupsDialog::Restore(snapshot));
                Self::none()
            }
            BackupsMsg::RestoreNameChanged(n) => {
                if let Some((_, name)) = self.backups.restore.as_mut() {
                    *name = n;
                }
                self.backups.restore_error = None;
                Self::none()
            }
            BackupsMsg::RestoreConfirmed => {
                // Rows #142/#146: LOUD on empty (Flutter silently returned);
                // failure toasts via Started Err (#147).
                let (snapshot, name) = match self.backups.restore.clone() {
                    Some((s, n)) if !n.trim().is_empty() => (s, n.trim().to_string()),
                    _ => {
                        self.backups.restore_error = Some(fl!("app-new-container-name-required"));
                        return Self::none();
                    }
                };
                // Row #148: `.trim()` consistency (the backups clone dialog
                // was the one missing it).
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                self.backups.dialog = None;
                let backend = Arc::clone(&self.backend);
                let label = fl!(
                    "app-label-restore",
                    snapshot = snapshot.as_str(),
                    name = name.as_str()
                );
                Self::run(async move {
                    let result = backend.restore_from_snapshot(&snapshot, &name).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Other,
                        result,
                    })
                })
            }
            BackupsMsg::ExportDialogRequested => {
                // Row #143: prefilled path; the portal picker replaces
                // free-text (#149) at confirm time — Browse opens it now.
                if let Some(container) = self.backups.container.clone() {
                    self.backups.export_path = BackupsState::default_export_path(
                        &self
                            .config
                            .as_ref()
                            .map(|c| c.default_export_dir.clone())
                            .unwrap_or_default(),
                        &container,
                    );
                    self.backups.export_error = None;
                    self.backups.dialog = Some(BackupsDialog::Export);
                } else {
                    let toast = self.toast(fl!("app-select-container-first"));
                    return toast;
                }
                Self::none()
            }
            BackupsMsg::ExportPathChanged(p) => {
                self.backups.export_path = p;
                self.backups.export_error = None;
                Self::none()
            }
            BackupsMsg::ExportBrowseRequested => {
                // P0 portal save chooser (§4.3): runs async; the result
                // returns as `ExportPathPicked` (Cancel is silent).
                Self::run(async move {
                    let path = portal_save_suggested("export.tar").await;
                    Message::Backups(BackupsMsg::ExportPathPicked(path))
                })
            }
            BackupsMsg::ExportPathPicked(result) => {
                // Cancel: silent by portal convention.
                if let Ok(path) = result {
                    self.backups.export_path = path;
                    self.backups.export_error = None;
                }
                Self::none()
            }
            BackupsMsg::ExportConfirmed => {
                // Rows #143/#146/#147: LOUD on empty; failure toasts.
                let (container, path) = match (
                    self.backups.container.clone(),
                    self.backups.export_path.trim().to_string(),
                ) {
                    (Some(c), p) if !p.is_empty() => (c, p),
                    _ => {
                        self.backups.export_error = Some(fl!("app-output-path-required"));
                        return Self::none();
                    }
                };
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                self.backups.dialog = None;
                let backend = Arc::clone(&self.backend);
                let label = fl!(
                    "app-label-export",
                    container = container.as_str(),
                    path = path.as_str()
                );
                Self::run(async move {
                    let result = backend.export_container(&container, &path).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Other,
                        result,
                    })
                })
            }
            BackupsMsg::ImportDialogRequested => {
                // Row #144: archive path + image name, both LOUD on empty.
                self.backups.import_path.clear();
                self.backups.import_image.clear();
                self.backups.import_error = None;
                self.backups.dialog = Some(BackupsDialog::Import);
                Self::none()
            }
            BackupsMsg::ImportPathChanged(p) => {
                self.backups.import_path = p;
                self.backups.import_error = None;
                Self::none()
            }
            BackupsMsg::ImportImageChanged(i) => {
                self.backups.import_image = i;
                self.backups.import_error = None;
                Self::none()
            }
            BackupsMsg::ImportPathPicked(result) => {
                if let Ok(path) = result {
                    self.backups.import_path = path;
                    self.backups.import_error = None;
                }
                Self::none()
            }
            BackupsMsg::ImportBrowseRequested => Self::run(async move {
                let path = portal_open_archive().await;
                Message::Backups(BackupsMsg::ImportPathPicked(path))
            }),
            BackupsMsg::ImportConfirmed => {
                let (path, image) = match (
                    self.backups.import_path.trim().to_string(),
                    self.backups.import_image.trim().to_string(),
                ) {
                    (p, i) if !p.is_empty() && !i.is_empty() => (p, i),
                    _ => {
                        self.backups.import_error = Some(fl!("app-archive-and-image-required"));
                        return Self::none();
                    }
                };
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                self.backups.dialog = None;
                let backend = Arc::clone(&self.backend);
                let label = fl!(
                    "app-label-import",
                    path = path.as_str(),
                    image = image.as_str()
                );
                Self::run(async move {
                    let result = backend.import_container(&path, &image).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Other,
                        result,
                    })
                })
            }
            BackupsMsg::CloneDialogRequested => {
                // Row #145: unified with the details clone (ux.md §4.5) — opens
                // the SHARED `ActiveDialog::Clone` (same `-clone` default,
                // same `.trim()` validation #148, same confirm path).
                if let Some(container) = self.backups.container.clone() {
                    self.dialog = Some(ActiveDialog::Clone {
                        source: container.clone(),
                        name: format!("{container}-clone"),
                    });
                } else {
                    let toast = self.toast(fl!("app-select-container-first"));
                    return toast;
                }
                Self::none()
            }
            BackupsMsg::DialogCancelled => {
                self.backups.dialog = None;
                Self::none()
            }
        }
    }

    /// Package page view (T8, rows #106–#122).
    fn view_packages(&self) -> cosmic::Element<'_, Message> {
        use crate::packages as pkg;
        // Row #108: no-containers gate (Flutter `_buildNoContainersView`).
        // Without it an empty tree renders "Container Not Running" for a
        // container that does not exist — actively misleading.
        if self.containers.is_empty() {
            // B3: see `view_containers_page` — the create-prompt body is only
            // truthful when the list is *cleanly* empty.
            let (icon, title, body) = crate::views::container_list_copy(
                &self.containers,
                &fl!("app-packages-no-containers"),
            );
            return crate::views::empty_state(icon, title, body, None);
        }
        let st = &self.packages;
        let running = st
            .container
            .as_deref()
            .and_then(|n| self.containers.iter().find(|c| c.name == n))
            .map(|c| crate::icons::is_running(&c.status))
            .unwrap_or(false);
        let mut col = widget::Column::new().spacing(12);
        col = col.push(pkg::container_picker(
            &self.containers,
            st.container.as_deref(),
        ));
        if !running && st.container.is_some() {
            col = col.push(pkg::not_running_banner());
        }
        // Search bar (#112): Enter-triggered, disabled when stopped.
        col = col.push({
            let search: cosmic::Element<'_, Message> =
                widget::text_input::search_input(fl!("app-search-packages"), st.query.clone())
                    .on_input(|s| Message::Packages(crate::message::PackagesMsg::QueryChanged(s)))
                    .on_submit(|_| Message::Packages(crate::message::PackagesMsg::SearchSubmitted))
                    .into();
            search
        });
        // PM badge (#114) + clear-search (#113).
        col = col.push({
            let row: cosmic::Element<'_, Message> =
                widget::Row::new()
                    .push(widget::text::caption(fl!(
                        "app-package-manager",
                        manager = pkg::PackagesState::badge(st.manager)
                    )))
                    .push(widget::button::text(fl!("app-clear-search")).on_press(
                        Message::Packages(crate::message::PackagesMsg::SearchCleared),
                    ))
                    .spacing(8)
                    .into();
            row
        });
        // Quick actions: Install-from-box (#115, disabled when empty) +
        // Upgrade All + confirm (#116).
        col = col.push({
            let install_empty = st.query.trim().is_empty();
            let row: cosmic::Element<'_, Message> = widget::Row::new()
                .push(widget::button::standard(fl!("app-install")).on_press_maybe(
                    if running && !install_empty {
                        Some(Message::Packages(
                            crate::message::PackagesMsg::InstallFromBox,
                        ))
                    } else {
                        None
                    },
                ))
                .push(
                    widget::button::standard(fl!("app-upgrade-all")).on_press_maybe(if running {
                        Some(Message::Packages(
                            crate::message::PackagesMsg::UpgradeAllRequested,
                        ))
                    } else {
                        None
                    }),
                )
                .spacing(12)
                .into();
            row
        });
        // Manual command for Unknown PM (B1).
        if st.manager == Some(gosh_distrobox_core::models::PackageManager::Unknown) && running {
            col = col.push(pkg::manual_cmd_box(&st.manual_cmd));
        }
        // Tabs (#107) + lists (#117–#118).
        col = col.push(pkg::tab_bar(st.search_tab));
        if st.search_tab {
            col = col.push(pkg::search_list(st.searching, st.loading, &st.results));
        } else {
            col = col.push(pkg::installed_list(
                running,
                st.loading,
                st.error.as_deref(),
                &st.installed,
                st.container.as_deref().unwrap_or(""),
            ));
        }
        widget::scrollable(col).into()
    }

    /// Terminal message router (T9, rows #66–#77 + D8).
    fn update_terminal(&mut self, msg: crate::message::TerminalMsg) -> Task<Message> {
        use crate::message::TerminalMsg;
        match msg {
            TerminalMsg::OpenRequested(name) => {
                // Open the terminal page for this container; (re)load the
                // enter-command display. The picker seeds from the persisted
                // preference (#171) — without this the saved
                // `selected_terminal` was written, shown in Settings, and
                // then ignored, so every launch used the first-listed
                // terminal. Resolution prefers customs over built-ins
                // (`resolve_terminal`); an unresolvable or unset value
                // leaves `terminal_id` empty and `selected()` falls back to
                // the first available.
                self.terminal.container = Some(name.clone());
                self.terminal.terminal_id = self
                    .config
                    .as_ref()
                    .and_then(|cfg| self.resolve_configured_terminal(&cfg.selected_terminal));
                self.terminal.enter_argv = None;
                self.terminal.command_error = None;
                self.terminal.loading_command = true;
                let backend = Arc::clone(&self.backend);
                return Self::run(async move {
                    // `enter_command` is synchronous (argv build, no spawn).
                    let argv = backend.enter_command(&name);
                    Message::Terminal(TerminalMsg::CommandLoaded(name, Ok(argv)))
                });
            }
            TerminalMsg::Closed => {
                self.terminal = crate::terminal::TerminalState::default();
            }
            TerminalMsg::CommandReloadRequested(name) => {
                self.terminal.loading_command = true;
                self.terminal.command_error = None;
                let backend = Arc::clone(&self.backend);
                return Self::run(async move {
                    let argv = backend.enter_command(&name);
                    Message::Terminal(TerminalMsg::CommandLoaded(name, Ok(argv)))
                });
            }
            TerminalMsg::CommandLoaded(name, result) => {
                // Stale-response guard (T8 pattern): only the open container.
                if self.terminal.container.as_deref() != Some(&name) {
                    return Self::none();
                }
                self.terminal.loading_command = false;
                match result {
                    Ok(argv) => self.terminal.enter_argv = Some(argv),
                    Err(e) => {
                        self.terminal.command_error = Some(Self::error_text(&e));
                    }
                }
            }
            TerminalMsg::CopyRequested(cmd) => {
                // Rows #70–#72: real clipboard effect (T6 B1 lesson — never
                // drop the effect), toast follows.
                let write = cosmic::iced::clipboard::write(cmd.clone());
                let toast = Self::done(Message::Ui(UiMsg::CopiedToClipboard(cmd)));
                return Task::batch(vec![write, toast]);
            }
            TerminalMsg::TerminalSelected(i) => {
                let id = self.terminal_list().get(i).map(|t| t.full_command_id());
                self.terminal.terminal_id = id;
            }
            TerminalMsg::LaunchRequested(container) => {
                // D8: spawn the selected terminal attached to the container
                // through the env-mapped runner. Synchronous (spawn returns
                // immediately); toast reports (no output subscription).
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let terminals = self.terminal_list();
                let terminal = match self.terminal.selected(&terminals) {
                    Some(t) => t.clone(),
                    None => {
                        let toast = self.toast(fl!("app-no-terminal-available"));
                        return toast;
                    }
                };
                match self.backend.launch_terminal(&container, &terminal) {
                    Ok(()) => {
                        let toast = self.toast(fl!(
                            "app-launched-terminal",
                            terminal = terminal.name.as_str(),
                            container = container.as_str()
                        ));
                        return toast;
                    }
                    Err(e) => {
                        let toast = self.toast(fl!(
                            "app-could-not-launch-terminal",
                            error = Self::error_text(&e)
                        ));
                        return toast;
                    }
                }
            }
            TerminalMsg::LaunchFinished(result) => match result {
                Ok(msg) => {
                    let toast = self.toast(msg);
                    return toast;
                }
                Err(e) => {
                    let toast = self.toast(fl!("app-launch-failed", error = Self::error_text(&e)));
                    return toast;
                }
            },
        }
        Self::none()
    }

    /// Terminal list for the picker (D8): the built-in table, always
    /// listable with no filesystem or runner. Custom + flatpak entries join
    /// in T12's config pass (the repository owns them); launch fails loudly
    /// for absent programs in the meantime.
    /// The terminal list — owned by the backend (T12), never re-derived
    /// here: every picker renders `backend.terminals()` and every
    /// index-based `TerminalSelected` resolves against the same call, so an
    /// index cannot mean one terminal when sent and another when received.
    /// Map a persisted `selected_terminal` to a `full_command_id` in the
    /// indexed list (#171). Resolved against built-ins + customs rather
    /// than the merged list so a custom still wins a legacy bare-program
    /// value (`resolve_terminal`'s precedence). `None` for an unknown
    /// value: the picker then shows first-available instead of a
    /// terminal nobody chose.
    /// Whether an app/binaries reply belongs to the container the page is
    /// showing. `selected_container` is the same value the page header and
    /// the export toggles read, so this is exactly the "would this payload
    /// be displayed as the current container's?" test.
    fn apps_reply_is_current(&self, container: &str) -> bool {
        self.selected_container.as_deref() == Some(container)
    }

    fn resolve_configured_terminal(&self, stored: &str) -> Option<String> {
        let builtins = gosh_distrobox_core::backends::builtin_terminals();
        let customs = self
            .config
            .as_ref()
            .map(|c| c.custom_terminals.clone())
            .unwrap_or_default();
        gosh_distrobox_core::resolve_terminal(stored, &builtins, &customs)
            .map(|t| t.full_command_id())
    }

    fn terminal_list(&self) -> Vec<gosh_distrobox_core::backends::Terminal> {
        self.backend.terminals()
    }

    /// Settings router (T12, rows #163–#171 + config §5). Config load lives
    /// in `settings::load_entry` (called from `init`); the live-update path
    /// is the `config_subscription` above.
    fn update_settings(&mut self, msg: crate::message::SettingsMsg) -> Task<Message> {
        use crate::message::SettingsMsg;
        match msg {
            SettingsMsg::VersionReloadRequested => {
                self.loading_version = true;
                let backend = Arc::clone(&self.backend);
                return Self::run(async move {
                    let result = backend.distrobox_version().await;
                    Message::Settings(SettingsMsg::VersionLoaded(result))
                });
            }
            SettingsMsg::VersionLoaded(result) => {
                self.loading_version = false;
                match result {
                    Ok(v) => self.distrobox_version = v,
                    Err(e) => {
                        // #163: "Unknown" alone is indistinguishable from a
                        // distrobox that answered with nothing — the toast
                        // says which, so the Refresh button is not a dead
                        // end the user keeps pressing.
                        self.distrobox_version = fl!("app-version-unknown");
                        let toast = self.toast(fl!(
                            "app-could-not-read-version",
                            error = Self::error_text(&e)
                        ));
                        return toast;
                    }
                }
            }
            SettingsMsg::RefreshAllRequested => {
                // Row #165: refresh containers + version, toast afterwards.
                self.loading.containers = true;
                self.loading_version = true;
                let b1 = Arc::clone(&self.backend);
                let b2 = Arc::clone(&self.backend);
                let toast = self.toast(fl!("app-data-refreshed"));
                let refresh = Task::batch(vec![
                    Self::refresh_containers(&b1),
                    Self::run(async move {
                        let result = b2.distrobox_version().await;
                        Message::Settings(SettingsMsg::VersionLoaded(result))
                    }),
                ]);
                return Task::batch(vec![toast, refresh]);
            }
            SettingsMsg::StopAllRequested => {
                return Self::done(Message::Containers(ContainerMsg::StopAllRequested));
            }
            SettingsMsg::UpgradeAllRequested => {
                // Row #167 (dead redirect in Flutter): same real path as
                // dashboard (confirm → per-container tasks).
                return Self::done(Message::Containers(ContainerMsg::UpgradeAllRequested));
            }
            SettingsMsg::ClearCompleted => {
                // Row #168 + toast (Flutter toasted; the header path shares
                // the Tasks arm — route through it so behaviour is one path).
                let toast = self.toast(fl!("app-completed-tasks-cleared"));
                let clear = Self::done(Message::Tasks(TaskMsg::ClearCompleted));
                return Task::batch(vec![toast, clear]);
            }
            SettingsMsg::DeleteAllRequested => {
                // Row #170: shared destructive confirm with warning box copy.
                return self.confirm_or_run(ConfirmSpec {
                    title: fl!("app-delete-all-containers"),
                    body: fl!("app-confirm-delete-all-containers"),
                    confirm_label: fl!("app-delete-all"),
                    destructive: true,
                    action: ConfirmAction::DeleteAllContainers,
                });
            }
            SettingsMsg::TerminalSelected(i) => {
                // Row #171: persist terminal choice (best-effort).
                let terminals = self.terminal_list();
                if let Some(t) = terminals.get(i) {
                    let id = t.full_command_id();
                    let follow = self.write_config(|c| c.selected_terminal = id);
                    self.terminal.terminal_id = Some(
                        self.config
                            .as_ref()
                            .map(|c| c.selected_terminal.clone())
                            .unwrap_or_default(),
                    );
                    return follow;
                }
            }
            SettingsMsg::ConfirmToggled(v) => {
                return self.write_config(|c| c.confirm_destructive_actions = v);
            }
            SettingsMsg::ShowSkippedLinesToggled(v) => {
                return self.write_config(|c| c.show_skipped_lines = v);
            }
            SettingsMsg::SnapshotPrefixChanged(p) => {
                return self.write_config(|c| c.snapshot_prefix = p);
            }
            SettingsMsg::ExportDirChanged(d) => {
                return self.write_config(|c| c.default_export_dir = d);
            }
            SettingsMsg::OpenUrl(url) => {
                // Row #169: URL open toasts only on failure (Flutter parity
                // — success needs no confirmation). "Failure" here is a
                // failed LAUNCH; the handler's own exit is not observed
                // (`Backend::open_url`), so a link that opens nothing on a
                // host with no browser stays silent — as in Flutter.
                if let Err(e) = self.backend.open_url(&url) {
                    let toast = self.toast(fl!(
                        "app-could-not-open",
                        url = url,
                        error = Self::error_text(&e)
                    ));
                    return toast;
                }
            }
            SettingsMsg::ConfigChanged(cfg) => {
                // §5.1-2: external edits land live. Our own writes produce
                // an equal value, and the watcher only fires on real key
                // changes — so an equal payload is a genuine no-op and we
                // must NOT re-write it back (that would echo forever).
                if self.config.as_ref() == Some(&cfg) {
                    return Self::none();
                }
                self.legacy_import_pending = false;
                self.config = Some(cfg);
                self.sync_terminals();
            }
            SettingsMsg::LegacyImported(legacy) => {
                // D11/PKG-9: applied only while the gate is armed (the keys
                // were absent at load), and only for what the hosts still
                // has. `apply_legacy` never overwrites a key our config
                // already set. Empty result = fresh install → nothing to do.
                if !self.legacy_import_pending {
                    return Self::none();
                }
                self.legacy_import_pending = false;
                if legacy.selected_terminal.is_none() && legacy.custom_terminals.is_empty() {
                    return Self::none();
                }
                // The gate is armed only when a config handle exists —
                // `init` disarms it when the config dir is unavailable AND
                // skips the probe, so this is always the loaded case. An
                // in-memory-only import would show values that vanish on
                // restart; the honest degrade is to not import at all.
                let builtins = self.terminal_list();
                let Some(cfg) = self.config.as_mut() else {
                    return Self::none();
                };
                gosh_distrobox_core::apply_legacy(cfg, &legacy, &builtins);
                self.sync_terminals();
                let toast = self.toast(fl!("app-imported-settings"));
                let persist = self.write_config(|_| {});
                return Task::batch(vec![toast, persist]);
            }
        }
        Self::none()
    }

    /// Best-effort config write (T12 §5): mutate in memory, persist when
    /// a handle exists, toast when it doesn't. Never crashes. Returns an
    /// optional follow-up task (persistence-failure toast).
    fn write_config(
        &mut self,
        f: impl FnOnce(&mut gosh_distrobox_core::AppConfig),
    ) -> Task<Message> {
        let mut fail = false;
        if let Some(cfg) = self.config.as_mut() {
            f(cfg);
            // One `CosmicConfigEntry`: snake_case fields are the keys, one
            // file per key (§5.1-5). Preferred path — `config.rs`'s own
            // per-key helper does not carry `custom_terminals` (imported
            // customs would be dropped on the next save).
            if crate::settings::save_entry(&crate::settings::PrefsEntry::from(&*cfg)).is_err() {
                fail = true;
            }
        } else {
            fail = true;
        }
        if fail {
            self.toast(fl!("app-settings-no-persist"))
        } else {
            Self::none()
        }
    }

    fn update_packages(&mut self, msg: crate::message::PackagesMsg) -> Task<Message> {
        use crate::message::PackagesMsg;
        match msg {
            PackagesMsg::ContainerSelected(i) => {
                if let Some(c) = self.containers.get(i).map(|c| c.name.clone()) {
                    return self.select_package_container(c);
                }
                Self::none()
            }
            PackagesMsg::TabSelected(search) => {
                self.packages.search_tab = search;
                Self::none()
            }
            PackagesMsg::QueryChanged(q) => {
                self.packages.query = q;
                Self::none()
            }
            PackagesMsg::SearchSubmitted => {
                let (container, query) = match self.packages.container.clone() {
                    Some(c) if !self.packages.query.trim().is_empty() => {
                        (c, self.packages.query.trim().to_string())
                    }
                    _ => return Self::none(),
                };
                if !self.packages_running(&container) {
                    return Self::none();
                }
                self.packages.searching = true;
                self.packages.search_tab = true;
                self.packages.loading = true;
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let result = backend.search_packages(&container, &query).await;
                    Message::Packages(PackagesMsg::SearchLoaded(result))
                })
            }
            PackagesMsg::SearchCleared => {
                self.packages.query.clear();
                self.packages.results.clear();
                self.packages.searching = false;
                self.packages.search_tab = false;
                Self::none()
            }
            PackagesMsg::ReloadRequested(name) => self.select_package_container(name),
            PackagesMsg::ManagerDetected(name, result) => {
                // Stale-response guard: only the current container sticks.
                if self.packages.container.as_deref() != Some(&name) {
                    return Self::none();
                }
                match result {
                    Ok(pm) => self.packages.manager = Some(pm),
                    Err(e) => {
                        // B1 invariant: `Unknown` ONLY when the script
                        // reports "unknown" (→ Ok(Unknown)), INSTEAD of an
                        // error — never both. A failed probe leaves manager
                        // None ("Detecting…") + the error banner.
                        self.packages.manager = None;
                        self.packages.error = Some(Self::error_text(&e));
                    }
                }
                Self::none()
            }
            PackagesMsg::InstalledLoaded(name, result) => {
                if self.packages.container.as_deref() != Some(&name) {
                    return Self::none();
                }
                self.packages.loading = false;
                match result {
                    Ok(list) => {
                        self.packages.installed = list;
                        self.packages.error = None;
                    }
                    Err(e) => {
                        self.packages.error = Some(Self::error_text(&e));
                    }
                }
                Self::none()
            }
            PackagesMsg::SearchLoaded(result) => {
                self.packages.loading = false;
                match result {
                    Ok(list) => {
                        self.packages.results = list;
                        Self::none()
                    }
                    Err(e) => self.toast(fl!("app-search-failed", error = Self::error_text(&e))),
                }
            }
            PackagesMsg::InstallFromBox => {
                // Row #115: disabled when empty (the button enforces it) —
                // re-check here so a raced empty never spawns.
                match self.packages.container.clone() {
                    Some(c) if !self.packages.query.trim().is_empty() => {
                        let package = self.packages.query.trim().to_string();
                        self.confirm_install(c, package)
                    }
                    _ => Self::none(),
                }
            }
            PackagesMsg::InstallRequested(name) => {
                if let Some(container) = self.packages.container.clone() {
                    self.confirm_install(container, name)
                } else {
                    Self::none()
                }
            }
            PackagesMsg::RemoveRequested(name) => {
                if let Some(container) = self.packages.container.clone() {
                    return self.confirm_or_run(ConfirmSpec {
                        title: fl!("app-remove-package"),
                        body: fl!(
                            "app-confirm-remove-package",
                            package = name.as_str(),
                            container = container.as_str()
                        ),
                        confirm_label: fl!("action-remove"),
                        destructive: true,
                        action: ConfirmAction::RemovePackage {
                            container,
                            package: name,
                        },
                    });
                }
                Self::none()
            }
            PackagesMsg::UpgradeAllRequested => {
                if let Some(container) = self.packages.container.clone() {
                    return self.confirm_or_run(ConfirmSpec {
                        title: fl!("app-upgrade-all-packages"),
                        body: fl!(
                            "app-confirm-upgrade-packages",
                            container = container.as_str()
                        ),
                        confirm_label: fl!("app-upgrade-all"),
                        destructive: false,
                        action: ConfirmAction::UpgradeContainer(container),
                    });
                }
                Self::none()
            }
            PackagesMsg::ManualCmdChanged(c) => {
                self.packages.manual_cmd = c;
                Self::none()
            }
            PackagesMsg::ManualRunRequested => {
                // B1 Unknown: T9 owns real execution — record the intent.
                self.toast(fl!("app-manual-commands-t9"))
            }
        }
    }

    /// Install confirm helper (row #120): shared spec, non-destructive.
    fn confirm_install(&mut self, container: String, package: String) -> Task<Message> {
        self.confirm_or_run(ConfirmSpec {
            title: fl!("app-install-package"),
            body: fl!(
                "app-confirm-install-package",
                package = package.as_str(),
                container = container.as_str()
            ),
            confirm_label: fl!("app-install"),
            destructive: false,
            action: ConfirmAction::InstallPackage { container, package },
        })
    }

    /// Whether this container is currently running (search/actions gate).
    fn packages_running(&self, container: &str) -> bool {
        self.containers
            .iter()
            .find(|c| c.name == container)
            .map(|c| crate::icons::is_running(&c.status))
            .unwrap_or(false)
    }

    /// Backups dialogs (T10, rows #140/#142–#145): create/restore/export/
    /// import/clone bodies through the single modal slot. Every input
    /// validates LOUD (#146 — no silent returns); browse buttons open the
    /// portal choosers (P0 #149).
    fn backups_dialog(&self) -> Option<cosmic::Element<'_, Message>> {
        use crate::backups::BackupsDialog;
        use crate::message::BackupsMsg;
        let st = &self.backups;
        match st.dialog.as_ref()? {
            BackupsDialog::Create => {
                let body: cosmic::Element<'_, Message> = widget::Column::new()
                    .push(widget::text::body(fl!(
                        "app-create-snapshot-of",
                        name = st.container.as_deref().unwrap_or("")
                    )))
                    .push({
                        let input: cosmic::Element<'_, Message> = widget::text_input::text_input(
                            fl!("app-snapshot-name-placeholder"),
                            st.create_name.clone(),
                        )
                        .on_input(|s| Message::Backups(BackupsMsg::CreateNameChanged(s)))
                        .into();
                        input
                    })
                    .push(widget::text::caption(fl!("app-snapshot-commit-helper")))
                    .spacing(8)
                    .into();
                let body = match st.create_error.clone() {
                    Some(err) => widget::Column::new()
                        .push(body)
                        .push(widget::warning(err))
                        .spacing(8)
                        .into(),
                    None => body,
                };
                Some(
                    widget::dialog()
                        .title(fl!("app-create-snapshot"))
                        .control(body)
                        .primary_action({
                            let create: cosmic::Element<'_, Message> =
                                widget::button::suggested(fl!("action-create"))
                                    .on_press(Message::Backups(BackupsMsg::CreateConfirmed))
                                    .into();
                            create
                        })
                        .secondary_action({
                            let cancel: cosmic::Element<'_, Message> =
                                widget::button::standard(fl!("action-cancel"))
                                    .on_press(Message::Backups(BackupsMsg::DialogCancelled))
                                    .into();
                            cancel
                        })
                        .into(),
                )
            }
            BackupsDialog::Restore(snapshot) => {
                let name = st
                    .restore
                    .as_ref()
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                let body: cosmic::Element<'_, Message> = widget::Column::new()
                    .push(widget::text::body(fl!(
                        "app-restore-create-container",
                        snapshot = snapshot
                    )))
                    .push({
                        let input: cosmic::Element<'_, Message> = widget::text_input::text_input(
                            fl!("app-new-container-name-placeholder"),
                            name,
                        )
                        .on_input(|s| Message::Backups(BackupsMsg::RestoreNameChanged(s)))
                        .into();
                        input
                    })
                    .spacing(8)
                    .into();
                let body = match st.restore_error.clone() {
                    Some(err) => widget::Column::new()
                        .push(body)
                        .push(widget::warning(err))
                        .spacing(8)
                        .into(),
                    None => body,
                };
                Some(
                    widget::dialog()
                        .title(fl!("app-restore-from-snapshot"))
                        .control(body)
                        .primary_action({
                            let restore: cosmic::Element<'_, Message> =
                                widget::button::suggested(fl!("app-restore"))
                                    .on_press(Message::Backups(BackupsMsg::RestoreConfirmed))
                                    .into();
                            restore
                        })
                        .secondary_action({
                            let cancel: cosmic::Element<'_, Message> =
                                widget::button::standard(fl!("action-cancel"))
                                    .on_press(Message::Backups(BackupsMsg::DialogCancelled))
                                    .into();
                            cancel
                        })
                        .into(),
                )
            }
            BackupsDialog::Export => {
                let body: cosmic::Element<'_, Message> = widget::Column::new()
                    .push(widget::text::body(fl!(
                        "app-export-as-tar",
                        name = st.container.as_deref().unwrap_or("")
                    )))
                    .push({
                        let input: cosmic::Element<'_, Message> = widget::text_input::text_input(
                            fl!("app-export-path-placeholder"),
                            st.export_path.clone(),
                        )
                        .on_input(|s| Message::Backups(BackupsMsg::ExportPathChanged(s)))
                        .into();
                        input
                    })
                    .push(
                        widget::button::standard(fl!("app-browse"))
                            .on_press(Message::Backups(BackupsMsg::ExportBrowseRequested)),
                    )
                    .push(widget::warning(fl!("app-export-size-warning")))
                    .spacing(8)
                    .into();
                let body = match st.export_error.clone() {
                    Some(err) => widget::Column::new()
                        .push(body)
                        .push(widget::warning(err))
                        .spacing(8)
                        .into(),
                    None => body,
                };
                Some(
                    widget::dialog()
                        .title(fl!("app-export-container"))
                        .control(body)
                        .primary_action({
                            let export: cosmic::Element<'_, Message> =
                                widget::button::suggested(fl!("app-export"))
                                    .on_press(Message::Backups(BackupsMsg::ExportConfirmed))
                                    .into();
                            export
                        })
                        .secondary_action({
                            let cancel: cosmic::Element<'_, Message> =
                                widget::button::standard(fl!("action-cancel"))
                                    .on_press(Message::Backups(BackupsMsg::DialogCancelled))
                                    .into();
                            cancel
                        })
                        .into(),
                )
            }
            BackupsDialog::Import => {
                let body: cosmic::Element<'_, Message> = widget::Column::new()
                    .push(widget::text::body(fl!("app-archive-path")))
                    .push({
                        let input: cosmic::Element<'_, Message> = widget::text_input::text_input(
                            fl!("app-export-path-placeholder"),
                            st.import_path.clone(),
                        )
                        .on_input(|s| Message::Backups(BackupsMsg::ImportPathChanged(s)))
                        .into();
                        input
                    })
                    .push(
                        widget::button::standard(fl!("app-browse"))
                            .on_press(Message::Backups(BackupsMsg::ImportBrowseRequested)),
                    )
                    .push(widget::text::body(fl!("app-image-name")))
                    .push({
                        let input: cosmic::Element<'_, Message> = widget::text_input::text_input(
                            fl!("app-import-image-placeholder"),
                            st.import_image.clone(),
                        )
                        .on_input(|s| Message::Backups(BackupsMsg::ImportImageChanged(s)))
                        .into();
                        input
                    })
                    .spacing(8)
                    .into();
                let body = match st.import_error.clone() {
                    Some(err) => widget::Column::new()
                        .push(body)
                        .push(widget::warning(err))
                        .spacing(8)
                        .into(),
                    None => body,
                };
                Some(
                    widget::dialog()
                        .title(fl!("app-import-container"))
                        .control(body)
                        .primary_action({
                            let import: cosmic::Element<'_, Message> =
                                widget::button::suggested(fl!("app-import"))
                                    .on_press(Message::Backups(BackupsMsg::ImportConfirmed))
                                    .into();
                            import
                        })
                        .secondary_action({
                            let cancel: cosmic::Element<'_, Message> =
                                widget::button::standard(fl!("action-cancel"))
                                    .on_press(Message::Backups(BackupsMsg::DialogCancelled))
                                    .into();
                            cancel
                        })
                        .into(),
                )
            }
        }
    }

    /// Dispatch a confirmed follow-up (the `DialogMsg::Confirmed` body,
    /// shared with the `confirm_destructive_actions == false` fast path).
    fn dispatch_confirm_action(&mut self, action: ConfirmAction) -> Task<Message> {
        match action {
            ConfirmAction::RemoveContainer(name) => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                if !self.busy.insert(format!("remove:{name}")) {
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let result = backend
                        .remove_container(&name)
                        .await
                        .map(|_| fl!("app-container-deleted", name = name.as_str()));
                    Message::Containers(ContainerMsg::ActionFinished(result))
                })
            }
            ConfirmAction::StopAll => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                if !self.busy.insert("stop-all".to_string()) {
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let result = backend
                        .stop_all_containers()
                        .await
                        .map(|_| fl!("app-all-containers-stopped"));
                    Message::Containers(ContainerMsg::ActionFinished(result))
                })
            }
            ConfirmAction::DeleteAllContainers => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                if !self.busy.insert("delete-all".to_string()) {
                    return Self::none();
                }
                // Row #170 / #187: every container goes, and the ones that
                // did NOT are reported. Discarding each result and then
                // calling `list()` reported "All containers deleted" while a
                // wedged or permission-denied container silently survived.
                let names: Vec<String> = self.containers.iter().map(|c| c.name.clone()).collect();
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let mut failures = Vec::new();
                    for name in &names {
                        if let Err(e) = backend.remove_container(name).await {
                            failures.push(format!("{name} ({e})"));
                        }
                    }
                    let done = names.len() - failures.len();
                    // `failures` is built from container names + command
                    // errors, so it is user data → placeable (Q17).
                    let result = if failures.is_empty() {
                        Ok(fl!("app-all-containers-deleted", count = names.len()))
                    } else {
                        Ok(fl!(
                            "app-containers-deleted-partial",
                            done = done,
                            total = names.len(),
                            failures = failures.join(", ")
                        ))
                    };
                    Message::Containers(ContainerMsg::ActionFinished(result))
                })
            }
            ConfirmAction::UpgradeAll => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let running: Vec<String> = self
                    .containers
                    .iter()
                    .filter(|c| icons::is_running(&c.status))
                    .map(|c| c.name.clone())
                    .collect();
                let backend = Arc::clone(&self.backend);
                let spawns: Vec<Task<Message>> = running
                    .into_iter()
                    .map(|name| {
                        let backend = Arc::clone(&backend);
                        let label = fl!("app-label-upgrade", name = name.as_str());
                        Self::run(async move {
                            let result = backend.upgrade_container(&name).await;
                            Message::Tasks(TaskMsg::Started {
                                label,
                                kind: TaskKind::Upgrade,
                                result,
                            })
                        })
                    })
                    .collect();
                if spawns.is_empty() {
                    return Self::none();
                }
                Task::batch(spawns)
            }
            ConfirmAction::InstallPackage { container, package } => {
                // Row #122: spawn failure reports via
                // `TaskMsg::Started Err` → error banner (the
                // Started arm toasts too — never silent,
                // unlike Flutter null).
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                let label = fl!(
                    "app-label-install",
                    package = package.as_str(),
                    container = container.as_str()
                );
                Self::run(async move {
                    let result = backend.install_package(&container, &package).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Other,
                        result,
                    })
                })
            }
            ConfirmAction::RemovePackage { container, package } => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                let label = fl!(
                    "app-label-remove-package",
                    package = package.as_str(),
                    container = container.as_str()
                );
                Self::run(async move {
                    let result = backend.remove_package(&container, &package).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Other,
                        result,
                    })
                })
            }
            ConfirmAction::DeleteSnapshot(id) => {
                // Row #141: delete is short (no child to
                // stream); toast green/red via ActionFinished,
                // then reload the list in the same future.
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                Self::run(async move {
                    let delete = backend.delete_snapshot(&id).await;
                    Message::Backups(crate::message::BackupsMsg::DeleteFinished(delete))
                })
            }
            ConfirmAction::UpgradeContainer(container) => {
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let backend = Arc::clone(&self.backend);
                let label = fl!("app-label-upgrade", name = container.as_str());
                Self::run(async move {
                    let result = backend.upgrade_container(&container).await;
                    Message::Tasks(TaskMsg::Started {
                        label,
                        kind: TaskKind::Upgrade,
                        result,
                    })
                })
            }
        }
    }

    /// Wizard message router (T7, rows #78–#96). Every arm mutates
    /// `self.wizard` (or spawns via `Backend::create_container`); `None`
    /// wizard ignores everything (no dead dispatch).
    fn update_wizard(&mut self, msg: crate::message::WizardMsg) -> Task<Message> {
        use crate::message::WizardMsg;
        use crate::wizard::{VolumeDialog, WizardState, WizardStep};
        match msg {
            WizardMsg::Closed => {
                self.wizard = None;
            }
            WizardMsg::ImageSelected(image) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.selected_image = Some(image);
                    w.custom_image.clear();
                    w.inline_error = None;
                }
            }
            WizardMsg::CustomChanged(u) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.custom_image = u;
                    if !w.custom_image.is_empty() {
                        w.selected_image = None;
                    }
                    w.inline_error = None;
                }
            }
            WizardMsg::SearchChanged(q) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.search = q;
                }
            }
            WizardMsg::NextFromImage => {
                // Row #82 + #84: require an image (INLINE error, not a
                // snackbar); auto-default the name when empty.
                let next = if let Some(w) = self.wizard.as_ref() {
                    let image = w.effective_image();
                    if image.is_empty() {
                        None
                    } else {
                        Some((image.clone(), w.name.is_empty()))
                    }
                } else {
                    None
                };
                match next {
                    None => {
                        if let Some(w) = self.wizard.as_mut() {
                            w.inline_error = Some(fl!("app-please-select-an-image"));
                        }
                    }
                    Some((image, fill_name)) => {
                        if let Some(w) = self.wizard.as_mut() {
                            if fill_name {
                                w.name = WizardState::default_name(&image);
                            }
                            w.step = WizardStep::Config;
                            w.inline_error = None;
                        }
                    }
                }
            }
            WizardMsg::NameChanged(n) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.name = n;
                    w.inline_error = None;
                }
            }
            WizardMsg::InitToggled(v) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.init_system = v;
                }
            }
            WizardMsg::NvidiaToggled(v) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.nvidia = v;
                }
            }
            WizardMsg::AdvancedToggled => {
                if let Some(w) = self.wizard.as_mut() {
                    w.advanced_open = !w.advanced_open;
                }
            }
            WizardMsg::HomeChanged(h) => {
                if let Some(w) = self.wizard.as_mut() {
                    w.home_dir = h;
                }
            }
            WizardMsg::VolumeAddRequested => {
                if let Some(w) = self.wizard.as_mut() {
                    w.volume_dialog = Some(VolumeDialog::default());
                }
            }
            WizardMsg::VolumeDialogHostChanged(h) => {
                if let Some(w) = self.wizard.as_mut()
                    && let Some(d) = w.volume_dialog.as_mut()
                {
                    d.host = h;
                    d.error = None;
                }
            }
            WizardMsg::VolumeDialogContainerChanged(c) => {
                if let Some(w) = self.wizard.as_mut()
                    && let Some(d) = w.volume_dialog.as_mut()
                {
                    d.container = c;
                    d.error = None;
                }
            }
            WizardMsg::VolumeDialogReadOnlyToggled(v) => {
                if let Some(w) = self.wizard.as_mut()
                    && let Some(d) = w.volume_dialog.as_mut()
                {
                    d.read_only = v;
                }
            }
            WizardMsg::VolumeDialogConfirmed => {
                // Row #90: LOUD validation (the Flutter dialog silently
                // returned on bad input — fix, not parity).
                let verdict = if let Some(w) = self.wizard.as_ref() {
                    if let Some(d) = w.volume_dialog.as_ref() {
                        if d.host.trim().is_empty() {
                            Err(fl!("app-host-path-required"))
                        } else if d.container.trim().is_empty() {
                            Err(fl!("app-container-path-required"))
                        } else {
                            Ok((
                                d.host.trim().to_string(),
                                d.container.trim().to_string(),
                                d.read_only,
                            ))
                        }
                    } else {
                        return Self::none();
                    }
                } else {
                    return Self::none();
                };
                match verdict {
                    Err(err) => {
                        if let Some(w) = self.wizard.as_mut()
                            && let Some(d) = w.volume_dialog.as_mut()
                        {
                            d.error = Some(err);
                        }
                    }
                    Ok((host, container, read_only)) => {
                        if let Some(w) = self.wizard.as_mut() {
                            w.volumes.push(crate::wizard::WizardVolume {
                                host,
                                container,
                                read_only,
                            });
                            w.volume_dialog = None;
                        }
                    }
                }
            }
            WizardMsg::VolumeDialogCancelled => {
                if let Some(w) = self.wizard.as_mut() {
                    w.volume_dialog = None;
                }
            }
            WizardMsg::VolumeRemoved(i) => {
                if let Some(w) = self.wizard.as_mut()
                    && i < w.volumes.len()
                {
                    w.volumes.remove(i);
                }
            }
            WizardMsg::BackToImage => {
                if let Some(w) = self.wizard.as_mut() {
                    w.step = WizardStep::Image;
                    w.inline_error = None;
                }
            }
            WizardMsg::CreateRequested => {
                // Rows #91/#96: empty check + `CreateArgName` surfaced
                // INLINE (Flutter never called it before create).
                if is_blocked(self.backend.env()) {
                    self.error = self.backend.env().message.clone();
                    return Self::none();
                }
                let verdict = if let Some(w) = self.wizard.as_ref() {
                    if w.name.trim().is_empty() {
                        Err(fl!("app-please-enter-container-name"))
                    } else {
                        match gosh_distrobox_core::models::CreateArgName::new(w.name.trim()) {
                            // Flutter had no client-side name validation at all
                            // (`CreateArgName` was a bare freezed constructor),
                            // so the *backend's* diagnostic is what its snackbar
                            // showed. `to_string()` keeps that: the port added
                            // the early check, and the text stays the core one.
                            Err(e) => Err(fl!("app-invalid-container-name", error = e.to_string())),
                            Ok(arg_name) => {
                                use gosh_distrobox_core::models::{CreateArgs, Volume, VolumeMode};
                                let volumes: Vec<Volume> = w
                                    .volumes
                                    .iter()
                                    .map(|v| Volume {
                                        host_path: v.host.clone(),
                                        container_path: v.container.clone(),
                                        mode: if v.read_only {
                                            Some(VolumeMode::ReadOnly)
                                        } else {
                                            None
                                        },
                                    })
                                    .collect();
                                Ok(CreateArgs {
                                    init: w.init_system,
                                    nvidia: w.nvidia,
                                    home_path: if w.home_dir.trim().is_empty() {
                                        None
                                    } else {
                                        Some(w.home_dir.trim().to_string())
                                    },
                                    image: w.effective_image(),
                                    name: arg_name,
                                    volumes,
                                })
                            }
                        }
                    }
                } else {
                    return Self::none();
                };
                match verdict {
                    Err(err) => {
                        if let Some(w) = self.wizard.as_mut() {
                            w.inline_error = Some(err);
                        }
                    }
                    Ok(args) => {
                        let backend = Arc::clone(&self.backend);
                        let label = fl!("app-label-create", name = args.name.0.as_str());
                        if let Some(w) = self.wizard.as_mut() {
                            w.step = WizardStep::Progress;
                            w.inline_error = None;
                        }
                        return Self::run(async move {
                            let result = backend.create_container(args).await;
                            Message::Tasks(TaskMsg::Started {
                                label,
                                kind: TaskKind::Create,
                                result,
                            })
                        });
                    }
                }
            }
            WizardMsg::ProgressCancelRequested => {
                // Row #94 Cancel: same body as `TaskMsg::CancelRequested`
                // (marks the mirror + latches wizard completion) instead
                // of calling the registry directly — a direct call leaves
                // the progress step on "Creating…" with a dead Cancel
                // (no `Completed` ever arrives: core drops the sender
                // without a terminal event on cancel). Not `self.update`
                // (trait method, out of scope in this impl block).
                let id = self.wizard.as_ref().and_then(|w| w.task_id);
                if let Some(id) = id
                    && self.backend.cancel_task(id)
                {
                    self.latch_cancelled(id);
                    return Self::done(Message::Tasks(TaskMsg::Cancelled(id)));
                }
            }
            WizardMsg::ProgressDone => {
                // Row #94 Done/Close: leave the wizard; the list refreshes
                // behind via the normal subscription flow.
                self.wizard = None;
                self.loading.containers = true;
                return Self::refresh_containers(&self.backend);
            }
        }
        Self::none()
    }

    /// Whether this container has an upgrade task in flight (details tile
    /// inline spinner, row #60). Matches `TaskMsg::Started` labels
    /// (`Upgrade {name}`) for incomplete mirror entries — the data T5
    /// already stores, so the button disables while its task runs (and N
    /// presses cannot spawn N upgrades).
    fn upgrading(&self, container: &ContainerInfo) -> bool {
        let want = fl!("app-label-upgrade", name = container.name.as_str());
        self.tasks.values().any(|v| !v.completed && v.label == want)
    }

    /// Shared cancellation latch (O2 class): mark the mirror completed
    /// and latch wizard completion for the matching task, so the progress
    /// step lands on a terminal state (Done/Close) instead of a dead Cancel.
    fn latch_cancelled(&mut self, id: gosh_distrobox_core::TaskId) {
        if let Some(view) = self.tasks.get_mut(&id) {
            view.completed = true;
        }
        if let Some(w) = self.wizard.as_mut()
            && w.task_id == Some(id)
        {
            w.task_completed = true;
            w.task_success = false;
        }
    }

    /// Dashboard page: counts + task rows from the T5 mirror.
    fn view_dashboard(&self) -> cosmic::Element<'_, Message> {
        let task_rows: Vec<cosmic::Element<'_, Message>> = self
            .tasks
            .iter()
            .map(|(id, view)| {
                views::task_row(*id, view.label.clone(), view.completed, view.success)
            })
            .collect();
        let n_tasks = task_rows.len();
        views::view_dashboard(
            &self.containers,
            views::DashboardCounts::from_list(
                &self.containers,
                // B3: the setting that decides whether the Dashboard mentions
                // the rows `list()` could not parse. `from_list` applies it.
                self.config
                    .as_ref()
                    .map(|c| c.show_skipped_lines)
                    .unwrap_or(false),
            ),
            n_tasks,
            task_rows,
            self.error.clone(),
        )
    }

    /// Containers page (§6.3): gate states, then the card list with quick
    /// actions inline (context_drawer owns the drawer variant — T6 renders
    /// the actions as rows; the drawer shell lands with the card menu in
    /// the advocate pass if a drawer proves better than inline rows).
    fn view_containers_page(&self) -> cosmic::Element<'_, Message> {
        if self.loading.containers && self.containers.is_empty() {
            // First load only: keep content during refresh (row #11).
            return widget::container(widget::text::body(fl!("app-loading-containers")))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        if self.error.is_some() {
            // Row #36: the Flutter error state was bare Text('Error: …').
            // The shell banner above already shows the message text — this
            // page adds ONLY the retry affordance, not a second copy.
            return widget::Column::new()
                .push({
                    let retry: cosmic::Element<'_, Message> =
                        widget::button::standard(fl!("action-retry"))
                            .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                            .into();
                    retry
                })
                .spacing(12)
                .into();
        }
        // B3: "empty" now has two meanings, and conflating them is a lie. A
        // genuinely empty list means "create your first container"; a list
        // where every row failed to parse means rows WERE returned and we
        // could not read them — telling that user to create a container they
        // already have, and hiding the reason, is worse than the old hard
        // error this task set out to fix. All three copy parts come from one
        // helper so they cannot disagree; the page supplies only its own
        // phrasing for the clean case.
        if self.containers.is_empty() {
            let (icon, title, body) =
                views::container_list_copy(&self.containers, &fl!("app-create-first-container"));
            // Only the unreadable case offers a retry: refreshing will not
            // conjure a first container, but a transient bad read might.
            let action = (!self.containers.is_clean_empty()).then(|| {
                let refresh: cosmic::Element<'static, Message> =
                    widget::button::standard(fl!("action-refresh"))
                        .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                        .into();
                refresh
            });
            return views::empty_state(icon, title, body, action);
        }
        let mut col = widget::Column::new().spacing(8);
        // `.iter()` rather than `&self.containers`: `Deref` gives the slice
        // for field/method access but not `IntoIterator for &ContainerList`.
        for c in self.containers.iter() {
            let selected = self.selected_container.as_deref() == Some(c.name.as_str());
            col = col.push(views::container_row(c, selected));
        }
        widget::scrollable(col).into()
    }
}

/// Portal save picker (P0 §4.3): suggests a file name, returns the chosen
/// path or `Err` on cancel/error. Runs inside a `Task` future (§0.2);
/// cancel is silent by portal convention (matches the doc example).
async fn portal_save_suggested(suggested: &str) -> Result<String, String> {
    use cosmic::dialog::file_chooser;
    let dialog = file_chooser::save::Dialog::new()
        .title(fl!("app-file-chooser-export-title"))
        .file_name(suggested.to_string());
    match dialog.save_file().await {
        Ok(response) => response
            .url()
            .and_then(|u| u.to_file_path().ok())
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .ok_or_else(|| "no path selected".to_string()),
        Err(file_chooser::Error::Cancelled) => Err("cancelled".to_string()),
        Err(why) => Err(format!("{why:?}")),
    }
}

/// Portal open picker for archives (P0 §4.3).
async fn portal_open_archive() -> Result<String, String> {
    use cosmic::dialog::file_chooser;
    let dialog = file_chooser::open::Dialog::new().title(fl!("app-file-chooser-import-title"));
    match dialog.open_file().await {
        Ok(response) => response
            .url()
            .to_file_path()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .ok_or_else(|| "no path selected".to_string()),
        Err(file_chooser::Error::Cancelled) => Err("cancelled".to_string()),
        Err(why) => Err(format!("{why:?}")),
    }
}

/// Shared backend for the `fn`-pointer subscription builders (§3.4).
/// `Subscription::run_with` takes a plain `fn`, so builders cannot capture
/// `self.backend` — they read it here instead. Set once in `init`, before any
/// subscription is polled (race-free, mirroring libcosmic's own OnceLock
/// sender pattern).
static BACKEND: std::sync::OnceLock<Arc<Backend>> = std::sync::OnceLock::new();

/// Sweep interval for expired tasks (§3.3): core's 600 s TTL with a 30 s
/// poll. Read by the subscription above — one documented number, not two
/// literals.
pub const TASK_SWEEP_INTERVAL_SECS: u64 = 30;

/// Plain `fn` — no captures, per `Subscription::run_with`'s signature.
/// `data` is the `TaskId`, so iced keys each stream by task and tears it
/// down when the task leaves `App::tasks`. `subscribe()` replays the ring
/// buffer, then yields live events; `Finished` terminates the stream
/// (`None` future) and `Completed` carries the outcome to the UI.
fn task_output_subscription(id: TaskId) -> Subscription<Message> {
    Subscription::run_with(id, |id: &TaskId| {
        let id = *id;
        let rx = BACKEND.get().and_then(|b| b.tasks().subscribe(id));
        futures::stream::unfold(rx, move |rx| async move {
            let rx = rx?;
            match rx.recv().await {
                Ok(TaskEvent::Output(line)) => Some((
                    Message::Tasks(TaskMsg::Output {
                        id,
                        lines: vec![line],
                    }),
                    Some(rx),
                )),
                Ok(TaskEvent::Finished { success }) => {
                    Some((Message::Tasks(TaskMsg::Completed { id, success }), None))
                }
                Err(_) => None,
            }
        })
    })
}
