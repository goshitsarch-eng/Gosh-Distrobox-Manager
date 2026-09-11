//! Backups page (T10, ux.md §6.11, rows #133–#151).
//!
//! Header Refresh + New Snapshot (#133–#134, `header_end` — the FAB's
//! all-states bug disappears with the move), gates (#135, shared),
//! container picker (#136, shared dropdown WITH status dots — the Flutter
//! page lacked them), snapshots tab states (#137–#138), snapshot rows with
//! Restore/Delete (#139), create dialog with LOUD empty-name error (#140 —
//! the silent return is fixed), delete confirm + result toasts (#141),
//! restore dialog with prefilled name (#142), export/import cards + dialogs
//! (#143–#144) with PORTAL choosers (P0 §4.3 — free-text paths replaced),
//! clone card + dialog unified with details (#145), all five input dialogs
//! LOUD on empty fields (#146), every null-taskId failure toasts (#147),
//! clone `.trim()` consistency (#148), portal pickers not free text (#149),
//! shared task progress (#150 — mirror rows, no sixth dialog copy).

use crate::message::{BackupsMsg, Message};
use crate::views::empty_state;
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::models::SnapshotInfo;

/// Backups page state. Lives on `App` (row #7).
#[derive(Clone, Debug, Default)]
pub struct BackupsState {
    /// Selected container for snapshot/export/clone scoping.
    pub container: Option<String>,
    /// Snapshots vs Export/Import tab (false = snapshots).
    pub transfer_tab: bool,
    pub snapshots: Vec<SnapshotInfo>,
    pub loading: bool,
    pub error: Option<String>,
    /// Create-snapshot dialog name field.
    pub create_name: String,
    pub create_error: Option<String>,
    /// Restore dialog: (snapshot name, new container name).
    pub restore: Option<(String, String)>,
    pub restore_error: Option<String>,
    /// Export dialog output path.
    pub export_path: String,
    pub export_error: Option<String>,
    /// Import dialog archive path + image name.
    pub import_path: String,
    pub import_image: String,
    pub import_error: Option<String>,
    /// Which dialog is open (single modal slot owns rendering).
    pub dialog: Option<BackupsDialog>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupsDialog {
    Create,
    Restore(String),
    Export,
    Import,
}

impl BackupsState {
    /// Default snapshot name (`<prefix>-<container>`, Flutter's shape with
    /// the configured prefix in place of Flutter's hard-coded `gdm`).
    /// `prefix` comes from the Settings preference (#171); an empty prefix
    /// degrades to Flutter's `{container}-snapshot` rather than producing a
    /// leading-dash name. No timestamp — deterministic and valid; Flutter
    /// used millis, which hurt tests.
    pub fn default_snapshot_name(prefix: &str, container: &str) -> String {
        if prefix.trim().is_empty() {
            format!("{container}-snapshot")
        } else {
            format!("{}-{container}", prefix.trim())
        }
    }

    /// Default export path. A configured `dir` (#171) is honoured; empty
    /// falls back to Flutter's `/tmp/<name>-export.tar`.
    ///
    /// `~` is expanded here because the path becomes an argv element for
    /// `podman export -o <path>` — never a shell word — so a literal
    /// `~/Downloads/...` from the Settings hint would reach podman
    /// unexpanded and fail. A leading `~user` form is left alone: resolving
    /// another user's home is not this app's business.
    pub fn default_export_path(dir: &str, container: &str) -> String {
        let dir = dir.trim();
        if dir.is_empty() {
            return format!("/tmp/{container}-export.tar");
        }
        let dir = if dir == "~" {
            home().unwrap_or_else(|| dir.to_string())
        } else if let Some(rest) = dir.strip_prefix("~/") {
            match home() {
                Some(h) => format!("{}/{rest}", h.trim_end_matches('/')),
                None => dir.to_string(),
            }
        } else {
            dir.to_string()
        };
        format!("{}/{container}-export.tar", dir.trim_end_matches('/'))
    }
}

/// `$HOME` when set and non-empty (the sandbox sets it; a missing one just
/// leaves a `~` path verbatim, which the picker then shows and the user can
/// correct).
fn home() -> Option<String> {
    std::env::var("HOME").ok().filter(|h| !h.is_empty())
}

/// Snapshot row (#139): name, created, size, Restore, Delete.
pub fn snapshot_row(snapshot: &SnapshotInfo) -> cosmic::Element<'static, Message> {
    widget::Row::new()
        .push(
            widget::Column::new()
                .push(widget::text::body(snapshot.name.clone()))
                .push(widget::text::caption(format!(
                    "{} · {}",
                    snapshot.created, snapshot.size
                )))
                .width(Length::Fill)
                .spacing(2),
        )
        .push(
            widget::button::standard("Restore").on_press(Message::Backups(
                BackupsMsg::RestoreDialogRequested(snapshot.name.clone()),
            )),
        )
        .push(widget::button::text("Delete").on_press(Message::Backups(
            BackupsMsg::DeleteRequested(snapshot.id.clone()),
        )))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
}

/// Snapshots tab body (#137–#139).
pub fn snapshots_tab(
    loading: bool,
    error: Option<&str>,
    snapshots: &[SnapshotInfo],
    container: Option<&str>,
) -> cosmic::Element<'static, Message> {
    if loading {
        return widget::container(widget::text::body("Loading snapshots…"))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into();
    }
    if let Some(err) = error {
        return empty_state(
            "dialog-error-symbolic",
            "Could not load snapshots".to_string(),
            err.to_string(),
            Some(
                widget::button::standard("Retry")
                    .on_press(Message::Backups(BackupsMsg::ReloadRequested))
                    .into(),
            ),
        );
    }
    if snapshots.is_empty() {
        return empty_state(
            "document-open-symbolic",
            "No snapshots yet".to_string(),
            "Create your first snapshot to get started.".to_string(),
            container.map(|_| {
                let action: cosmic::Element<'static, Message> =
                    widget::button::suggested("Create First Snapshot")
                        .on_press(Message::Backups(BackupsMsg::CreateDialogRequested))
                        .into();
                action
            }),
        );
    }
    let mut col = widget::Column::new()
        .push(widget::text::caption(format!(
            "{} snapshot{}",
            snapshots.len(),
            if snapshots.len() != 1 { "s" } else { "" }
        )))
        .spacing(8);
    for snap in snapshots {
        col = col.push(snapshot_row(snap));
    }
    widget::scrollable(col).into()
}

/// Transfer tab body: export/import/clone cards (#143–#145). `container`
/// is intentionally unused — card actions gate on `has_container` and the
/// name renders in the export dialog, not the tab.
pub fn transfer_tab(
    _container: Option<&str>,
    has_container: bool,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    // Export card (#143).
    col = col.push(widget::text::caption_heading("EXPORT CONTAINER"));
    col = col.push(widget::text::caption(
        "Save a container to a tar archive (portal file chooser).",
    ));
    col = col.push(
        widget::button::standard("Export…").on_press_maybe(if has_container {
            Some(Message::Backups(BackupsMsg::ExportDialogRequested))
        } else {
            None
        }),
    );
    // Import card (#144).
    col = col.push(widget::text::caption_heading("IMPORT CONTAINER"));
    col = col.push(widget::text::caption(
        "Restore a container from a tar archive (portal file chooser).",
    ));
    col = col.push(
        widget::button::standard("Import…")
            .on_press(Message::Backups(BackupsMsg::ImportDialogRequested)),
    );
    // Clone card (#145 — disabled without a container, unified dialog).
    col = col.push(widget::text::caption_heading("CLONE CONTAINER"));
    col = col.push(widget::text::caption(
        "Create a copy of the selected container.",
    ));
    col = col.push(
        widget::button::standard("Clone…").on_press_maybe(if has_container {
            Some(Message::Backups(BackupsMsg::CloneDialogRequested))
        } else {
            None
        }),
    );
    widget::scrollable(col).into()
}

// `container` selects which card actions enable (via `has_container`); the
// name itself renders in the export dialog, not the tab.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_are_deterministic() {
        // Empty preferences keep Flutter's exact defaults.
        assert_eq!(
            BackupsState::default_snapshot_name("", "mybox"),
            "mybox-snapshot"
        );
        assert_eq!(
            BackupsState::default_export_path("", "mybox"),
            "/tmp/mybox-export.tar"
        );
    }

    /// #171: the Settings preferences must actually reach the dialogs — a
    /// prefix/dir that is stored and displayed but never applied is the
    /// dead-state the config module claims not to have.
    #[test]
    fn preferences_prefill_the_dialogs() {
        assert_eq!(
            BackupsState::default_snapshot_name("snap", "mybox"),
            "snap-mybox"
        );
        assert_eq!(
            BackupsState::default_snapshot_name("  ", "mybox"),
            "mybox-snapshot",
            "blank prefix must not make a leading-dash name"
        );
        assert_eq!(
            BackupsState::default_export_path("/home/me/dumps", "mybox"),
            "/home/me/dumps/mybox-export.tar"
        );
        assert_eq!(
            BackupsState::default_export_path("/home/me/dumps/", "mybox"),
            "/home/me/dumps/mybox-export.tar",
            "trailing slash must not double up"
        );
        // The Settings hint is `~/Downloads`, and the path is handed to
        // `podman export -o` as an argv element (no shell), so `~` must be
        // expanded here or the prefill is guaranteed to fail.
        assert_eq!(
            BackupsState::default_export_path("~/Downloads", "mybox"),
            format!("{}/Downloads/mybox-export.tar", home().expect("HOME set")),
        );
        assert_eq!(
            BackupsState::default_export_path("~", "mybox"),
            format!("{}/mybox-export.tar", home().expect("HOME set")),
        );
        assert!(
            !BackupsState::default_export_path("~other/dumps", "mybox").contains("~")
                || BackupsState::default_export_path("~other/dumps", "mybox")
                    .starts_with("~other/"),
            "another user's home is left to the shell-free caller to reject"
        );
    }
}
