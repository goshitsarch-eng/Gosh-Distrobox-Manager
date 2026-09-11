//! T9 integration tests: start op + terminal launch through `Backend`.
//!
//! T2 tier: `#[tokio::test]` (§0.2), `NullCommandRunner` fixtures. Page logic
//! (sections, picker, banners) lives in `updates.rs`/`terminal.rs`
//! unit scope; what is asserted here is the backend contract each T9 arm
//! depends on:
//! - `start_container` sends `podman start <name>` (B5) and the
//!   transition is observed via `list()`, not assumed;
//! - `enter_command` returns the `distrobox enter <name>` argv for display
//!   (row #69) without spawning;
//! - `launch_terminal` spawns `<terminal…> <enter argv>` through the
//!   env-mapped runner (D8 — mapping applies to the terminal too);
//! - blocked env refuses start/launch.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::supported_terminals::Terminal;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode};

fn backend_with_tracker() -> (
    Backend,
    gosh_distrobox_core::fakers::OutputTracker<gosh_distrobox_core::fakers::CommandRunnerEvent>,
) {
    let runner = Distrobox::null_command_runner(&[]);
    let tracker = runner.output_tracker();
    let backend = Backend::new(
        runner,
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    (backend, tracker)
}

fn argv(
    tracker: &gosh_distrobox_core::fakers::OutputTracker<
        gosh_distrobox_core::fakers::CommandRunnerEvent,
    >,
) -> Vec<String> {
    tracker
        .items()
        .iter()
        .filter_map(|e| e.command().map(|c| c.to_string()))
        .collect()
}

fn test_terminal() -> Terminal {
    Terminal {
        name: "Test Terminal".to_string(),
        program: "test-term".to_string(),
        extra_args: vec!["-e".to_string()],
        separator_arg: "--".to_string(),
        read_only: true,
    }
}

#[tokio::test]
async fn start_sends_podman_start_argv() {
    // B5: `podman start <name>` — there is no `distrobox start` subcommand
    // (verified against distrobox 1.8.2.5's dispatch table).
    let (backend, tracker) = backend_with_tracker();
    backend.start_container("mybox").await.expect("start");
    let argv = argv(&tracker);
    assert!(
        argv.iter().any(|a| a == "podman start mybox"),
        "podman start argv: {argv:?}"
    );
}

#[test]
fn enter_command_returns_display_argv_without_spawning() {
    // Row #69: argv for display — no spawn (tracker stays empty).
    let (backend, tracker) = backend_with_tracker();
    let display = backend.enter_command("mybox");
    assert_eq!(display[..2], ["distrobox".to_string(), "enter".to_string()]);
    assert!(display.contains(&"mybox".to_string()));
    assert!(argv(&tracker).is_empty(), "display must not spawn");
}

#[test]
fn launch_spawns_terminal_with_enter_argv() {
    // D8: `test-term -e -- distrobox enter mybox` through the runner.
    let (backend, tracker) = backend_with_tracker();
    backend
        .launch_terminal("mybox", &test_terminal())
        .expect("launch");
    let argv = argv(&tracker);
    let launch = argv
        .iter()
        .find(|a| a.starts_with("test-term"))
        .expect("terminal argv: {argv:?}");
    for part in ["-e", "--", "distrobox", "enter", "mybox"] {
        assert!(launch.contains(part), "{part} in {launch}");
    }
}

#[test]
fn builtin_terminal_table_is_nonempty() {
    use gosh_distrobox_core::backends::supported_terminals::builtin_terminals;
    let table = builtin_terminals();
    assert!(!table.is_empty());
    assert!(table.iter().any(|t| t.program == "cosmic-term"));
}

#[tokio::test]
async fn blocked_env_refuses_start_and_launch() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    );
    assert!(matches!(
        &*backend.start_container("x").await.expect_err("start").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend
            .launch_terminal("x", &test_terminal())
            .expect_err("launch")
            .0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
}
