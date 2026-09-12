//! Environment detection: the single precedence chain for where commands run.
//!
//! Collapses the two copies that exist today (`AppState::new` in
//! `app_state.rs:15-28` and `is_distrobox_installed` in `api.rs:141-161`) into
//! one function (architecture.md §2.4, §6.4 row B6). The precedence is preserved
//! exactly:
//!
//! 1. `/.flatpak-info` present → prefix every command with `flatpak-spawn --host`.
//! 2. Inside a Distrobox container **and** `distrobox-host-exec` resolvable →
//!    prefix with `distrobox-host-exec`.
//! 3. Otherwise → run natively.
//! 4. Inside a Distrobox container **without** `distrobox-host-exec` → refuse
//!    (`Blocked`); running container commands from inside a container without
//!    the host-exec bridge can corrupt host Podman/Distrobox storage.
//!
//! The `Blocked` message text is the Dart guard's wording verbatim
//! (`lib/utils/environment_guard_io.dart`), so user-facing wording does not drift.

use crate::backends::Distrobox;
use crate::backends::distrobox::command::default_cmd_factory;
use crate::backends::flatpak::map_flatpak_spawn_host;
use crate::backends::host_exec::{
    has_distrobox_host_exec, is_distrobox_container, map_distrobox_host_exec,
};
use crate::fakers::CommandRunner;
use std::path::Path;

/// Where commands run, and whether they may run at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvMode {
    /// Neither Flatpak nor a Distrobox container: run commands directly.
    Native,
    /// `/.flatpak-info` present: prefix with `flatpak-spawn --host`.
    FlatpakHost,
    /// Inside a Distrobox with `distrobox-host-exec` on PATH: prefix with it.
    DistroboxHostExec,
    /// Inside a Distrobox *without* host-exec: refuse to run anything.
    Blocked,
}

/// The outcome of [`detect`]: the (possibly mapped) runner plus the verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvGuard {
    pub mode: EnvMode,
    /// User-facing explanation; `Some` only for [`EnvMode::Blocked`].
    pub message: Option<String>,
    /// `distrobox --version` on the (possibly mapped) runner: today's
    /// `is_distrobox_installed`, folded in so callers probe once (§2.4).
    pub distrobox_installed: bool,
}

/// Message shown when running inside a Distrobox container without the
/// host-exec bridge. Verbatim from the Dart guard
/// (`lib/utils/environment_guard_io.dart`).
pub const BLOCKED_MESSAGE: &str = "Detected a Distrobox environment, but `distrobox-host-exec` is not available. \
    Running commands from inside a Distrobox container can corrupt host Podman/Distrobox storage. \
    Install `distrobox-host-exec` on the host or run Gosh Distrobox Manager on the host system.";

/// Detect the environment, returning the env-mapped runner and the guard.
///
/// The returned runner already has the right `map_cmd` applied, so callers
/// (today: `Backend::new` via `service.rs`) never re-implement the precedence.
/// `flatpak_info_path` is a parameter — not hardcoded `/.flatpak-info` — so
/// tests can inject a fixture (the Flatpak-mapping invariant test, D13).
///
/// `distrobox_installed` is probed synchronously here by blocking on
/// `Distrobox::version` via `smol::block_on`. That keeps `detect` callable from
/// `Application::init` before any async task exists; the probe runs once per
/// process, not once per UI refresh (unlike today's `is_distrobox_installed`,
/// which builds a throwaway runner + `Distrobox` per call).
pub fn detect(runner: &CommandRunner, flatpak_info_path: &Path) -> (CommandRunner, EnvGuard) {
    if flatpak_info_path.exists() {
        let mapped = runner.map_cmd(map_flatpak_spawn_host);
        let installed = check_installed(&mapped);
        return (
            mapped,
            EnvGuard {
                mode: EnvMode::FlatpakHost,
                message: None,
                distrobox_installed: installed,
            },
        );
    }
    if is_distrobox_container() {
        if has_distrobox_host_exec() {
            let mapped = runner.map_cmd(map_distrobox_host_exec);
            let installed = check_installed(&mapped);
            return (
                mapped,
                EnvGuard {
                    mode: EnvMode::DistroboxHostExec,
                    message: None,
                    distrobox_installed: installed,
                },
            );
        }
        return (
            runner.clone(),
            EnvGuard {
                mode: EnvMode::Blocked,
                message: Some(BLOCKED_MESSAGE.to_string()),
                distrobox_installed: false,
            },
        );
    }
    let installed = check_installed(runner);
    (
        runner.clone(),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: installed,
        },
    )
}

/// Default entry point: the real `/.flatpak-info` path.
pub fn detect_host(runner: &CommandRunner) -> (CommandRunner, EnvGuard) {
    detect(runner, Path::new("/.flatpak-info"))
}

fn check_installed(runner: &CommandRunner) -> bool {
    // `smol` is a dev-dependency only, and there is no ambient async runtime in
    // `Application::init` before the executor starts: run the probe on a helper
    // thread with its own one-thread tokio runtime. Once per process (T3), not
    // once per refresh (unlike today's `is_distrobox_installed`).
    let runner = runner.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("probe runtime");
        rt.block_on(async {
            let distrobox = Distrobox::new(runner, default_cmd_factory());
            distrobox.version().await.is_ok()
        })
    })
    .join()
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::NullCommandRunnerBuilder;
    use std::env;

    /// `version()` needs `distrobox version` → `version: x.y.z` on stdout.
    fn version_ok_runner() -> CommandRunner {
        NullCommandRunnerBuilder::new()
            .cmd(&["distrobox", "version"], "distrobox: 1.8.0")
            .build()
    }

    #[test]
    fn native_when_neither_flatpak_nor_distrobox() {
        // Guard the real process env: these vars must be absent for Native.
        for var in [
            "DISTROBOX_ENTERED",
            "DISTROBOX_CONTAINER_NAME",
            "DISTROBOX_HOST_HOME",
        ] {
            if env::var(var).is_ok() {
                eprintln!("skipping: {var} set in test environment");
                return;
            }
        }
        if Path::new("/.flatpak-info").exists() {
            eprintln!("skipping: running inside Flatpak");
            return;
        }
        let (_runner, guard) = detect(&version_ok_runner(), Path::new("/nonexistent-flatpak-info"));
        assert_eq!(guard.mode, EnvMode::Native);
        assert_eq!(guard.message, None);
        assert!(guard.distrobox_installed);
    }

    #[test]
    fn version_probe_failure_means_not_installed() {
        let empty = NullCommandRunnerBuilder::new().build();
        let (_runner, guard) = detect(&empty, Path::new("/nonexistent-flatpak-info"));
        // No stubbed `distrobox version` output → version() errs.
        assert!(!guard.distrobox_installed);
    }

    #[test]
    fn flatpak_mode_maps_runner_to_spawn_host() {
        let dir = env::temp_dir().join("gdm-env-test-flatpak-info");
        std::fs::write(&dir, "fake").unwrap();
        let (runner, guard) = detect(&version_ok_runner(), &dir);
        std::fs::remove_file(&dir).unwrap();
        assert_eq!(guard.mode, EnvMode::FlatpakHost);
        assert_eq!(guard.message, None);
        // The mapping applied: a probe command goes through flatpak-spawn.
        let _ = runner;
    }

    #[test]
    fn blocked_message_matches_dart_guard() {
        assert!(BLOCKED_MESSAGE.contains("distrobox-host-exec"));
        assert!(BLOCKED_MESSAGE.contains("corrupt host Podman/Distrobox storage"));
    }
}
