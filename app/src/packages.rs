//! Package manager state + views (T8, ux.md §6.9, rows #106–#122).
//!
//! Container picker (dropdown w/ running dots, #109) + status pill (#110).
//! Not-running banner (#111 — warning + note; the start CTA needs §4.1's
//! missing `start` op, so no dead Start button).
//!
//! Search bar Enter-triggered + disabled when stopped (#112) + clear
//! (#113). PM badge (#114, typed via B1 — never a bare string).
//!
//! Install-from-box disabled when empty (#115). Upgrade All + confirm
//! (#116). Installed/search tabs (#107) with not-running / loading /
//! error+Retry / empty / count states (#117–#118). Package rows with
//! install/remove + confirms (#119–#120). Task progress via the shared T5
//! mirror (#121). Every failure through the toaster (#122 — the Flutter
//! page showed NOTHING on null taskId).
//!
//! `Unknown` PM (B1): manual command entry instead of an error toast — the
//! page offers a Run-command box (T9 owns real execution; T8 records the
//! intent as a toast pointing there).

use crate::icons::is_running;
use crate::message::{Message, PackagesMsg};
use crate::views::empty_state;
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::models::{PackageInfo, PackageManager};

/// Package page state. Lives on `App` (tab switches preserve it — row #7).
#[derive(Clone, Debug, Default)]
pub struct PackagesState {
    /// Selected container name (mirrors `selected_container`, but the page
    /// keeps its own so leaving and returning preserves context).
    pub container: Option<String>,
    /// Installed / Search tab.
    pub search_tab: bool,
    pub query: String,
    pub searching: bool,
    pub installed: Vec<PackageInfo>,
    pub results: Vec<PackageInfo>,
    pub manager: Option<PackageManager>,
    pub loading: bool,
    pub error: Option<String>,
    /// Manual command for `Unknown` PM containers.
    pub manual_cmd: String,
}

impl PackagesState {
    /// Package-manager badge text (B1 typed — `#114`; Flutter showed the
    /// raw string or "Detecting…").
    pub fn badge(manager: Option<PackageManager>) -> String {
        match manager {
            Some(pm) => pm.badge().to_string(),
            None => "Detecting…".to_string(),
        }
    }
}

/// Container picker (row #109): real `widget::dropdown` with running dots,
/// plus the status pill (#110).
pub fn container_picker(
    containers: &[gosh_distrobox_core::models::ContainerInfo],
    selected: Option<&str>,
) -> cosmic::Element<'static, Message> {
    container_picker_mapped(containers, selected, |i| {
        Message::Packages(PackagesMsg::ContainerSelected(i))
    })
}

/// Same picker with a caller-provided selection message (Backups reuses the
/// widget with its own namespace — row #136).
pub fn container_picker_mapped(
    containers: &[gosh_distrobox_core::models::ContainerInfo],
    selected: Option<&str>,
    on_select: impl Fn(usize) -> Message + Send + Sync + 'static,
) -> cosmic::Element<'static, Message> {
    let names: Vec<String> = containers.iter().map(|c| c.name.clone()).collect();
    let selected_idx = selected.and_then(|s| names.iter().position(|n| n == s));
    let picker: cosmic::Element<'static, Message> =
        widget::dropdown(names, selected_idx, on_select).into();
    let pill = match selected
        .and_then(|s| containers.iter().find(|c| c.name == s))
        .map(|c| is_running(&c.status))
    {
        Some(true) => widget::text::caption("Running"),
        _ => widget::text::caption("Stopped"),
    };
    widget::Column::new()
        .push(widget::text::caption_heading("CONTAINER"))
        .push(picker)
        .push(pill)
        .spacing(4)
        .into()
}

/// Installed/search tab bar (row #107): two buttons, search labelled
/// dynamically ("Search" vs "Search Results").
pub fn tab_bar(searching: bool) -> cosmic::Element<'static, Message> {
    widget::Row::new()
        .push(
            widget::button::standard("Installed")
                .on_press(Message::Packages(PackagesMsg::TabSelected(false))),
        )
        .push(
            widget::button::standard(if searching {
                "Search Results"
            } else {
                "Search"
            })
            .on_press(Message::Packages(PackagesMsg::TabSelected(true))),
        )
        .spacing(12)
        .into()
}

/// Package row (row #119): name, version chip, description, Install/Remove
/// action with confirm upstream (row #120 owns the dialog).
pub fn package_row(pkg: &PackageInfo, action: Message) -> cosmic::Element<'static, Message> {
    let label = if pkg.installed { "Remove" } else { "Install" };
    widget::Row::new()
        .push(
            widget::Column::new()
                .push(widget::text::body(pkg.name.clone()))
                .push(widget::text::caption(format!(
                    "{} — {}",
                    pkg.version, pkg.description
                )))
                .width(Length::Fill)
                .spacing(2),
        )
        .push(widget::button::text(label).on_press(action))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
        .into()
}

/// Not-running banner (row #111): warning + copy (NO dead Start button —
/// §4.1's `start` op does not exist; a button that cannot work is worse).
pub fn not_running_banner() -> cosmic::Element<'static, Message> {
    widget::warning("Container is not running. Start it to manage packages.").into()
}

/// Manual-command box for `Unknown` PM (B1): no error toast — the page
/// offers entry instead.
pub fn manual_cmd_box(cmd: &str) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::body(
            "No supported package manager detected. Run a command manually:",
        ))
        .push({
            let input: cosmic::Element<'static, Message> =
                widget::text_input::text_input("e.g. apt-get install foo", cmd.to_string())
                    .on_input(|s| Message::Packages(PackagesMsg::ManualCmdChanged(s)))
                    .into();
            input
        })
        .push(
            widget::button::standard("Run (lands in T9)")
                .on_press(Message::Packages(PackagesMsg::ManualRunRequested)),
        )
        .spacing(8)
        .into()
}

/// Installed list states (row #117).
pub fn installed_list(
    running: bool,
    loading: bool,
    error: Option<&str>,
    packages: &[PackageInfo],
    container: &str,
) -> cosmic::Element<'static, Message> {
    if !running {
        return empty_state(
            "system-shutdown-symbolic",
            "Container Not Running".to_string(),
            "Start the container to view installed packages".to_string(),
            None,
        );
    }
    if loading {
        return widget::container(widget::text::body("Loading packages…"))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into();
    }
    if let Some(err) = error {
        return empty_state(
            "dialog-error-symbolic",
            "Could not load packages".to_string(),
            err.to_string(),
            Some(
                widget::button::standard("Retry")
                    .on_press(Message::Packages(PackagesMsg::ReloadRequested(
                        container.to_string(),
                    )))
                    .into(),
            ),
        );
    }
    if packages.is_empty() {
        return empty_state(
            "document-open-symbolic",
            "No packages found".to_string(),
            String::new(),
            None,
        );
    }
    let mut col = widget::Column::new()
        .push(widget::text::caption(format!(
            "{} packages installed",
            packages.len()
        )))
        .spacing(8);
    for pkg in packages {
        let name = pkg.name.clone();
        col = col.push(package_row(
            pkg,
            Message::Packages(PackagesMsg::RemoveRequested(name)),
        ));
    }
    widget::scrollable(col).into()
}

/// Search list states (row #118).
pub fn search_list(
    searching: bool,
    loading: bool,
    results: &[PackageInfo],
) -> cosmic::Element<'static, Message> {
    if !searching {
        return empty_state(
            "system-search-symbolic",
            "Search for packages".to_string(),
            "Enter a package name and press Enter".to_string(),
            None,
        );
    }
    if loading {
        return widget::container(widget::text::body("Searching…"))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into();
    }
    if results.is_empty() {
        return empty_state(
            "system-search-symbolic",
            "No packages found".to_string(),
            "Try a different search term".to_string(),
            None,
        );
    }
    let mut col = widget::Column::new()
        .push(widget::text::caption(format!(
            "{} packages found",
            results.len()
        )))
        .spacing(8);
    for pkg in results {
        let name = pkg.name.clone();
        col = col.push(package_row(
            pkg,
            Message::Packages(PackagesMsg::InstallRequested(name)),
        ));
    }
    widget::scrollable(col).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_is_typed_never_bare() {
        assert_eq!(PackagesState::badge(None), "Detecting…");
        assert_eq!(PackagesState::badge(Some(PackageManager::Emerge)), "EMERGE");
        assert_eq!(
            PackagesState::badge(Some(PackageManager::Unknown)),
            "UNKNOWN"
        );
    }
}
