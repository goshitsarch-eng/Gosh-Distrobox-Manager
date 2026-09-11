//! App configuration over cosmic-config (T12, architecture.md §5).
//!
//! Config id `io.github.gosh_distrobox_manager`, version 1 — the same string
//! as `APP_ID`, the gschema id, the metainfo `<id>`, and `.desktop` `Icon=`.
//! One file per key under `…/cosmic/<id>/v1/<key>` (flat per-key storage: a
//! corrupt key cannot take the rest down, §5.1-5).
//!
//! Keys (§5.3): `selected_terminal` (legacy `selected-terminal` import,
//! rewritten to `full_command_id`), `confirm_destructive_actions`,
//! `snapshot_prefix`, `default_export_dir`. Window geometry + refresh
//! interval + show-skipped + distrobox-source are deferred (no readers yet;
//! adding a key without a reader is dead state).
//!
//! Legacy import (one-time): the gschema keys were kebab-case and are read
//! by NO code (§0.4) — only `selected-terminal` carries value (default
//! `'gnome-terminal'`, matched by program, rewritten as `full_command_id`,
//! ARCH-Q5/D11). The DistroShelf `distroshelf-terminals.json` custom list is
//! imported the same way (PKG-9): read once from the host data dir through
//! the runner's filesystem (NOT the sandbox path), then stored as
//! `custom_terminals` and never read again.
//!
//! Degrade, don't crash: `load()` returns `Default` when the config dir is
//! unavailable; every setter is a best-effort write returning `Result`.

use crate::backends::Terminal;
use crate::fakers::{Command, CommandRunner};
use serde::{Deserialize, Serialize};

pub const CONFIG_ID: &str = "io.github.gosh_distrobox_manager";
pub const CONFIG_VERSION: u64 = 1;

/// Upstream DistroShelf GSettings id (D11): the only real migration source —
/// our own gschema was born dead (nothing ever read it). Read once through
/// the env-mapped runner (host-side under Flatpak), then never again.
pub const LEGACY_GSETTINGS_ID: &str = "com.ranfdev.DistroShelf";
/// Pre-rename custom-terminal list (PKG-9 §1.4.2): read once from the HOST
/// data dir through the runner (never the sandbox path), then stored as
/// `custom_terminals` and never read again.
pub const LEGACY_CUSTOM_LIST: &str = "distroshelf-terminals.json";

/// Persisted preferences (subset of §5.3 — only keys with readers).
/// `PartialEq` backs the `watch_config` change check in `app/` (the
/// `CosmicConfigEntry` derive lives there — core takes no libcosmic dep, so
/// the T2 git-pointer/sidecar story stays at five).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Terminal identity (`full_command_id`, program + extra_args).
    pub selected_terminal: String,
    /// Gate destructive confirms (remove/stop-all/delete).
    pub confirm_destructive_actions: bool,
    /// Default prefix for snapshot names.
    pub snapshot_prefix: String,
    /// Start dir for export dialogs.
    pub default_export_dir: String,
    /// Custom (non-read-only) terminals (ARCH-Q5/D11 store).
    pub custom_terminals: Vec<Terminal>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_terminal: "gnome-terminal".to_string(),
            confirm_destructive_actions: true,
            snapshot_prefix: "gdm".to_string(),
            default_export_dir: String::new(),
            custom_terminals: Vec::new(),
        }
    }
}

/// One-time legacy import result (D11/PKG-9): what a previous DistroShelf
/// install (or our own born-dead gschema era) left behind. Applied only for
/// keys our own config does not already set — never overwrites the user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyImport {
    /// Bare program name from `selected-terminal` (rewritten to
    /// `full_command_id` at apply time).
    pub selected_terminal: Option<String>,
    /// Custom list from the host `distroshelf-terminals.json`.
    pub custom_terminals: Vec<Terminal>,
}

/// Read the legacy sources once through the env-mapped runner (D11). Both
/// probes run host-side under Flatpak (`flatpak-spawn --host` mapping) and
/// locally otherwise — either way, never the sandbox path. Every failure is
/// a silent empty (fresh installs have nothing to import; the gsettings
/// schema is absent unless DistroShelf was installed).
pub async fn import_legacy(runner: &CommandRunner) -> LegacyImport {
    let mut out = LegacyImport::default();
    // Upstream `com.ranfdev.DistroShelf` `selected-terminal` (a bare program
    // name in single quotes). `distrobox-executable` is deliberately NOT
    // carried: `"bundled"` is dropped per D11 (nothing ships a distrobox),
    // and geometry belongs to libcosmic's own persistence (packaging §1.6).
    let probe = Command::new_with_args(
        "gsettings",
        ["get", LEGACY_GSETTINGS_ID, "selected-terminal"],
    );
    if let Ok(raw) = runner.output_string(probe).await {
        let program = raw.trim().trim_matches('\'').trim().to_string();
        if !program.is_empty() {
            tracing::info!(target: "gosh_config", "legacy import: selected-terminal={program}");
            out.selected_terminal = Some(program);
        }
    }
    // Pre-rename custom list from the HOST data dir. `$HOME` here is the
    // host's: the command runs host-side through the runner mapping.
    let cat = Command::new_with_args(
        "sh",
        [
            "-c",
            "cat \"${XDG_DATA_HOME:-$HOME/.local/share}/distroshelf-terminals.json\"",
        ],
    );
    // A missing file is the common case (no legacy install) and stays
    // quiet, but a file that EXISTS and does not parse is a real, silent
    // loss of the user's custom list — the original is kept (D11), which
    // only helps if the journal says why it was skipped.
    let parsed = match runner.output_string(cat).await {
        Ok(json) => match serde_json::from_str::<Vec<Terminal>>(json.trim()) {
            Ok(list) => Some(list),
            Err(e) => {
                tracing::warn!(target: "gosh_config", "legacy import: distroshelf-terminals.json did not parse, custom terminals not imported: {e}");
                None
            }
        },
        Err(e) => {
            tracing::debug!(target: "gosh_config", "legacy import: no legacy custom list: {e}");
            None
        }
    };
    if let Some(list) = parsed {
        // First-and-log on ambiguity (D11 Q7): duplicates collapse by
        // `full_command_id`, the first occurrence winning.
        let mut seen = std::collections::HashSet::new();
        out.custom_terminals = list
            .into_iter()
            .filter(|t| {
                if !seen.insert(t.full_command_id()) {
                    tracing::info!(target: "gosh_config", "legacy import: duplicate custom terminal dropped: {}", t.name);
                    return false;
                }
                true
            })
            .collect();
        if !out.custom_terminals.is_empty() {
            tracing::info!(target: "gosh_config", "legacy import: {} custom terminals", out.custom_terminals.len());
        }
    }
    out
}

/// Apply a legacy import onto defaults: kebab-case keys → snake_case fields
/// (§5.2, Q6 closed — rename, not copy). The program value is matched
/// against known terminals and rewritten as `full_command_id` (ARCH-Q5);
/// unknown programs stay verbatim — the picker falls back to first-available.
pub fn apply_legacy(cfg: &mut AppConfig, legacy: &LegacyImport, builtins: &[Terminal]) {
    if let Some(program) = legacy.selected_terminal.as_deref() {
        // Customs first (same precedence as `resolve_terminal`): the custom
        // was the user's explicit addition. First match wins, ambiguity is
        // logged at read time (D11 Q7).
        let rewritten = legacy
            .custom_terminals
            .iter()
            .chain(builtins.iter())
            .find(|t| t.program == program)
            .map(|t| t.full_command_id())
            .unwrap_or_else(|| program.to_string());
        cfg.selected_terminal = rewritten;
    }
    if !legacy.custom_terminals.is_empty() {
        cfg.custom_terminals = legacy.custom_terminals.clone();
    }
}

/// Resolve a stored terminal id against built-ins + customs. Custom
/// terminals win outright (exact `full_command_id`, then program for
/// legacy values), then built-ins the same way — a builtin whose id equals
/// its program must not shadow an imported custom on a legacy
/// bare-program value. `None` for anything unresolvable: the first-
/// available fallback belongs to the caller, since silently substituting a
/// different terminal would launch one the user did not pick.
pub fn resolve_terminal<'a>(
    stored: &str,
    builtins: &'a [Terminal],
    customs: &'a [Terminal],
) -> Option<&'a Terminal> {
    if stored.is_empty() {
        return None;
    }
    for list in [customs, builtins] {
        if let Some(found) = list.iter().find(|t| t.full_command_id() == stored) {
            return Some(found);
        }
        if let Some(found) = list.iter().find(|t| t.program == stored) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(program: &str, extra: &[&str]) -> Terminal {
        Terminal {
            name: program.into(),
            program: program.into(),
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
            separator_arg: "--".into(),
            read_only: true,
        }
    }

    #[test]
    fn apply_legacy_rewrites_program_to_command_id() {
        let builtins = vec![builtin("gnome-terminal", &[])];
        let mut cfg = AppConfig::default();
        apply_legacy(
            &mut cfg,
            &LegacyImport {
                selected_terminal: Some("gnome-terminal".into()),
                custom_terminals: vec![],
            },
            &builtins,
        );
        assert_eq!(cfg.selected_terminal, "gnome-terminal");
        // Unknown programs stay verbatim (picker falls back).
        let mut cfg2 = AppConfig::default();
        apply_legacy(
            &mut cfg2,
            &LegacyImport {
                selected_terminal: Some("nope-term".into()),
                custom_terminals: vec![],
            },
            &builtins,
        );
        assert_eq!(cfg2.selected_terminal, "nope-term");
        // Empty import leaves defaults alone (never overwrites).
        let mut cfg3 = AppConfig::default();
        apply_legacy(&mut cfg3, &LegacyImport::default(), &builtins);
        assert_eq!(cfg3, AppConfig::default());
    }

    #[test]
    fn apply_legacy_prefers_custom_match() {
        let builtins = vec![builtin("flatpak", &["run", "org.gnome.Console"])];
        let custom = Terminal {
            name: "Mine".into(),
            program: "flatpak".into(),
            extra_args: vec!["run".into(), "my.term".into()],
            separator_arg: "--".into(),
            read_only: false,
        };
        let mut cfg = AppConfig::default();
        apply_legacy(
            &mut cfg,
            &LegacyImport {
                selected_terminal: Some("flatpak".into()),
                custom_terminals: vec![custom],
            },
            &builtins,
        );
        // Customs win on ambiguity (same precedence as resolve_terminal).
        assert_eq!(cfg.selected_terminal, "flatpak run my.term");
        assert_eq!(cfg.custom_terminals.len(), 1);
    }

    #[test]
    fn resolve_prefers_custom_exact_then_custom_program_then_builtin() {
        let customs = vec![Terminal {
            name: "Mine".into(),
            program: "gnome-terminal".into(),
            extra_args: vec!["--mine".into()],
            separator_arg: "--".into(),
            read_only: false,
        }];
        let builtins = vec![Terminal {
            name: "GNOME Terminal".into(),
            program: "gnome-terminal".into(),
            extra_args: vec![],
            separator_arg: "--".into(),
            read_only: true,
        }];
        // Exact full_command_id wins over program match.
        assert_eq!(
            resolve_terminal("gnome-terminal --mine", &builtins, &customs)
                .unwrap()
                .name,
            "Mine"
        );
        // Bare program matches (legacy values).
        assert_eq!(
            resolve_terminal("gnome-terminal", &builtins, &customs)
                .unwrap()
                .name,
            "Mine"
        );
        // Unknown or absent resolves to nothing — the caller falls back to
        // first-available. Substituting here would launch a terminal the
        // user never chose and hide a bad stored value.
        assert!(resolve_terminal("nope", &builtins, &customs).is_none());
        assert!(resolve_terminal("", &builtins, &customs).is_none());
        assert!(resolve_terminal("nope", &[], &[]).is_none());
    }

    #[test]
    fn defaults_match_doc() {
        let cfg = AppConfig::default();
        assert!(cfg.confirm_destructive_actions);
        assert_eq!(cfg.snapshot_prefix, "gdm");
    }

    /// One-time probe wiring (D11/PKG-9): both legacy sources go through
    /// the env-mapped runner, values are unquoted, and a duplicate custom
    /// collapses by `full_command_id` (first wins, D11 Q7).
    #[tokio::test]
    async fn import_legacy_reads_gsettings_and_host_list() {
        use crate::fakers::NullCommandRunnerBuilder;
        let mut builder = NullCommandRunnerBuilder::new();
        builder.cmd(
            &["gsettings", "get", LEGACY_GSETTINGS_ID, "selected-terminal"],
            "'gnome-terminal'",
        );
        builder.cmd(
            &[
                "sh",
                "-c",
                "cat \"${XDG_DATA_HOME:-$HOME/.local/share}/distroshelf-terminals.json\"",
            ],
            r#"[{"name":"Mine","program":"flatpak","extra_args":["run","my.term"],"separator_arg":"--","read_only":false},
                {"name":"Dup","program":"flatpak","extra_args":["run","my.term"],"separator_arg":"--","read_only":false}]"#,
        );
        let runner = builder.build();
        let got = import_legacy(&runner).await;
        assert_eq!(got.selected_terminal.as_deref(), Some("gnome-terminal"));
        assert_eq!(got.custom_terminals.len(), 1, "duplicate collapses");
        assert_eq!(got.custom_terminals[0].name, "Mine");
    }

    /// Every probe failure is a silent empty: fresh installs have neither
    /// `gsettings` schema nor the pre-rename list file. The import must
    /// never be the reason a first run fails.
    #[tokio::test]
    async fn import_legacy_is_empty_when_sources_are_absent() {
        use crate::fakers::NullCommandRunnerBuilder;
        let runner = NullCommandRunnerBuilder::new().build();
        let got = import_legacy(&runner).await;
        assert_eq!(got, LegacyImport::default());
    }
}
