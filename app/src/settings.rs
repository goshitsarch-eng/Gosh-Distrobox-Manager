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
//!
//! The `show_skipped_lines` toggle (T13/B3) is deliberately NOT one of the
//! rows above: it has no Flutter counterpart to reach parity with, so giving
//! it a row number would break D19's frozen 1–193 sequence. It is recorded as
//! I13 in PLAN.md §4 instead.

use crate::fl;
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
    /// B3 (§6.4): mention the rows a list parser skipped.
    pub show_skipped_lines: bool,
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
            show_skipped_lines: cfg.show_skipped_lines,
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
            show_skipped_lines: entry.show_skipped_lines,
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
        show_skipped_lines: config
            .get::<bool>("show_skipped_lines")
            .unwrap_or_else(|e| {
                if e.is_err() {
                    tracing::warn!(target: "gosh_config", "config key show_skipped_lines unreadable, using default: {e}");
                }
                defaults.show_skipped_lines
            }),
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
        .push(widget::text::caption_heading(fl!(
            "settings-system-information"
        )))
        .spacing(4);
    col = col.push({
        let row: cosmic::Element<'static, Message> = widget::Row::new()
            .push(
                widget::text::body(fl!("settings-distrobox-version"))
                    .width(cosmic::iced::Length::Fill),
            )
            .push(widget::text::body(if loading_version {
                fl!("state-loading")
            } else {
                version.to_string()
            }))
            .push(
                widget::button::standard(fl!("action-refresh"))
                    .on_press(Message::Settings(SettingsMsg::VersionReloadRequested)),
            )
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        row
    });
    for (label, value) in [
        (fl!("settings-total-containers"), total.to_string()),
        (fl!("settings-running-containers"), running.to_string()),
        (
            fl!("settings-distrobox-installed"),
            if installed {
                fl!("settings-yes")
            } else {
                fl!("settings-no")
            }
            .to_string(),
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
        .push(widget::text::caption_heading(fl!("settings-preferences")))
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
    // Confirm toggle + B3 skipped-rows toggle share one settings list.
    col = col.push({
        let mut toggles = widget::list_column::list_column();
        toggles = toggles.add(
            widget::settings::item::builder(fl!("settings-confirm-destructive"))
                .description(fl!("settings-confirm-destructive-description"))
                .toggler(config.confirm_destructive_actions, |v| {
                    Message::Settings(SettingsMsg::ConfirmToggled(v))
                }),
        );
        // B3 (§6.4): the rows distrobox emitted that we could not parse are
        // always collected and logged; this decides whether the Dashboard
        // mentions them.
        toggles = toggles.add(
            widget::settings::item::builder(fl!("settings-show-skipped-rows"))
                .description(fl!("settings-show-skipped-rows-description"))
                .toggler(config.show_skipped_lines, |v| {
                    Message::Settings(SettingsMsg::ShowSkippedLinesToggled(v))
                }),
        );
        let toggles_el: cosmic::Element<'static, Message> = toggles.into_element();
        toggles_el
    });
    // Snapshot prefix.
    col = col.push(widget::text::body(fl!("settings-snapshot-prefix")));
    col = col.push({
        let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
            fl!("settings-snapshot-prefix-placeholder"),
            config.snapshot_prefix.clone(),
        )
        .on_input(|s| Message::Settings(SettingsMsg::SnapshotPrefixChanged(s)))
        .into();
        input
    });
    // Export dir.
    col = col.push(widget::text::body(fl!("settings-default-export-dir")));
    col = col.push({
        let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
            fl!("settings-default-export-dir-placeholder"),
            config.default_export_dir.clone(),
        )
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
            .name(fl!("app-title"))
            .icon(widget::icon::from_name("io.github.gosh_distrobox_manager").handle())
            .version(format!("v{}", env!("CARGO_PKG_VERSION")))
            .comments(fl!("settings-about-comments"))
            .copyright("GPL-3.0-or-later")
            .license("GPL-3.0-or-later")
            .links([
                (
                    fl!("settings-link-source-code"),
                    "https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager",
                ),
                (fl!("settings-link-distrobox-docs"), "https://distrobox.it"),
            ])
    });
    widget::about(info, |url| {
        Message::Settings(SettingsMsg::OpenUrl(url.to_string()))
    })
}

/// Danger zone (row #170): Delete All + confirm with warning box.
pub fn danger_zone(has_containers: bool) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::caption_heading(fl!("settings-danger-zone")))
        .push(
            widget::button::destructive(fl!("settings-delete-all-containers")).on_press_maybe(
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
        fl!("settings-preferences-unavailable"),
        fl!("settings-preferences-unavailable-body"),
        None,
    )
}

#[cfg(test)]
mod tests {
    /// Every field is set *away* from its default. Round-tripping
    /// `AppConfig::default()` passes even when a `From` impl drops a field
    /// entirely — both sides are then the same default, so nothing is
    /// compared. Non-default values make a dropped field a real mismatch.
    #[test]
    fn entry_round_trips_core_config() {
        let cfg = gosh_distrobox_core::AppConfig {
            selected_terminal: "kitty".to_string(),
            confirm_destructive_actions: false,
            snapshot_prefix: "snap".to_string(),
            default_export_dir: "/tmp/exports".to_string(),
            show_skipped_lines: true,
            custom_terminals: vec![gosh_distrobox_core::backends::Terminal {
                name: "WezTerm".to_string(),
                program: "wezterm".to_string(),
                extra_args: vec!["start".to_string()],
                separator_arg: "--".to_string(),
                read_only: true,
            }],
        };
        let entry = super::PrefsEntry::from(&cfg);
        // Spot-check through the entry too, so the assertion is not satisfied
        // by a `From` pair that loses and then re-invents the same value.
        assert!(entry.show_skipped_lines);
        assert_eq!(entry.selected_terminal, "kitty");
        assert_eq!(entry.custom_terminals.len(), 1);
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
    ///
    /// Asserted against LITERALS, not against
    /// `PrefsEntry::from(&AppConfig::default())` — that expression is exactly
    /// what `impl Default for PrefsEntry` *is*, so comparing the two is a
    /// tautology that no edit to either side can fail. These values can.
    /// Swapping in `#[derive(Default)]` (which would give an empty terminal
    /// id and `confirm_destructive_actions: false`, silently turning off
    /// destructive confirms) fails every line here.
    #[test]
    fn entry_default_matches_core_default() {
        let entry = super::PrefsEntry::default();
        assert_eq!(entry.selected_terminal, "gnome-terminal");
        assert!(entry.confirm_destructive_actions);
        assert_eq!(entry.snapshot_prefix, "gdm");
        assert!(entry.default_export_dir.is_empty());
        assert!(!entry.show_skipped_lines);
        assert!(entry.custom_terminals.is_empty());
        // And the core side of the same contract, so a change to
        // `AppConfig::default()` that forgets `PrefsEntry` is caught too.
        let core = gosh_distrobox_core::AppConfig::default();
        assert_eq!(entry.selected_terminal, core.selected_terminal);
        assert_eq!(
            entry.confirm_destructive_actions,
            core.confirm_destructive_actions
        );
        assert_eq!(entry.snapshot_prefix, core.snapshot_prefix);
        assert_eq!(entry.show_skipped_lines, core.show_skipped_lines);
    }
}
