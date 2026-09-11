//! Page views: Dashboard, Containers, Details (T6, ux.md §6.2–§6.5).
//!
//! Shared pieces (§3.3/§3.4/§3.6): `empty_state` (the ~20-times-repeated
//! icon→title→body→action pattern), `gate_view` (the three global gates —
//! blocked / not-installed / load error — implemented ONCE at the shell
//! level so every page inherits them), `container_row` (distro icon, name,
//! status dot+text, chevron), and the single-modal confirm dialog.
//!
//! Conventions honored: keep content during refresh (inline spinners, never
//! a full-page spinner over displayed data); every mutation reports through
//! the toaster (§3.4); destructive actions use the destructive button class
//! with a warning icon and the consequence in the body.

use crate::icons::{distro_icon, is_running, status_label};
use crate::message::{ContainerMsg, DetailsMsg, DialogMsg, Message, TaskMsg};
use cosmic::iced::Length;
use cosmic::widget::toaster::{Toast, Toasts};
use cosmic::widget::{self, nav_bar};
use gosh_distrobox_core::models::ContainerInfo;

/// Pages in the nav bar. Dashboard first (Flutter rail order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Containers,
    Images,
    Packages,
    Updates,
    Apps,
    Stats,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::Containers => "Containers",
            Page::Images => "Images",
            Page::Packages => "Packages",
            Page::Updates => "Updates",
            Page::Apps => "Apps",
            Page::Stats => "Stats",
        }
    }

    pub const ALL: [Page; 7] = [
        Page::Dashboard,
        Page::Containers,
        Page::Images,
        Page::Packages,
        Page::Updates,
        Page::Apps,
        Page::Stats,
    ];
}

/// Which page is active (nav-bar data lookup with Containers fallback).
pub fn active_page(nav_model: &nav_bar::Model) -> Page {
    nav_model
        .active_data::<Page>()
        .copied()
        .unwrap_or(Page::Containers)
}

/// Activate a tab by page (replaces positional `activate_position` — the
/// T6/T7 numeric literals broke silently when pages were added).
pub fn activate_page(nav_model: &mut nav_bar::Model, page: Page) {
    if let Some(pos) = Page::ALL.iter().position(|p| *p == page) {
        nav_model.activate_position(pos as u16);
    }
}

/// Activate the Containers tab (row #23 "View all" — dead in Flutter).
pub fn activate_containers(nav_model: &mut nav_bar::Model) {
    activate_page(nav_model, Page::Containers);
}

/// Shared empty state (§3.4): icon → title → body → optional action.
pub fn empty_state(
    icon: &'static str,
    title: String,
    body: String,
    action: Option<cosmic::Element<'static, Message>>,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::icon::from_name(icon).size(48).icon())
        .push(widget::text::title3(title))
        .push(widget::text::body(body))
        .spacing(12)
        .align_x(cosmic::iced::Alignment::Center);
    if let Some(action) = action {
        col = col.push(action);
    }
    widget::container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

/// The three global gates (§3.4), evaluated in order. `Some` = gate active
/// (render this INSTEAD of the page); `None` = render the page.
pub fn gate(
    blocked: Option<String>,
    distrobox_installed: bool,
    loading_anything: bool,
) -> Option<cosmic::Element<'static, Message>> {
    if let Some(message) = blocked {
        // Row #12: Environment Blocked (icon, title, message, Check Again).
        // "Check Again" re-runs refresh — the probe itself runs in `init`,
        // and refresh re-queries version/installed state. No re-probe button
        // until T9/T12 (EnvMsg::Probed); refresh is the honest action today.
        return Some(empty_state(
            "dialog-error-symbolic",
            "Environment Blocked".to_string(),
            message,
            Some(
                widget::button::suggested("Check Again")
                    .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                    .into(),
            ),
        ));
    }
    if !distrobox_installed && !loading_anything {
        // Row #13: Distrobox Not Found.
        return Some(empty_state(
            "dialog-warning-symbolic",
            "Distrobox Not Found".to_string(),
            "Distrobox is required to manage Linux containers. Please install it to use Gosh Distrobox Manager.".to_string(),
            Some(
                widget::button::suggested("Check Again")
                    .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                    .into(),
            ),
        ));
    }
    None
}

/// Shared container row (§6.3, rows #24/#37–#39): distro icon, name, status
/// dot + text, chevron. Tap → details; inline Stop when running (row #25).
pub fn container_row(
    container: &ContainerInfo,
    selected: bool,
) -> cosmic::Element<'static, Message> {
    // Rows #24/#34/#38/#45: distro icon, name, status DOT + text, chevron.
    // The dot is a ● glyph in default text colour + the `status_label` copy.
    // `status_color()` (theme success/accent/warning/control) exists and is
    // unit-tested, but NEITHER `Text::color` NOR `SelectableText::color`
    // satisfy `<Theme as Catalog>::Class: From<StyleFn>` in this iced rev —
    // coloured text is structurally unavailable. No hard-coded colours
    // (§3.5 holds); the coloured dot lands when the bound lifts.
    let status = status_label(&container.status);
    let row = widget::Row::new()
        .push(
            widget::icon::from_name(distro_icon(&container.image))
                .size(20)
                .icon(),
        )
        .push({
            let status_line: cosmic::Element<'static, Message> =
                widget::text::caption(format!("● {status}")).into();
            let name_col: cosmic::Element<'static, Message> = widget::Column::new()
                .push(widget::text::body(container.name.clone()))
                .push(status_line)
                .spacing(2)
                .width(Length::Fill)
                .into();
            name_col
        })
        .push({
            let chev: cosmic::Element<'static, Message> =
                widget::icon::from_name("go-next-symbolic")
                    .size(16)
                    .icon()
                    .into();
            chev
        })
        .spacing(12)
        .align_y(cosmic::iced::Alignment::Center);
    let mut list = widget::list_column::list_column();
    list = list.add(
        widget::list::button(row)
            .on_press(Message::Details(DetailsMsg::OpenRequested(
                container.clone(),
            )))
            .selected(selected),
    );
    let mut col = widget::Column::new().push(list.into_element());
    if is_running(&container.status) {
        col = col.push({
            let stop: cosmic::Element<'static, Message> = widget::button::text("Stop")
                .on_press(Message::Containers(ContainerMsg::StopRequested(
                    container.name.clone(),
                )))
                .into();
            stop
        });
    }
    col.into()
}

/// Dashboard (rows #9–#34): status card, stat tiles, active tasks (only when
/// non-empty), container preview (first 5), quick actions. NO pull-to-refresh
/// (row #32, touch idiom — header Refresh button instead); content stays
/// during refresh (row #11).
pub fn view_dashboard(
    containers: &[ContainerInfo],
    running: usize,
    stopped: usize,
    total: usize,
    active_task_count: usize,
    task_rows: Vec<cosmic::Element<'static, Message>>,
    error: Option<String>,
) -> cosmic::Element<'static, Message> {
    let healthy = error.is_none();
    let mut col = widget::Column::new().spacing(16);

    // Status card (rows #14–#15).
    let mut status = widget::Column::new()
        .push(widget::text::caption_heading("SYSTEM STATUS"))
        .push(widget::text::title4(if healthy {
            "All Systems Operational"
        } else {
            "Attention Required"
        }))
        .push(widget::text::body(if total == 0 {
            "No containers configured. Create one to get started!".to_string()
        } else {
            format!(
                "{running} of {total} container{} running.",
                if total != 1 { "s" } else { "" }
            )
        }))
        .spacing(4);
    if let Some(err) = error {
        let warn: cosmic::Element<'static, Message> = widget::warning(err).into();
        status = status.push(warn);
    }
    col = col.push({
        let status_el: cosmic::Element<'static, Message> = status.into();
        status_el
    });

    // Stat tiles (rows #16–#18).
    col = col.push({
        let tiles: cosmic::Element<'static, Message> = widget::Row::new()
            .push(stat_tile("Total Containers".to_string(), total.to_string()))
            .push(stat_tile("Running".to_string(), running.to_string()))
            .push(stat_tile("Stopped".to_string(), stopped.to_string()))
            .spacing(12)
            .into();
        tiles
    });

    // Active tasks, conditional (rows #19–#21). Rows come from the app
    // (mirror state) — this module stays stateless.
    if !task_rows.is_empty() {
        col = col.push({
            let t: cosmic::Element<'static, Message> =
                widget::text::caption_heading("ACTIVE TASKS").into();
            t
        });
        for row in task_rows {
            col = col.push(row);
        }
        let _ = active_task_count;
    }

    // Containers preview (rows #22–#27): first 5 + View all.
    col = col.push({
        let header: cosmic::Element<'static, Message> = widget::Row::new()
            .push(widget::text::caption_heading("CONTAINERS").width(Length::Fill))
            .push({
                let view_all: cosmic::Element<'static, Message> = widget::button::text("View all")
                    .on_press(Message::Containers(ContainerMsg::ViewAllRequested))
                    .into();
                view_all
            })
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        header
    });
    if containers.is_empty() {
        col = col.push(empty_state(
            "document-open-symbolic",
            "No containers yet".to_string(),
            "Create your first container to get started".to_string(),
            None,
        ));
    } else {
        for container in containers.iter().take(5) {
            col = col.push(container_row(container, false));
        }
    }

    // Quick actions (rows #28–#31). New Container → wizard; Upgrade All → confirm → per-container tasks (row #29
    // implemented for real, not a redirect snackbar); Stop All + confirm
    // (row #30, destructive class); Refresh (row #31).
    col = col.push({
        let t: cosmic::Element<'static, Message> =
            widget::text::caption_heading("QUICK ACTIONS").into();
        t
    });
    col = col.push({
        let new_btn: cosmic::Element<'static, Message> = widget::button::standard("New Container")
            .on_press(Message::Containers(ContainerMsg::NewContainerRequested))
            .into();
        let upgrade_btn: cosmic::Element<'static, Message> =
            widget::button::standard("Upgrade All")
                .on_press(Message::Containers(ContainerMsg::UpgradeAllRequested))
                .into();
        let actions: cosmic::Element<'static, Message> = widget::Row::new()
            .push(new_btn)
            .push(upgrade_btn)
            .spacing(12)
            .into();
        actions
    });
    col = col.push({
        let stop_all: cosmic::Element<'static, Message> = widget::button::standard("Stop All")
            .on_press(Message::Containers(ContainerMsg::StopAllRequested))
            .into();
        let refresh: cosmic::Element<'static, Message> = widget::button::standard("Refresh")
            .on_press(Message::Containers(ContainerMsg::RefreshRequested))
            .into();
        let row: cosmic::Element<'static, Message> = widget::Row::new()
            .push(stop_all)
            .push(refresh)
            .spacing(12)
            .into();
        row
    });

    widget::scrollable(col).into()
}

fn stat_tile(title: String, value: String) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::title3(value.to_string()))
        .push(widget::text::caption(title.to_string()))
        .spacing(4)
        .width(Length::Fill)
        .into()
}

/// Active-task row (rows #19–#21): label + spinner/done + cancel while
/// running. `completed`/`success` come from the T5 mirror — no
/// string-sniffing (§3.1).
pub fn task_row(
    id: gosh_distrobox_core::TaskId,
    label: String,
    completed: bool,
    success: bool,
) -> cosmic::Element<'static, Message> {
    // Rows #20/#21: spinner while running; success check vs failure icon
    // when done (a failed task must not render a success check).
    let mut row = widget::Row::new()
        .push(widget::text::body(label).width(Length::Fill))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);
    if completed {
        row = row.push({
            let icon = if success {
                "object-select-symbolic"
            } else {
                "dialog-error-symbolic"
            };
            let done: cosmic::Element<'static, Message> =
                widget::icon::from_name(icon).size(16).icon().into();
            done
        });
    } else {
        row = row.push({
            let cancel: cosmic::Element<'static, Message> = widget::button::text("Cancel")
                .on_press(Message::Tasks(TaskMsg::CancelRequested(id)))
                .into();
            cancel
        });
    }
    row.into()
}

/// Details page (rows #53–#65): nested under Containers. Header back button
/// pops (row #53); hero card with image chip (#56, tap copies — #57);
/// status card with Stop when running (#58–#59); Upgrade tile with inline
/// spinner (#60, shared task component); Applications tile → Apps (#61);
/// Clone tile → dialog (#62); Open Terminal tile, disabled when stopped
/// (#63); Danger Zone delete + confirm (#64). Start/restart (row #65) is
/// P1-blocking per §4.1 — NO start op exists in the backend, so no Start
/// button is rendered (a button that cannot work is worse than none; the
/// not-running copy says terminal/upgrades need a running container).
pub fn view_details(
    container: &ContainerInfo,
    upgrading: bool,
) -> cosmic::Element<'static, Message> {
    let running = is_running(&container.status);
    let mut col = widget::Column::new().spacing(16);

    // Hero card (rows #56–#57): icon, name, image chip (tap = copy).
    col = col.push({
        let hero: cosmic::Element<'static, Message> = widget::Column::new()
            .push({
                let hero_row: cosmic::Element<'static, Message> = widget::Row::new()
                    .push(
                        widget::icon::from_name(distro_icon(&container.image))
                            .size(32)
                            .icon(),
                    )
                    .push(widget::text::title3(container.name.clone()).width(Length::Fill))
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
                    .into();
                hero_row
            })
            .push({
                let copy: cosmic::Element<'static, Message> =
                    widget::button::text(container.image.clone())
                        .on_press(Message::Details(DetailsMsg::CopyImageRequested(
                            container.image.clone(),
                        )))
                        .into();
                copy
            })
            .spacing(8)
            .into();
        hero
    });

    // Status card (rows #58–#59).
    let mut status_row = widget::Row::new()
        .push(widget::text::body(status_label(&container.status)).width(Length::Fill))
        .push(widget::text::caption(format!("ID: {}", container.id)))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);
    if running {
        status_row = status_row.push({
            let stop: cosmic::Element<'static, Message> = widget::button::standard("Stop")
                .on_press(Message::Details(DetailsMsg::StopRequested(
                    container.name.clone(),
                )))
                .into();
            stop
        });
    }
    col = col.push({
        let heading: cosmic::Element<'static, Message> =
            widget::text::caption_heading("CONTAINER STATUS").into();
        let status_el: cosmic::Element<'static, Message> = status_row.into();
        let card: cosmic::Element<'static, Message> = widget::Column::new()
            .push(heading)
            .push(status_el)
            .spacing(4)
            .into();
        card
    });

    // Quick actions (rows #60–#63). Push widgets directly (T3 pattern):
    // a typed Column accepts widgets; pre-wrapping in Element breaks the
    // Theme/Renderer inference this iced rev needs.
    let upgrade_label = if upgrading {
        "Upgrading…"
    } else {
        "Upgrade Container"
    };
    let actions = widget::Column::new()
        .push(
            widget::button::standard(upgrade_label).on_press_maybe(if upgrading {
                None
            } else {
                Some(Message::Details(DetailsMsg::UpgradeRequested(
                    container.name.clone(),
                )))
            }),
        )
        .push(
            widget::button::standard("Applications").on_press(Message::Details(
                DetailsMsg::AppsRequested(container.name.clone()),
            )),
        )
        .push(
            widget::button::standard("Clone Container").on_press(Message::Details(
                DetailsMsg::CloneRequested(container.name.clone()),
            )),
        )
        .spacing(4);
    let actions = actions.push(widget::button::standard("Open Terminal").on_press_maybe(
        if running {
            Some(Message::Details(DetailsMsg::TerminalRequested(
                container.name.clone(),
            )))
        } else {
            None
        },
    ));
    col = col.push(
        widget::Column::new()
            .push(widget::text::caption_heading("QUICK ACTIONS"))
            .push(actions)
            .spacing(4),
    );

    // Danger zone (row #64): shared destructive confirm (§3.3).
    col = col.push(
        widget::Column::new()
            .push(widget::text::caption_heading("DANGER ZONE"))
            .push(
                widget::button::destructive("Delete Container").on_press(Message::Details(
                    DetailsMsg::RemoveRequested(container.name.clone()),
                )),
            )
            .spacing(4),
    );

    widget::scrollable(col).into()
}

/// Clone dialog body state lives in `App` (T6): source + editable name.
/// Rendered through `Application::dialog()` (single modal slot, §3.3).
/// `CloneNameChanged` (not `CloneConfirmed`) carries keystrokes — confirming
/// with an empty source would clone nothing; the Clone button sends the real
/// `CloneConfirmed { source, name }`.
pub fn clone_dialog(source: &str, name: &str) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::body(format!(
            "Clone {source} to a new container:"
        )))
        .push({
            let input: cosmic::Element<'static, Message> =
                widget::text_input::text_input("New container name", name.to_string())
                    .on_input(|s| Message::Details(DetailsMsg::CloneNameChanged(s)))
                    .into();
            input
        })
        .spacing(8)
        .into()
}

/// The single modal (§3.3): confirm dialogs share copy via `ConfirmSpec`
/// (fixing the card-vs-details divergence, row #52); the clone dialog is
/// the only input dialog in T6 scope.
pub fn dialog_view(
    dialog: &Option<crate::app::ActiveDialog>,
) -> Option<cosmic::Element<'static, Message>> {
    let dialog = dialog.as_ref()?;
    match dialog {
        crate::app::ActiveDialog::Confirm(spec) => {
            let primary: cosmic::Element<'static, Message> = if spec.destructive {
                widget::button::destructive(spec.confirm_label.clone())
                    .on_press(Message::Dialog(DialogMsg::Confirmed))
                    .into()
            } else {
                widget::button::suggested(spec.confirm_label.clone())
                    .on_press(Message::Dialog(DialogMsg::Confirmed))
                    .into()
            };
            Some(
                widget::dialog()
                    .title(spec.title.clone())
                    .body(spec.body.clone())
                    .primary_action(primary)
                    .secondary_action({
                        let cancel: cosmic::Element<'static, Message> =
                            widget::button::standard("Cancel")
                                .on_press(Message::Dialog(DialogMsg::Cancelled))
                                .into();
                        cancel
                    })
                    .into(),
            )
        }
        crate::app::ActiveDialog::Clone { source, name } => Some(
            widget::dialog()
                .title(format!("Clone {source}"))
                .control(clone_dialog(source, name))
                .primary_action({
                    let clone: cosmic::Element<'static, Message> =
                        widget::button::suggested("Clone")
                            .on_press(Message::Details(DetailsMsg::CloneConfirmed {
                                source: source.clone(),
                                name: name.clone(),
                            }))
                            .into();
                    clone
                })
                .secondary_action({
                    let cancel: cosmic::Element<'static, Message> =
                        widget::button::standard("Cancel")
                            .on_press(Message::Dialog(DialogMsg::Cancelled))
                            .into();
                    cancel
                })
                .into(),
        ),
    }
}

/// Wrap the page in the toaster (§3.4): every mutation reports through
/// toasts, never bare snackbars — and package/backups pages' silent
/// failures become visible here by construction.
pub fn with_toasts<'a>(
    toasts: &'a Toasts<Message>,
    page: cosmic::Element<'a, Message>,
) -> cosmic::Element<'a, Message> {
    widget::toaster(toasts, page)
}

/// Push a toast; returns the follow-up `Task` (auto-dismiss timer).
/// `Toasts::push` yields an iced `Task<Message>`; map it into the app's
/// `Task<Action<Message>>` via `From` (`Action::App`).
pub fn push_toast(toasts: &mut Toasts<Message>, text: String) -> cosmic::app::Task<Message> {
    toasts.push(Toast::new(text)).map(cosmic::Action::App)
}
