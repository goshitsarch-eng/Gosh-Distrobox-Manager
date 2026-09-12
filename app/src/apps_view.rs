//! Apps export page (T12, ux.md §6.14, rows #172–#179).
//!
//! Header title + container subtitle + refresh (#172), loading /
//! error+Retry (#173), search over name + exec (#174), Manual Binary Export
//! button (#175), section + count badge (#176), empty states (#177), app
//! cards in a 2-col grid with export toggles + EXPORTED label (#178),
//! export-binary dialog (#179 — path field, LOUD on empty).

use crate::fl;
use crate::message::{AppMsg, Message};
use crate::views::empty_state;
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::models::AppInfo;

/// Apps page state. Lives on `App` (row #7): search only — the binary
/// dialog fields live on `App` alongside the other dialog states.
#[derive(Clone, Debug, Default)]
pub struct AppsViewState {
    pub search: String,
}

impl AppsViewState {
    /// Filter over name + exec (Flutter `_filterApps`).
    pub fn filter<'a>(apps: &'a [AppInfo], query: &str) -> Vec<&'a AppInfo> {
        if query.is_empty() {
            return apps.iter().collect();
        }
        let q = query.to_lowercase();
        apps.iter()
            .filter(|a| a.name.to_lowercase().contains(&q) || a.exec.to_lowercase().contains(&q))
            .collect()
    }
}

/// App card (#178): icon, name, exec, export toggle, EXPORTED label.
pub fn app_card(app: &AppInfo, container: &str) -> cosmic::Element<'static, Message> {
    let desktop = app.desktop_file_path.clone();
    let container = container.to_string();
    let mut col = widget::Column::new().spacing(4);
    col = col.push({
        let head: cosmic::Element<'static, Message> = widget::Row::new()
            // Row #178: icon from the desktop file's `Icon=` (`AppInfo.icon`,
            // populated by `Backend::container_apps`). `from_name` resolves
            // the themed name and yields a blank handle for a missing icon,
            // so a stale entry degrades to a gap, not a broken image.
            .push({
                let icon: cosmic::Element<'static, Message> =
                    widget::icon::from_name(app.icon.clone())
                        .size(24)
                        .icon()
                        .into();
                icon
            })
            .push(widget::text::body(app.name.clone()).width(Length::Fill))
            .push({
                let mut toggles = widget::list_column::list_column();
                toggles = toggles.add({
                    let desktop2 = desktop.clone();
                    let container2 = container.clone();
                    let exported = app.is_exported;
                    widget::settings::item::builder("").toggler(exported, move |v| {
                        Message::Apps(if v {
                            AppMsg::ExportRequested(container2.clone(), desktop2.clone())
                        } else {
                            AppMsg::UnexportRequested(container2.clone(), desktop2.clone())
                        })
                    })
                });
                let toggles_el: cosmic::Element<'static, Message> = toggles.into_element();
                toggles_el
            })
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        head
    });
    col = col.push(widget::text::caption(app.exec.clone()));
    col = col.push(widget::text::caption(if app.is_exported {
        fl!("apps-exported")
    } else {
        fl!("apps-not-exported")
    }));
    col.into()
}

/// Full apps page body (container preselected via details tile, T6).
pub fn view_apps_page(
    container: Option<&str>,
    apps: &[AppInfo],
    binaries: &[gosh_distrobox_core::models::ExportedBinary],
    loading: bool,
    error: Option<&str>,
    state: &AppsViewState,
) -> cosmic::Element<'static, Message> {
    let Some(container) = container else {
        return empty_state(
            "document-open-symbolic",
            fl!("apps-empty-select-title"),
            fl!("apps-empty-select-body"),
            None,
        );
    };
    let mut col = widget::Column::new().spacing(12);
    col = col.push(widget::text::title3(fl!(
        "apps-title",
        container = container
    )));
    // Search (#174).
    col = col.push({
        let search: cosmic::Element<'static, Message> =
            widget::text_input::search_input(fl!("apps-search-placeholder"), state.search.clone())
                .id(cosmic::iced::widget::Id::new(crate::views::SEARCH_APPS))
                .on_input(|s| Message::Apps(AppMsg::SearchChanged(s)))
                .into();
        search
    });
    // Manual binary export (#175).
    col = col.push(
        widget::button::suggested(fl!("apps-manual-binary-export"))
            .on_press(Message::Apps(AppMsg::BinaryDialogRequested)),
    );
    if loading {
        col = col.push(widget::text::body(fl!("apps-loading")));
        return widget::scrollable(col).into();
    }
    if let Some(err) = error {
        col = col.push(empty_state(
            "dialog-error-symbolic",
            fl!("apps-error-title"),
            err.to_string(),
            Some(
                widget::button::standard(fl!("action-retry"))
                    .on_press(Message::Apps(AppMsg::ReloadRequested(
                        container.to_string(),
                    )))
                    .into(),
            ),
        ));
        return widget::scrollable(col).into();
    }
    let filtered = AppsViewState::filter(apps, &state.search);
    // Section + badge (#176).
    col = col.push({
        let head: cosmic::Element<'static, Message> = widget::Row::new()
            .push(widget::text::body(fl!("apps-section-installed")).width(Length::Fill))
            .push(widget::text::caption(fl!(
                "apps-count-found",
                count = filtered.len()
            )))
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        head
    });
    if filtered.is_empty() {
        // Empty states (#177).
        col = col.push(empty_state(
            "document-open-symbolic",
            if state.search.is_empty() {
                fl!("apps-empty-no-apps-title")
            } else {
                fl!("apps-empty-no-match-title")
            },
            String::new(),
            None,
        ));
    } else {
        // 2-col grid (#178).
        let mut grid = widget::grid::grid();
        for (i, app) in filtered.iter().enumerate() {
            grid = grid.push(app_card(app, container));
            if i % 2 == 1 {
                grid = grid.insert_row();
            }
        }
        col = col.push({
            let grid_el: cosmic::Element<'static, Message> = grid.into();
            grid_el
        });
    }
    // Exported binaries count (existing T3 mirror surface).
    if !binaries.is_empty() {
        col = col.push(widget::text::caption(fl!(
            "apps-count-binaries",
            count = binaries.len()
        )));
    }
    widget::scrollable(col).into()
}

/// Export-binary dialog body (#179): path field + LOUD error.
pub fn binary_dialog_body(path: &str, error: Option<&str>) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::text::body(fl!("apps-binary-dialog-body")))
        .push({
            let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
                fl!("apps-binary-path-placeholder"),
                path.to_string(),
            )
            .on_input(|s| Message::Apps(AppMsg::BinaryPathChanged(s)))
            .into();
            input
        })
        .spacing(8);
    if let Some(err) = error {
        col = col.push(widget::warning(err.to_string()));
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, exec: &str) -> AppInfo {
        AppInfo {
            name: name.into(),
            exec: exec.into(),
            icon: String::new(),
            desktop_file_path: format!("/{name}.desktop"),
            is_exported: false,
        }
    }

    #[test]
    fn filter_covers_name_and_exec() {
        let apps = vec![app("Vim", "vim"), app("Code", "code --new-window")];
        assert_eq!(AppsViewState::filter(&apps, "").len(), 2);
        assert_eq!(AppsViewState::filter(&apps, "vim").len(), 1);
        assert_eq!(AppsViewState::filter(&apps, "--new").len(), 1);
        assert!(AppsViewState::filter(&apps, "zzz").is_empty());
    }
}
