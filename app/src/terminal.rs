//! Terminal-launch page (T9, ux.md §6.6, rows #66–#77).
//!
//! Info header (#67: icon, name, status pill, monospace image), not-running
//! banner WITH a working Start CTA (#68 — B5 lands before banners freeze,
//! D7), enter-command display selectable + monospace (#69), copy icon +
//! filled Copy Command (#70–#72, running only), Upgrade Packages routed to
//! the shared task flow (#73 — not a bare snackbar), Stop + confirm (#74),
//! details rows (#75), honest help text (#76 — command display, not
//! emulation), NO terminal emulation (#77 — dropped per §4.5).
//!
//! Launch (D8): terminal picker (supported_terminals revival) + Launch
//! button spawns `terminal … <enter argv>` through the env-mapped runner.
//! No output subscription — the terminal owns its window.

use crate::icons::{distro_icon, is_running, status_label};
use crate::message::{ContainerMsg, DetailsMsg, Message, TerminalMsg};
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::backends::Terminal;
use gosh_distrobox_core::models::ContainerInfo;

/// Terminal page state. Lives on `App` (row #7).
#[derive(Clone, Debug, Default)]
pub struct TerminalState {
    /// Container the page was opened for.
    pub container: Option<String>,
    /// Terminal picker selection (full_command_id).
    pub terminal_id: Option<String>,
    /// Enter-command display (loaded async, row #69).
    pub enter_argv: Option<Vec<String>>,
    pub loading_command: bool,
    pub command_error: Option<String>,
}

impl TerminalState {
    /// Resolve the selected terminal (or the first available).
    pub fn selected<'a>(&self, terminals: &'a [Terminal]) -> Option<&'a Terminal> {
        match &self.terminal_id {
            Some(id) => terminals.iter().find(|t| &t.full_command_id() == id),
            None => terminals.first(),
        }
    }
}

/// Info header (row #67).
pub fn info_header(container: &ContainerInfo) -> cosmic::Element<'static, Message> {
    widget::Row::new()
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
                .push(widget::text::caption(status_label(&container.status)))
                .push(widget::text::monotext(container.image.clone()))
                .spacing(2)
                .width(Length::Fill),
        )
        .spacing(12)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
}

/// Full terminal page body.
pub fn view_terminal(
    container: &ContainerInfo,
    state: &TerminalState,
    terminals: &[Terminal],
    upgrading: bool,
) -> cosmic::Element<'static, Message> {
    let running = is_running(&container.status);
    let mut col = widget::Column::new().spacing(12);
    col = col.push(info_header(container));

    // Command section (rows #69–#72).
    col = col.push(widget::text::caption_heading("TERMINAL ACCESS"));
    if !running {
        // Row #68: banner WITH Start CTA (B5 — dead in Flutter).
        col = col.push(widget::warning(
            "Container is not running. Start the container to access the terminal.",
        ));
        col = col.push(
            widget::button::standard("Start").on_press(Message::Containers(
                ContainerMsg::StartRequested(container.name.clone()),
            )),
        );
    } else if state.loading_command {
        col = col.push(widget::text::body("Loading terminal command…"));
    } else if let Some(err) = &state.command_error {
        col = col.push(widget::warning(format!("Error: {err}")));
        col = col.push(
            widget::button::standard("Retry").on_press(Message::Terminal(
                TerminalMsg::CommandReloadRequested(container.name.clone()),
            )),
        );
    } else if let Some(argv) = &state.enter_argv {
        // Rows #69–#71: selectable monospace command + icon button + filled
        // Copy Command + lead-in (Flutter had all three).
        let cmd = argv.join(" ");
        col = col.push(widget::text::body(
            "Run this command in your terminal to enter the container:",
        ));
        col = col.push(widget::text::monotext(cmd.clone()));
        col = col.push({
            let copy: cosmic::Element<'static, Message> = widget::Row::new()
                .push(
                    widget::button::icon(widget::icon::from_name("edit-copy-symbolic").handle())
                        .on_press(Message::Terminal(TerminalMsg::CopyRequested(cmd.clone()))),
                )
                .push(
                    widget::button::suggested("Copy Command")
                        .on_press(Message::Terminal(TerminalMsg::CopyRequested(cmd))),
                )
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .into();
            copy
        });
    }

    // Terminal picker + Launch (D8).
    if running {
        col = col.push(widget::text::caption_heading("TERMINAL"));
        let names: Vec<String> = terminals.iter().map(|t| t.name.clone()).collect();
        let selected_idx = state
            .terminal_id
            .as_deref()
            .and_then(|id| terminals.iter().position(|t| t.full_command_id() == id))
            .or(if names.is_empty() { None } else { Some(0) });
        col = col.push({
            let picker: cosmic::Element<'static, Message> =
                widget::dropdown(names, selected_idx, |i| {
                    Message::Terminal(TerminalMsg::TerminalSelected(i))
                })
                .into();
            picker
        });
        col = col.push(
            widget::button::suggested("Launch Terminal").on_press(Message::Terminal(
                TerminalMsg::LaunchRequested(container.name.clone()),
            )),
        );
    }

    // Quick actions (rows #73–#74).
    let upgrade_label = if upgrading {
        "Upgrading…"
    } else {
        "Upgrade Packages"
    };
    col = col.push(widget::button::standard(upgrade_label).on_press_maybe(
        if running && !upgrading {
            Some(Message::Details(DetailsMsg::UpgradeRequested(
                container.name.clone(),
            )))
        } else {
            None
        },
    ));
    if running {
        col = col.push(
            widget::button::standard("Stop Container").on_press(Message::Containers(
                ContainerMsg::StopRequested(container.name.clone()),
            )),
        );
    }

    // Details rows (#75).
    col = col.push(widget::text::caption_heading("CONTAINER DETAILS"));
    for (k, v) in [
        ("ID", container.id.clone()),
        ("Name", container.name.clone()),
        ("Image", container.image.clone()),
        ("Status", status_label(&container.status)),
    ] {
        col = col.push(widget::text::caption(format!("{k}: {v}")));
    }

    // Honest help text (#76 — command display, not emulation).
    col = col.push(widget::text::caption(
        "Gosh Distrobox Manager shows the command to enter containers and launches your terminal. Full terminal emulation is not included.",
    ));

    widget::scrollable(col).into()
}
