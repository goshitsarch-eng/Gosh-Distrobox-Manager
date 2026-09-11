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
    /// Images page's OWN load error (O3): the global `error` banner already
    /// reports failures everywhere — passing it into the images view made
    /// every unrelated failure render as "Could not load images". Set on
    /// `Images Loaded(Err)`, cleared on request/success.
    images_error: Option<String>,
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
    /// Terminal page state (T9, rows #66–#77 + D8).
    terminal: crate::terminal::TerminalState,
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
            images_error: None,
            packages: crate::packages::PackagesState::default(),
            terminal: crate::terminal::TerminalState::default(),
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
                            .map(|_| format!("{name} started"));
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
                        let toast = self.toast("No running containers to upgrade.".to_string());
                        return toast;
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
                                let label = format!("Install {package} in {container}");
                                return Self::run(async move {
                                    let result =
                                        backend.install_package(&container, &package).await;
                                    Message::Tasks(TaskMsg::Started { label, result })
                                });
                            }
                            ConfirmAction::RemovePackage { container, package } => {
                                if is_blocked(self.backend.env()) {
                                    self.error = self.backend.env().message.clone();
                                    return Self::none();
                                }
                                let backend = Arc::clone(&self.backend);
                                let label = format!("Remove {package} from {container}");
                                return Self::run(async move {
                                    let result = backend.remove_package(&container, &package).await;
                                    Message::Tasks(TaskMsg::Started { label, result })
                                });
                            }
                            ConfirmAction::UpgradeContainer(container) => {
                                if is_blocked(self.backend.env()) {
                                    self.error = self.backend.env().message.clone();
                                    return Self::none();
                                }
                                let backend = Arc::clone(&self.backend);
                                let label = format!("Upgrade {container}");
                                return Self::run(async move {
                                    let result = backend.upgrade_container(&container).await;
                                    Message::Tasks(TaskMsg::Started { label, result })
                                });
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
            Message::Packages(msg) => return self.update_packages(msg),
            Message::Updates(_) => return Self::none(), // namespace reserved (T9+)
            Message::Terminal(msg) => return self.update_terminal(msg),
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
                        let toast = self.toast("Enter a custom image URL first.".to_string());
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
                TaskMsg::Started { label, result } => match result {
                    Err(e) => {
                        // Row #122 (ux.md:504 "Route to toaster"): spawn
                        // failures toast AND banner — Flutter showed nothing.
                        let toast = self.toast(format!(
                            "Could not start {}: {}",
                            label,
                            Self::error_text(&e)
                        ));
                        // O2 second leg: a spawn failure must not strand the
                        // wizard on Progress (no task exists to drive it).
                        // Step back to Config with the error inline (the
                        // toast above already fired — no silent failure).
                        if let Some(w) = self.wizard.as_mut()
                            && w.step == crate::wizard::WizardStep::Progress
                            && label.starts_with("Create ")
                        {
                            w.step = crate::wizard::WizardStep::Config;
                            w.inline_error = Some(format!(
                                "Could not start creation: {}",
                                Self::error_text(&e)
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
                            && label.starts_with("Create ")
                        {
                            w.task_id = Some(id);
                        }
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
                            .unwrap_or_else(|| "Task".to_string());
                        let toast = self.toast(format!("{label} failed — see output for details"));
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
            Page::Updates => self.view_updates(),
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
            return vec![widget::button::standard("Back").on_press(msg).into()];
        }
        // O6: Updates owns Refresh+Upgrade All in `header_end` (row #123) —
        // the generic Refresh here would render it twice.
        if self.active_page() == Page::Updates {
            return vec![];
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
    /// Header actions (row #123): Updates gets Refresh + Upgrade All
    /// (`header_end` two buttons); Containers keeps New Container.
    fn header_end(&self) -> Vec<cosmic::Element<'_, Self::Message>> {
        match self.active_page() {
            Page::Containers if self.details_for.is_none() && self.wizard.is_none() => {
                vec![
                    widget::button::suggested("New Container")
                        .on_press(Message::Containers(ContainerMsg::NewContainerRequested))
                        .into(),
                ]
            }
            Page::Updates => vec![
                widget::button::standard("Refresh")
                    .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                    .into(),
                widget::button::suggested("Upgrade All")
                    .on_press(Message::Containers(ContainerMsg::UpgradeAllRequested))
                    .into(),
            ],
            _ => vec![],
        }
    }

    fn dialog(&self) -> Option<cosmic::Element<'_, Self::Message>> {
        // Single modal slot (§3.3): wizard sub-dialogs (volume, image
        // details) take precedence over the page-level dialog.
        if let Some(wizard) = &self.wizard
            && let Some(volume) = &wizard.volume_dialog
        {
            return Some(
                widget::dialog()
                    .title("Add Volume")
                    .control(crate::wizard_view::volume_dialog_body(volume))
                    .primary_action({
                        let add: cosmic::Element<'_, Message> = widget::button::suggested("Add")
                            .on_press(Message::Wizard(
                                crate::message::WizardMsg::VolumeDialogConfirmed,
                            ))
                            .into();
                        add
                    })
                    .secondary_action({
                        let cancel: cosmic::Element<'_, Message> =
                            widget::button::standard("Cancel")
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

    /// Package page view (T8, rows #106–#122).
    fn view_packages(&self) -> cosmic::Element<'_, Message> {
        use crate::packages as pkg;
        // Row #108: no-containers gate (Flutter `_buildNoContainersView`).
        // Without it an empty tree renders "Container Not Running" for a
        // container that does not exist — actively misleading.
        if self.containers.is_empty() {
            return crate::views::empty_state(
                "document-open-symbolic",
                "No containers found.".to_string(),
                "Create a container before managing packages.".to_string(),
                None,
            );
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
                widget::text_input::search_input("Search for packages...", st.query.clone())
                    .on_input(|s| Message::Packages(crate::message::PackagesMsg::QueryChanged(s)))
                    .on_submit(|_| Message::Packages(crate::message::PackagesMsg::SearchSubmitted))
                    .into();
            search
        });
        // PM badge (#114) + clear-search (#113).
        col = col.push({
            let row: cosmic::Element<'_, Message> = widget::Row::new()
                .push(widget::text::caption(format!(
                    "Package manager: {}",
                    pkg::PackagesState::badge(st.manager)
                )))
                .push(
                    widget::button::text("Clear search").on_press(Message::Packages(
                        crate::message::PackagesMsg::SearchCleared,
                    )),
                )
                .spacing(8)
                .into();
            row
        });
        // Quick actions: Install-from-box (#115, disabled when empty) +
        // Upgrade All + confirm (#116).
        col = col.push({
            let install_empty = st.query.trim().is_empty();
            let row: cosmic::Element<'_, Message> = widget::Row::new()
                .push(widget::button::standard("Install").on_press_maybe(
                    if running && !install_empty {
                        Some(Message::Packages(
                            crate::message::PackagesMsg::InstallFromBox,
                        ))
                    } else {
                        None
                    },
                ))
                .push(
                    widget::button::standard("Upgrade All").on_press_maybe(if running {
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
                // enter-command display. Terminal picker defaults to first.
                self.terminal.container = Some(name.clone());
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
                        let toast = self.toast("No terminal available to launch.".to_string());
                        return toast;
                    }
                };
                match self.backend.launch_terminal(&container, &terminal) {
                    Ok(()) => {
                        let toast =
                            self.toast(format!("Launched {} for {container}", terminal.name));
                        return toast;
                    }
                    Err(e) => {
                        let toast = self.toast(format!(
                            "Could not launch terminal: {}",
                            Self::error_text(&e)
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
                    let toast = self.toast(format!("Launch failed: {}", Self::error_text(&e)));
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
    fn terminal_list(&self) -> Vec<gosh_distrobox_core::backends::Terminal> {
        gosh_distrobox_core::backends::supported_terminals::builtin_terminals()
    }

    /// Package message router (T8, rows #106–#122).
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
                    Err(e) => self.toast(format!("Search failed: {}", Self::error_text(&e))),
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
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
                        title: "Remove Package".to_string(),
                        body: format!(
                            "Remove \"{name}\" from \"{container}\"?\n\nThis may also remove dependent packages."
                        ),
                        confirm_label: "Remove".to_string(),
                        destructive: true,
                        action: ConfirmAction::RemovePackage {
                            container,
                            package: name,
                        },
                    }));
                }
                Self::none()
            }
            PackagesMsg::UpgradeAllRequested => {
                if let Some(container) = self.packages.container.clone() {
                    self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
                        title: "Upgrade All Packages".to_string(),
                        body: format!("Upgrade all packages in \"{container}\"?"),
                        confirm_label: "Upgrade All".to_string(),
                        destructive: false,
                        action: ConfirmAction::UpgradeContainer(container),
                    }));
                }
                Self::none()
            }
            PackagesMsg::ManualCmdChanged(c) => {
                self.packages.manual_cmd = c;
                Self::none()
            }
            PackagesMsg::ManualRunRequested => {
                // B1 Unknown: T9 owns real execution — record the intent.
                self.toast("Manual commands run in T9 — container must be running.".to_string())
            }
        }
    }

    /// Install confirm helper (row #120): shared spec, non-destructive.
    fn confirm_install(&mut self, container: String, package: String) -> Task<Message> {
        self.dialog = Some(ActiveDialog::Confirm(ConfirmSpec {
            title: "Install Package".to_string(),
            body: format!("Install \"{package}\" in \"{container}\"?"),
            confirm_label: "Install".to_string(),
            destructive: false,
            action: ConfirmAction::InstallPackage { container, package },
        }));
        Self::none()
    }

    /// Whether this container is currently running (search/actions gate).
    fn packages_running(&self, container: &str) -> bool {
        self.containers
            .iter()
            .find(|c| c.name == container)
            .map(|c| crate::icons::is_running(&c.status))
            .unwrap_or(false)
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
                            w.inline_error = Some("Please select an image".to_string());
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
                            Err("Host path is required.".to_string())
                        } else if d.container.trim().is_empty() {
                            Err("Container path is required.".to_string())
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
                        Err("Please enter a container name".to_string())
                    } else {
                        match gosh_distrobox_core::models::CreateArgName::new(w.name.trim()) {
                            Err(e) => Err(format!("Invalid container name: {e}")),
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
                        let label = format!("Create {}", args.name);
                        if let Some(w) = self.wizard.as_mut() {
                            w.step = WizardStep::Progress;
                            w.inline_error = None;
                        }
                        return Self::run(async move {
                            let result = backend.create_container(args).await;
                            Message::Tasks(TaskMsg::Started { label, result })
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
        let want = format!("Upgrade {}", container.name);
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
