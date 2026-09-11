//! Settings page (T12, ux.md §6.13, rows #163–#171).
//!
//! System info (distrobox version + refresh #163, totals #164), Refresh
//! All Data + toast (#165), Stop All + confirm (#166), Upgrade All for
//! real (#167 — dead snackbar redirect in Flutter, same path as dashboard),
//! Clear Completed + toast (#168), About via `widget::about()` (#169 —
//! replaces the whole card; feature `about`), Danger Zone Delete All +
//! confirm (#170), and persisted preferences (#171): selected terminal
//! picker, confirm-destructive toggle, snapshot prefix, export dir — all
//! backed by cosmic-config (§5, degrade-don't-crash).

use crate::message::{Message, SettingsMsg};
use crate::views::empty_state;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::widget;
use gosh_distrobox_core::{AppConfig, CONFIG_ID, CONFIG_VERSION};

/// Config identity for the `watch_config` subscription (same string as
/// `APP_ID`, §5.2).
pub fn config_id() -> &'static str {
    CONFIG_ID
}

/// cosmic-config entry (T12 §5.2–5.3, D11): the `CosmicConfigEntry` derive
/// writes each field to its own top-level key (`…/cosmic/<id>/v1/<field>`,
/// snake_case field name = key, Q6 closed), so a corrupt key cannot take the
/// rest down. Lives in `app/` — core takes no libcosmic dep, so the T2
/// git-pointer/sidecar story stays at five. `From` impls translate to/from
/// the plain core struct; only keys with readers exist (no dead state).
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry,
)]
#[version = 1]
pub struct PrefsEntry {
    pub selected_terminal: String,
    pub confirm_destructive_actions: bool,
    pub snapshot_prefix: String,
    pub default_export_dir: String,
    pub custom_terminals: Vec<gosh_distrobox_core::backends::Terminal>,
}

/// Manual, and it MUST mirror `AppConfig::default()` — never a derived
/// `Default`. `CosmicConfigEntry::get_entry` starts from `Self::default()`
/// and only fills keys that exist, so on a fresh install (no keys on disk)
/// the watcher's first emission IS this value. A derived default would send
/// `confirm_destructive_actions: false` and an empty terminal id, silently
/// turning destructive confirms off for a user who never asked. One source
/// of truth: the core struct's own default.
impl Default for PrefsEntry {
    fn default() -> Self {
        Self::from(&AppConfig::default())
    }
}

impl From<&AppConfig> for PrefsEntry {
    fn from(cfg: &AppConfig) -> Self {
        Self {
            selected_terminal: cfg.selected_terminal.clone(),
            confirm_destructive_actions: cfg.confirm_destructive_actions,
            snapshot_prefix: cfg.snapshot_prefix.clone(),
            default_export_dir: cfg.default_export_dir.clone(),
            custom_terminals: cfg.custom_terminals.clone(),
        }
    }
}

impl From<&PrefsEntry> for AppConfig {
    fn from(entry: &PrefsEntry) -> Self {
        Self {
            selected_terminal: entry.selected_terminal.clone(),
            confirm_destructive_actions: entry.confirm_destructive_actions,
            snapshot_prefix: entry.snapshot_prefix.clone(),
            default_export_dir: entry.default_export_dir.clone(),
            custom_terminals: entry.custom_terminals.clone(),
        }
    }
}

/// Read the entry with per-field documented defaults (degrade-don't-crash):
/// a missing key takes the `AppConfig::default()` value, a corrupt key logs
/// and does the same — neither takes the rest down. `None` = the config dir
/// itself is unavailable. Returns the config plus whether the legacy-import
/// keys are absent (D11 one-time import gate: only `NotFound` counts, never
/// a corrupt key — a corrupt key is not the user's, but neither is it a
/// license to overwrite).
pub fn load_entry() -> Option<(AppConfig, bool)> {
    use cosmic::cosmic_config::{Config, ConfigGet, Error};
    let config = Config::new(CONFIG_ID, CONFIG_VERSION).ok()?;
    let defaults = AppConfig::default();
    // Presence probe with the real value types (no extra deps): only
    // `NotFound` counts as absent — a corrupt key keeps its fallback and is
    // never overwritten by the import.
    let needs_import = matches!(
        config.get::<String>("selected_terminal"),
        Err(Error::NotFound)
    ) || matches!(
        config.get::<Vec<gosh_distrobox_core::backends::Terminal>>("custom_terminals"),
        Err(Error::NotFound)
    );
    let get_string = |key: &str, fallback: &str| {
        config.get::<String>(key).unwrap_or_else(|e| {
            if e.is_err() {
                tracing::warn!(target: "gosh_config", "config key {key} unreadable, using default: {e}");
            }
            fallback.to_string()
        })
    };
    let cfg = AppConfig {
        selected_terminal: get_string("selected_terminal", &defaults.selected_terminal),
        confirm_destructive_actions: config
            .get::<bool>("confirm_destructive_actions")
            .unwrap_or_else(|e| {
                if e.is_err() {
                    tracing::warn!(target: "gosh_config", "config key confirm_destructive_actions unreadable, using default: {e}");
                }
                defaults.confirm_destructive_actions
            }),
        snapshot_prefix: get_string("snapshot_prefix", &defaults.snapshot_prefix),
        default_export_dir: get_string("default_export_dir", &defaults.default_export_dir),
        custom_terminals: config
            .get::<Vec<gosh_distrobox_core::backends::Terminal>>("custom_terminals")
            .unwrap_or_else(|e| {
                if e.is_err() {
                    tracing::warn!(target: "gosh_config", "config key custom_terminals unreadable, using default: {e}");
                }
                Vec::new()
            }),
    };
    Some((cfg, needs_import))
}

/// Best-effort write of the whole entry (one file per key — atomic per key,
/// §5.1). Returns the first error as a string for the toast path.
pub fn save_entry(entry: &PrefsEntry) -> Result<(), String> {
    use cosmic::cosmic_config::{Config, CosmicConfigEntry};
    let config = Config::new(CONFIG_ID, CONFIG_VERSION).map_err(|e| e.to_string())?;
    entry.write_entry(&config).map_err(|e| e.to_string())
}

/// System info section (rows #163–#164).
pub fn system_info(
    version: &str,
    loading_version: bool,
    total: usize,
    running: usize,
    installed: bool,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::text::caption_heading("SYSTEM INFORMATION"))
        .spacing(4);
    col = col.push({
        let row: cosmic::Element<'static, Message> = widget::Row::new()
            .push(widget::text::body("Distrobox Version").width(cosmic::iced::Length::Fill))
            .push(widget::text::body(if loading_version {
                "Loading…".to_string()
            } else {
                version.to_string()
            }))
            .push(
                widget::button::standard("Refresh")
                    .on_press(Message::Settings(SettingsMsg::VersionReloadRequested)),
            )
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        row
    });
    for (label, value) in [
        ("Total Containers", total.to_string()),
        ("Running Containers", running.to_string()),
        (
            "Distrobox Installed",
            if installed { "Yes" } else { "No" }.to_string(),
        ),
    ] {
        col = col.push({
            let row: cosmic::Element<'static, Message> = widget::Row::new()
                .push(widget::text::body(label).width(cosmic::iced::Length::Fill))
                .push(widget::text::body(value))
                .spacing(8)
                .into();
            row
        });
    }
    col.into()
}

/// Preferences section (row #171): terminal picker, confirm toggle,
/// snapshot prefix, export dir — every control writes through immediately
/// (best-effort; failures toast, never crash).
pub fn preferences(
    config: &AppConfig,
    terminals: &[gosh_distrobox_core::backends::Terminal],
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::text::caption_heading("PREFERENCES"))
        .spacing(4);
    // Selected terminal picker.
    let names: Vec<String> = terminals.iter().map(|t| t.name.clone()).collect();
    let selected_idx = terminals.iter().position(|t| {
        t.full_command_id() == config.selected_terminal || t.program == config.selected_terminal
    });
    col = col.push({
        let picker: cosmic::Element<'static, Message> =
            widget::dropdown(names, selected_idx, |i| {
                Message::Settings(SettingsMsg::TerminalSelected(i))
            })
            .into();
        picker
    });
    // Confirm toggle.
    col = col.push({
        let mut toggles = widget::list_column::list_column();
        toggles = toggles.add(
            widget::settings::item::builder("Confirm destructive actions")
                .description("Ask before remove, stop-all, delete")
                .toggler(config.confirm_destructive_actions, |v| {
                    Message::Settings(SettingsMsg::ConfirmToggled(v))
                }),
        );
        let toggles_el: cosmic::Element<'static, Message> = toggles.into_element();
        toggles_el
    });
    // Snapshot prefix.
    col = col.push(widget::text::body("Snapshot Prefix"));
    col = col.push({
        let input: cosmic::Element<'static, Message> =
            widget::text_input::text_input("gdm", config.snapshot_prefix.clone())
                .on_input(|s| Message::Settings(SettingsMsg::SnapshotPrefixChanged(s)))
                .into();
        input
    });
    // Export dir.
    col = col.push(widget::text::body("Default Export Directory"));
    col = col.push({
        let input: cosmic::Element<'static, Message> =
            widget::text_input::text_input("~/Downloads", config.default_export_dir.clone())
                .on_input(|s| Message::Settings(SettingsMsg::ExportDirChanged(s)))
                .into();
        input
    });
    col.into()
}

/// About section (row #169, I5): `widget::about()` replaces the whole
/// hand-rolled card. Links route through `SettingsMsg::OpenUrl` (failure
/// toasts, success silent — Flutter parity). The widget borrows its `About`,
/// so a process-lifetime singleton backs the `'static` element (all fields
/// are build constants).
pub fn about() -> cosmic::Element<'static, Message> {
    use std::sync::OnceLock;
    static INFO: OnceLock<widget::about::About> = OnceLock::new();
    let info = INFO.get_or_init(|| {
        widget::about::About::default()
            .name("Gosh Distrobox Manager")
            .icon(widget::icon::from_name("io.github.gosh_distrobox_manager").handle())
            .version(format!("v{}", env!("CARGO_PKG_VERSION")))
            .comments("A GUI for managing Distrobox containers.")
            .copyright("GPL-3.0-or-later")
            .license("GPL-3.0-or-later")
            .links([
                (
                    "Source Code",
                    "https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager",
                ),
                ("Distrobox Docs", "https://distrobox.it"),
            ])
    });
    widget::about(info, |url| {
        Message::Settings(SettingsMsg::OpenUrl(url.to_string()))
    })
}

/// Danger zone (row #170): Delete All + confirm with warning box.
pub fn danger_zone(has_containers: bool) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::caption_heading("DANGER ZONE"))
        .push(
            widget::button::destructive("Delete All Containers").on_press_maybe(
                if has_containers {
                    Some(Message::Settings(SettingsMsg::DeleteAllRequested))
                } else {
                    None
                },
            ),
        )
        .spacing(4)
        .into()
}

/// Empty config-notice (row #171 degrade path): shown when the config dir
/// is unavailable — preferences still render (defaults), writes toast.
pub fn config_unavailable() -> cosmic::Element<'static, Message> {
    empty_state(
        "dialog-warning-symbolic",
        "Preferences unavailable".to_string(),
        "Settings will not persist this session.".to_string(),
        None,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn entry_round_trips_core_config() {
        let cfg = gosh_distrobox_core::AppConfig::default();
        let entry = super::PrefsEntry::from(&cfg);
        let back = gosh_distrobox_core::AppConfig::from(&entry);
        assert_eq!(cfg, back);
    }

    #[test]
    fn entry_version_matches_core() {
        use cosmic::cosmic_config::CosmicConfigEntry;
        assert_eq!(
            super::PrefsEntry::VERSION,
            gosh_distrobox_core::CONFIG_VERSION
        );
    }

    /// The load-bearing invariant of the whole watch path: `get_entry`
    /// substitutes `Self::default()` for every absent key, so an entry
    /// default that drifts from `AppConfig::default()` would hand the UI
    /// settings nobody chose (the fresh-install case: no keys on disk).
    #[test]
    fn entry_default_matches_core_default() {
        assert_eq!(
            super::PrefsEntry::default(),
            super::PrefsEntry::from(&gosh_distrobox_core::AppConfig::default())
        );
        assert!(super::PrefsEntry::default().confirm_destructive_actions);
    }
}
