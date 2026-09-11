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

use crate::message::{
    AppMsg, ContainerMsg, EnvMsg, ImageMsg, Message, StatsMsg, UiMsg, is_blocked,
};
use cosmic::app::{Core, Task};
use cosmic::iced::{Length, Subscription};
use cosmic::widget::{self, nav_bar};
use gosh_distrobox_core::models::{AppInfo, ContainerInfo, ContainerStats, ExportedBinary};
use gosh_distrobox_core::{Backend, CoreError, CoreFailure};
use std::sync::Arc;

/// Reverse-domain id (D15-adjacent): same string as the gschema id, the
/// metainfo `<id>`, the `.desktop` `Icon=`, and the `cosmic-config` id (§5.2).
pub const APP_ID: &str = "io.github.gosh_distrobox_manager";

/// Nav pages for the T3 read-only browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Containers,
    Images,
    Apps,
    Stats,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Containers => "Containers",
            Page::Images => "Images",
            Page::Apps => "Apps",
            Page::Stats => "Stats",
        }
    }

    const ALL: [Page; 4] = [Page::Containers, Page::Images, Page::Apps, Page::Stats];
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
}

impl App {
    fn active_page(&self) -> Page {
        self.nav_model
            .active_data::<Page>()
            .copied()
            .unwrap_or(Page::Containers)
    }

    /// No-op task.
    fn none() -> Task<Message> {
        Task::none()
    }

    /// Synchronous follow-up message (explicit `Action::App` form — never
    /// fights inference, §2.3). First producer lands with T6's action buttons.
    #[allow(dead_code)]
    fn done(m: Message) -> Task<Message> {
        Task::done(cosmic::Action::App(m))
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

    fn view_containers(&self) -> cosmic::Element<'_, Message> {
        if self.loading.containers {
            return widget::container(widget::text::body("Loading containers…"))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        if self.containers.is_empty() {
            return widget::container(widget::text::body("No containers found."))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }
        let mut list = widget::list_column::list_column();
        for c in &self.containers {
            let selected = self.selected_container.as_deref() == Some(c.name.as_str());
            let status = format!("{:?}", c.status);
            let row = widget::Row::new()
                .push(widget::text::body(c.name.clone()).width(Length::Fill))
                .push(widget::text::caption(c.image.clone()))
                .push(widget::text::caption(status))
                .spacing(12);
            // `list_column` takes `ListButton` items; the row selects the
            // container and is keyboard-activatable (a11y, REVIEW UX-16).
            let item = widget::list::button(row)
                .on_press(Message::Containers(ContainerMsg::Selected(Some(c.clone()))))
                .selected(selected);
            list = list.add(item);
        }
        widget::scrollable(list.into_element()).into()
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
        // Lazy-load each domain on first visit; containers load at init.
        match self.active_page() {
            Page::Containers => {
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
                    if let Err(e) = result {
                        self.error = Some(Self::error_text(&e));
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
            Message::Env(EnvMsg::Probed(_)) => {
                // No producer in T3 (the probe runs synchronously in `init`);
                // the variant exists so the match stays exhaustive per the §2.3
                // draft, and re-probe buttons land in T9/T12.
            }
            Message::Ui(UiMsg::DismissError) => self.error = None,
        }
        Self::none()
    }

    /// No subscriptions in T3: no live tasks yet (those land with T5's
    /// `Subscription::run_with` wiring), no config watching yet (T12).
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }

    fn view(&self) -> cosmic::Element<'_, Self::Message> {
        let page = match self.active_page() {
            Page::Containers => self.view_containers(),
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
        widget::container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }

    fn header_start(&self) -> Vec<cosmic::Element<'_, Self::Message>> {
        vec![
            widget::button::standard("Refresh")
                .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                .into(),
        ]
    }
}
