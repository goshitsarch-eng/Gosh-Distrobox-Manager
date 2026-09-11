//! Updates page (T9, ux.md §6.10, rows #123–#132).
//!
//! Header Refresh + Upgrade All (#123, `header_end`), gates (#124, shared),
//! summary header (#125), RUNNING/STOPPED sections (#126/#129 — stopped uses
//! a caption class, never `Opacity(0.7)` per §3.5), running cards with
//! READY/UPGRADING state + Upgrade button/inline spinner (#127–#128),
//! stopped cards with the start CTA (#130 — B5 `start`, dead in Flutter),
//! upgrade-all confirm + empty toast (#131), shared task progress (#132,
//! dashboard mirror rows, not a fifth dialog copy).

use crate::icons::{distro_icon, is_running};
use crate::message::{ContainerMsg, Message};
use crate::views::empty_state;
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::models::ContainerInfo;

/// Summary counts (row #125).
pub fn summary(containers: &[ContainerInfo]) -> (usize, usize) {
    let running = containers.iter().filter(|c| is_running(&c.status)).count();
    (running, containers.len() - running)
}

/// Running container card (rows #127–#128): distro icon, name, state chip,
/// image, Upgrade button or inline spinner while its task runs.
pub fn running_card(
    container: &ContainerInfo,
    upgrading: bool,
) -> cosmic::Element<'static, Message> {
    let state = if upgrading {
        "UPGRADING…"
    } else {
        "READY TO UPGRADE"
    };
    let mut col = widget::Column::new().spacing(8);
    col = col.push({
        let head: cosmic::Element<'static, Message> = widget::Row::new()
            .push({
                let icon: cosmic::Element<'static, Message> =
                    widget::icon::from_name(distro_icon(&container.image))
                        .size(32)
                        .icon()
                        .into();
                icon
            })
            .push(
                widget::Column::new()
                    .push(widget::text::body(container.name.clone()))
                    .push(widget::text::caption(state))
                    .push(widget::text::caption(container.image.clone()))
                    .spacing(2)
                    .width(Length::Fill),
            )
            .spacing(12)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        head
    });
    col = col.push({
        let action: cosmic::Element<'static, Message> = if upgrading {
            widget::Row::new()
                .push(widget::progress_bar::indeterminate_circular())
                .push(widget::text::caption("Upgrading…"))
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .into()
        } else {
            widget::button::suggested("Upgrade")
                .on_press(Message::Containers(ContainerMsg::UpgradeRequested(
                    container.name.clone(),
                )))
                .into()
        };
        action
    });
    col.into()
}

/// Stopped container card (rows #129–#130): dimmed via caption class (never
/// opacity), copy + Start CTA (B5 — the Flutter dead end, now a button).
pub fn stopped_card(container: &ContainerInfo) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(8);
    col = col.push({
        let head: cosmic::Element<'static, Message> = widget::Row::new()
            .push({
                let icon: cosmic::Element<'static, Message> =
                    widget::icon::from_name(distro_icon(&container.image))
                        .size(32)
                        .icon()
                        .into();
                icon
            })
            .push(
                widget::Column::new()
                    .push(widget::text::body(container.name.clone()))
                    .push(widget::text::caption("STOPPED"))
                    .push(widget::text::caption(container.image.clone()))
                    .spacing(2)
                    .width(Length::Fill),
            )
            .spacing(12)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        head
    });
    col = col.push(widget::warning("Start the container to enable upgrades."));
    col = col.push(
        widget::button::standard("Start").on_press(Message::Containers(
            ContainerMsg::StartRequested(container.name.clone()),
        )),
    );
    col.into()
}

/// Task mirror rows for the page's upgrade tasks (row #132 — shared
/// component, not a fifth `_UpgradeProgressDialog` copy). Reuses the
/// dashboard `task_row`.
pub fn upgrade_task_rows(
    tasks: &std::collections::BTreeMap<gosh_distrobox_core::TaskId, crate::app::TaskView>,
) -> Vec<cosmic::Element<'static, Message>> {
    tasks
        .iter()
        .filter(|(_, v)| v.label.starts_with("Upgrade "))
        .map(|(id, v)| crate::views::task_row(*id, v.label.clone(), v.completed, v.success))
        .collect()
}

/// Full updates page body.
pub fn view_updates(
    containers: &[ContainerInfo],
    upgrading: &dyn Fn(&ContainerInfo) -> bool,
    task_rows: Vec<cosmic::Element<'static, Message>>,
) -> cosmic::Element<'static, Message> {
    if containers.is_empty() {
        return empty_state(
            "document-open-symbolic",
            "No Containers".to_string(),
            "Create a container to manage updates.".to_string(),
            None,
        );
    }
    let (running_n, stopped_n) = summary(containers);
    let total = containers.len();
    let mut col = widget::Column::new().spacing(12);
    col = col.push(widget::text::title3(format!(
        "{total} Container{} Available",
        if total != 1 { "s" } else { "" }
    )));
    col = col.push(widget::text::body(format!(
        "{running_n} running, {stopped_n} stopped. Upgrade running containers to update their packages."
    )));
    let running: Vec<&ContainerInfo> = containers
        .iter()
        .filter(|c| is_running(&c.status))
        .collect();
    let stopped: Vec<&ContainerInfo> = containers
        .iter()
        .filter(|c| !is_running(&c.status))
        .collect();
    if !running.is_empty() {
        col = col.push(widget::text::caption_heading("RUNNING CONTAINERS"));
        for c in running {
            col = col.push(running_card(c, upgrading(c)));
        }
    }
    if !stopped.is_empty() {
        col = col.push(widget::text::caption_heading("STOPPED CONTAINERS"));
        for c in stopped {
            col = col.push(stopped_card(c));
        }
    }
    if !task_rows.is_empty() {
        col = col.push(widget::text::caption_heading("UPGRADE TASKS"));
        for row in task_rows {
            col = col.push(row);
        }
    }
    widget::scrollable(col).into()
}
