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

use crate::icons;
use crate::message::{
    AppMsg, ConfirmAction, ConfirmSpec, ContainerMsg, DetailsMsg, DialogMsg, EnvMsg, ImageMsg,
    Message, StatsMsg, TaskMsg, UiMsg, is_blocked, running_count, stopped_count,
};
use crate::views::{self, Page, active_page};
use cosmic::app::{Core, Task};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::toaster::Toasts;
use cosmic::widget::{self, nav_bar};
use gosh_distrobox_core::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use gosh_distrobox_core::{
    Backend, CoreError, CoreFailure, MAX_TASK_OUTPUT_LINES, TaskEvent, TaskId,
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

    containers: Vec<ContainerInfo>,
    selected_container: Option<String>,
    images: Vec<String>,
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
pub struct TaskView {
    pub label: String,
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    #[allow(dead_code)]
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
            CoreError::BlockedEnvironment => {
                "Running inside a Distrobox container without distrobox-host-exec. \
                 Install distrobox-host-exec on the host or run on the host system."
                    .to_string()
            }
            CoreError::CommandFailed {
                command, stderr, ..
            } => format!("`{command}` failed: {stderr}"),
            other => other.to_string(),
        }
    }

    fn view_images(&self) -> cosmic::Element<'_, Message> {
        if self.loading.images {
            return widget::container(widget::text::body("Loading images…"))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        if self.images.is_empty() {
            return widget::container(widget::text::body("No images found."))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        let mut col = widget::Column::new().spacing(4);
        for image in &self.images {
            col = col.push(widget::text::body(image.clone()));
        }
        widget::scrollable(col).into()
    }

    fn view_apps(&self) -> cosmic::Element<'_, Message> {
        let Some(selected) = self.selected_container.clone() else {
            return widget::container(widget::text::body("Select a container to list its apps."))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        if self.loading.apps {
            return widget::container(widget::text::body(format!("Loading apps for {selected}…")))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        let mut col = widget::Column::new()
            .push(widget::text::title3(format!("Apps in {selected}")))
            .spacing(4);
        if self.apps.is_empty() {
            col = col.push(widget::text::body("No apps found."));
        }
        for app in &self.apps {
            let marker = if app.is_exported { " [exported]" } else { "" };
            col = col.push(widget::text::body(format!("{}{}", app.name, marker)));
        }
        col = col.push(widget::text::title3("Exported binaries"));
        if self.exported_binaries.is_empty() {
            col = col.push(widget::text::body("No exported binaries."));
        }
        for bin in &self.exported_binaries {
            col = col.push(widget::text::body(bin.name.clone()));
        }
        widget::scrollable(col).into()
    }

    fn view_stats(&self) -> cosmic::Element<'_, Message> {
        let Some(selected) = self.selected_container.clone() else {
            return widget::container(widget::text::body("Select a container to show its stats."))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        if self.loading.stats {
            return widget::container(widget::text::body(format!("Loading stats for {selected}…")))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        let Some(stats) = &self.stats else {
            return widget::container(widget::text::body(format!("No stats for {selected} yet.")))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };
        widget::Column::new()
            .push(widget::text::title3(format!("Stats: {selected}")))
            .push(widget::text::body(format!(
                "CPU: {:.1}%",
                stats.cpu_percent
            )))
            .push(widget::text::body(format!(
                "Memory: {} / {} ({:.1}%)",
                stats.memory_usage, stats.memory_limit, stats.memory_percent
            )))
            .push(widget::text::body(format!(
                "Network I/O: {}",
                stats.network_io
            )))
            .push(widget::text::body(format!("Block I/O: {}", stats.block_io)))
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

        let mut app = App {
            core,
            nav_model,
            backend: Arc::clone(&backend),
            error: None,
            containers: Vec::new(),
            selected_container: None,
            images: Vec::new(),
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
        app.core_mut()
            .set_header_title("Gosh Distrobox Manager".to_string());

        if blocked {
            app.error = backend.env().message.clone();
            return (app, Self::none());
        }
        app.loading.containers = true;
        let task = Self::refresh_containers(&backend);
        (app, task)
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav_model.activate(id);
        // Leaving Containers pops the details stack (single-level; details
        // never nests deeper, row #53).
        if self.active_page() != Page::Containers {
            self.details_for = None;
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
            Page::Apps | Page::Stats => Self::none(),
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::NavSelect(id) => return self.on_nav_select(id),
            Message::Containers(msg) => match msg {
                ContainerMsg::RefreshRequested => {
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
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
                                    Message::Apps(AppMsg::Loaded(result))
                                }),
                                Self::run(async move {
                                    let result = b2.exported_binaries(&c2).await;
                                    Message::Apps(AppMsg::BinariesLoaded(result))
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
                            let toast = self.toast(format!("Failed: {}", Self::error_text(&e)));
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
                            .map(|_| format!("{name} stopped"));
                        Message::Containers(ContainerMsg::ActionFinished(result))
                    });
                }
                ContainerMsg::RemoveRequested(name) => {
                    // Destructive → shared confirm (§3.3), not a direct run.
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
                        title: "Delete Container".to_string(),
                        body: format!(
                            "Are you sure you want to delete \"{name}\"?\n\nThis action cannot be undone and all container data will be lost."
                        ),
                        confirm_label: "Delete".to_string(),
                        destructive: true,
                        action: ConfirmAction::RemoveContainer(name),
                    }));
                }
                ContainerMsg::StopAllRequested => {
                    let count = running_count(&self.containers);
                    if count == 0 || !self.busy.insert("stop-all".to_string()) {
                        return Self::none();
                    }
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
                        title: "Stop All Containers".to_string(),
                        body: format!("Stop all {count} running containers?"),
                        confirm_label: "Stop All".to_string(),
                        destructive: true,
                        action: ConfirmAction::StopAll,
                    }));
                    self.busy.remove("stop-all");
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
                    let label = format!("Upgrade {name}");
                    let toast = self.toast(format!("Upgrading {name}…"));
                    let spawn = Self::run(async move {
                        let result = backend.upgrade_container(&name).await;
                        Message::Tasks(TaskMsg::Started { label, result })
                    });
                    return Task::batch(vec![toast, spawn]);
                }
                ContainerMsg::ViewAllRequested => {
                    // Row #23 (dead in Flutter): switch to Containers tab.
                    views::activate_containers(&mut self.nav_model);
                }
                ContainerMsg::NewContainerRequested => {
                    // Row #28 (dead in Flutter) + row #44 (FAB → header):
                    // the wizard lands in T7. If already on Containers the
                    // header button IS the affordance — say so; otherwise
                    // switch there first.
                    if self.active_page() == Page::Containers && self.details_for.is_none() {
                        let toast = self.toast("The create wizard lands in T7.".to_string());
                        return toast;
                    }
                    views::activate_containers(&mut self.nav_model);
                    let toast = self.toast(
                        "The create wizard lands in T7 — use the New Container button in the header."
                            .to_string(),
                    );
                    return toast;
                }
                ContainerMsg::UpgradeAllRequested => {
                    // Row #29 (redirect snackbar in Flutter): confirm, then
                    // spawn one upgrade task per running container.
                    let running: Vec<String> = self
                        .containers
                        .iter()
                        .filter(|c| icons::is_running(&c.status))
                        .map(|c| c.name.clone())
                        .collect();
                    if running.is_empty() {
                        return Self::none();
                    }
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
                        title: "Upgrade All Containers".to_string(),
                        body: format!("Upgrade packages in {} running containers?", running.len()),
                        confirm_label: "Upgrade All".to_string(),
                        destructive: false,
                        action: ConfirmAction::UpgradeAll,
                    }));
                }
            },
            Message::Details(msg) => match msg {
                DetailsMsg::OpenRequested(container) => {
                    // Row #26 (dashboard preview tap → details): the details
                    // page renders under Containers only, so switch there
                    // first — otherwise the tap is a silent no-op AND leaves
                    // a stale `details_for` for the next Containers visit.
                    views::activate_containers(&mut self.nav_model);
                    self.details_for = Some(container.clone());
                    // Selecting also loads apps/binaries/stats mirrors —
                    // reuse the T3 selection fan-out verbatim.
                    return self
                        .update(Message::Containers(ContainerMsg::Selected(Some(container))));
                }
                DetailsMsg::Closed => {
                    self.details_for = None;
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
                    if is_blocked(self.backend.env()) {
                        self.error = self.backend.env().message.clone();
                        return Self::none();
                    }
                    match gosh_distrobox_core::models::CreateArgName::new(&name) {
                        Err(_) => {
                            let toast = self.toast(format!(
                                "Invalid container name {name:?}: must match [a-zA-Z0-9][a-zA-Z0-9_.-]* (row #96)."
                            ));
                            return toast;
                        }
                        Ok(arg_name) => {
                            use gosh_distrobox_core::models::CreateArgs;
                            let backend = Arc::clone(&self.backend);
                            let label = format!("Clone to {name}");
                            let args = CreateArgs {
                                init: false,
                                nvidia: false,
                                home_path: None,
                                image: String::new(),
                                name: arg_name,
                                volumes: vec![],
                            };
                            self.dialog = None;
                            let toast = self.toast(format!("Cloning {source} to {name}…"));
                            let spawn = Self::run(async move {
                                let result = backend.clone_container(&source, args).await;
                                Message::Tasks(TaskMsg::Started { label, result })
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
                    // container's apps. Position 3 = Apps in Page::ALL.
                    if let Some(c) = self.containers.iter().find(|c| c.name == name).cloned() {
                        self.nav_model.activate_position(3);
                        return self.update(Message::Containers(ContainerMsg::Selected(Some(c))));
                    }
                }
                DetailsMsg::TerminalRequested(name) => {
                    // Terminal page lands in T9 — record as a toast, not a
                    // dead button (row #63 disabled state becomes a message).
                    let toast = self.toast(format!(
                        "Terminal for {name} lands in T9 — container must be running."
                    ));
                    return toast;
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
                        ActiveDialog::Confirm(spec) => match spec.action {
                            ConfirmAction::RemoveContainer(name) => {
                                if is_blocked(self.backend.env()) {
                                    self.error = self.backend.env().message.clone();
                                    return Self::none();
                                }
                                if !self.busy.insert(format!("remove:{name}")) {
                                    return Self::none();
                                }
                                let backend = Arc::clone(&self.backend);
                                return Self::run(async move {
                                    let result = backend
                                        .remove_container(&name)
                                        .await
                                        .map(|_| format!("{name} deleted"));
                                    Message::Containers(ContainerMsg::ActionFinished(result))
                                });
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
                                return Self::run(async move {
                                    let result = backend
                                        .stop_all_containers()
                                        .await
                                        .map(|_| "All containers stopped".to_string());
                                    Message::Containers(ContainerMsg::ActionFinished(result))
                                });
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
                                        let label = format!("Upgrade {name}");
                                        Self::run(async move {
                                            let result = backend.upgrade_container(&name).await;
                                            Message::Tasks(TaskMsg::Started { label, result })
                                        })
                                    })
                                    .collect();
                                if spawns.is_empty() {
                                    return Self::none();
                                }
                                return Task::batch(spawns);
                            }
                        },
                        ActiveDialog::Clone { .. } => {
                            // Clone uses its own primary button
                            // (CloneConfirmed); confirming a stale dialog
                            // state is a no-op.
                            return Self::none();
                        }
                    }
                }
            },
            Message::Apps(msg) => match msg {
                AppMsg::LoadRequested(container) => {
                    self.loading.apps = true;
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend.container_apps(&container).await;
                        Message::Apps(AppMsg::Loaded(result))
                    });
                }
                AppMsg::Loaded(result) => {
                    self.loading.apps = false;
                    match result {
                        Ok(apps) => self.apps = apps,
                        Err(e) => self.error = Some(Self::error_text(&e)),
                    }
                }
                AppMsg::BinariesLoadRequested(container) => {
                    self.loading.binaries = true;
                    let backend = Arc::clone(&self.backend);
                    return Self::run(async move {
                        let result = backend.exported_binaries(&container).await;
                        Message::Apps(AppMsg::BinariesLoaded(result))
                    });
                }
                AppMsg::BinariesLoaded(result) => {
                    self.loading.binaries = false;
                    match result {
                        Ok(bins) => self.exported_binaries = bins,
                        Err(e) => self.error = Some(Self::error_text(&e)),
                    }
                }
            },
            Message::Images(msg) => match msg {
                ImageMsg::LoadRequested => {
                    self.loading.images = true;
                    return Self::load_images(&self.backend);
                }
                ImageMsg::Loaded(result) => {
                    self.loading.images = false;
                    match result {
                        Ok(images) => self.images = images,
                        Err(e) => self.error = Some(Self::error_text(&e)),
                    }
                }
            },
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
                TaskMsg::Started { label, result } => match result {
                    Ok(id) => {
                        self.tasks.insert(
                            id,
                            TaskView {
                                label,
                                output: Vec::new(),
                                completed: false,
                                success: false,
                                started_at: std::time::Instant::now(),
                            },
                        );
                    }
                    Err(e) => self.error = Some(Self::error_text(&e)),
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
                    if !success {
                        let label = self
                            .tasks
                            .get(&id)
                            .map(|v| v.label.clone())
                            .unwrap_or_else(|| "Task".to_string());
                        let toast = self.toast(format!("{label} failed — see output for details"));
                        return toast;
                    }
                }
                TaskMsg::CancelRequested(id) => {
                    // Synchronous registry call — `cancel` takes no lock
                    // across blocking calls, so this is safe in `update`.
                    if self.backend.cancel_task(id) {
                        if let Some(view) = self.tasks.get_mut(&id) {
                            view.completed = true;
                        }
                        return Self::done(Message::Tasks(TaskMsg::Cancelled(id)));
                    }
                }
                TaskMsg::Cancelled(_) => {
                    // Mirror already marked in `CancelRequested`; the arm
                    // exists so Activity-page producers (T11) typecheck.
                }
                TaskMsg::ClearCompleted => {
                    self.tasks.retain(|_, v| !v.completed);
                }
                TaskMsg::Expired(ids) => {
                    for id in ids {
                        self.tasks.remove(&id);
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
            Message::Env(EnvMsg::Probed(_)) => {
                // No producer in T3 (the probe runs synchronously in `init`);
                // the variant exists so the match stays exhaustive per the §2.3
                // draft, and re-probe buttons land in T9/T12.
            }
            Message::Ui(msg) => match msg {
                UiMsg::DismissError => self.error = None,
                UiMsg::ToastClosed(id) => {
                    self.toasts.remove(id);
                }
                UiMsg::CopiedToClipboard(what) => {
                    let toast = self.toast(format!("Copied {what} to clipboard"));
                    return toast;
                }
            },
        }
        Self::none()
    }

    /// T5 subscriptions (§3.4): (a) per-task output streams, keyed by `TaskId`
    /// so iced tears each down when the task leaves `self.tasks`; (b) the TTL
    /// sweep tick. The tick carries nothing — iced subscriptions cannot borrow
    /// `self.backend` (the builder is a plain `fn`), so the sweep runs in the
    /// `ExpiredTick` arm via `BACKEND`, and the resulting ids flow back as
    /// `TaskMsg::Expired`. Config watching lands in T12.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            Subscription::batch(self.tasks.keys().copied().map(task_output_subscription)),
            cosmic::iced::time::every(std::time::Duration::from_secs(TASK_SWEEP_INTERVAL_SECS))
                .map(|_| Message::Tasks(TaskMsg::ExpiredTick)),
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
            // Details pushes over Containers (row #53 back pops).
            Page::Dashboard => self.view_dashboard(),
            Page::Containers => match &self.details_for {
                Some(container) => views::view_details(container, self.upgrading(container)),
                None => self.view_containers_page(),
            },
            Page::Images => self.view_images(),
            Page::Apps => self.view_apps(),
            Page::Stats => self.view_stats(),
        };
        let body: cosmic::Element<'_, Self::Message> = match self.error.clone() {
            None => page,
            Some(err) => widget::Column::new()
                .push(
                    widget::Row::new()
                        .push(widget::text::body(err).width(Length::Fill))
                        .push(
                            widget::button::standard("Dismiss")
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
        // Row #53: back button on the details page; Refresh otherwise.
        if self.details_for.is_some() && self.active_page() == Page::Containers {
            return vec![
                widget::button::standard("Back")
                    .on_press(Message::Details(DetailsMsg::Closed))
                    .into(),
            ];
        }
        vec![
            widget::button::standard("Refresh")
                .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                .into(),
        ]
    }

    /// Row #44 (FAB → header): the create affordance lives here, not in a
    /// floating button. "New Container" on the Containers page (the T7
    /// wizard consumes the request); Refresh beside it on every page.
    /// `header_end` (not `header_start`) per the shell contract — Refresh
    /// in start is the T3 leftover this replaces.
    fn header_end(&self) -> Vec<cosmic::Element<'_, Self::Message>> {
        match self.active_page() {
            Page::Containers if self.details_for.is_none() => vec![
                widget::button::suggested("New Container")
                    .on_press(Message::Containers(ContainerMsg::NewContainerRequested))
                    .into(),
            ],
            _ => vec![],
        }
    }

    fn dialog(&self) -> Option<cosmic::Element<'_, Self::Message>> {
        views::dialog_view(&self.dialog)
    }
}

impl App {
    /// Whether this container has an upgrade task in flight (details tile
    /// inline spinner, row #60). Matches `TaskMsg::Started` labels
    /// (`Upgrade {name}`) for incomplete mirror entries — the data T5
    /// already stores, so the button disables while its task runs (and N
    /// presses cannot spawn N upgrades).
    fn upgrading(&self, container: &ContainerInfo) -> bool {
        let want = format!("Upgrade {}", container.name);
        self.tasks.values().any(|v| !v.completed && v.label == want)
    }

    /// Dashboard page: counts + task rows from the T5 mirror.
    fn view_dashboard(&self) -> cosmic::Element<'_, Message> {
        let running = running_count(&self.containers);
        let stopped = stopped_count(&self.containers);
        let total = self.containers.len();
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
            running,
            stopped,
            total,
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
            return widget::container(widget::text::body("Loading containers…"))
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
                    let retry: cosmic::Element<'_, Message> = widget::button::standard("Retry")
                        .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                        .into();
                    retry
                })
                .spacing(12)
                .into();
        }
        if self.containers.is_empty() {
            return views::empty_state(
                "document-open-symbolic",
                "No containers found.".to_string(),
                "Create your first container to get started.".to_string(),
                None,
            );
        }
        let mut col = widget::Column::new().spacing(8);
        for c in &self.containers {
            let selected = self.selected_container.as_deref() == Some(c.name.as_str());
            col = col.push(views::container_row(c, selected));
        }
        widget::scrollable(col).into()
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
