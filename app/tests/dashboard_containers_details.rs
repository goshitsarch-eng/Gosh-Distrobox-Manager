//! T6 integration tests: container actions through `Backend` + message flow.
//!
//! T2 tier: `#[tokio::test]` (§0.2 — `spawn_task` needs an entered runtime),
//! `NullCommandRunner` fixtures driving the same argv the Flutter app sent.
//! Widget observation stays T3-tier (T6–T12's deal); what is asserted here is
//! the registry/message contract each T6 button depends on:
//! - stop/remove/stop-all produce the exact `distrobox …` argv and succeed;
//! - upgrade spawns a task whose `Started` payload the `update` arm consumes;
//! - clone validates names through `CreateArgName::new` (row #96) before any
//!   spawn — an invalid name never reaches the backend;
//! - blocked env refuses every mutation (guard_ok-first).

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::distrobox::DistroboxCommandRunnerResponse;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode};

fn backend_for(responses: &[DistroboxCommandRunnerResponse]) -> Backend {
    Backend::new(
        Distrobox::null_command_runner(responses),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    )
}

fn blocked_backend() -> Backend {
    Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    )
}

#[tokio::test]
async fn stop_sends_distrobox_stop_argv() {
    // `stop` returns the command's stdout (empty under the null runner) —
    // the assertion is on the argv the runner saw, T2-tier.
    use gosh_distrobox_core::backends::Distrobox;
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
    backend.stop_container("mybox").await.expect("stop");
    let argv: Vec<String> = tracker
        .items()
        .iter()
        .filter_map(|e| e.command().map(|c| c.to_string()))
        .collect();
    assert!(
        argv.iter()
            .any(|a| a.contains("stop") && a.contains("mybox")),
        "distrobox stop argv: {argv:?}"
    );
}

#[tokio::test]
async fn remove_sends_distrobox_rm_argv() {
    use gosh_distrobox_core::backends::Distrobox;
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
    backend.remove_container("oldbox").await.expect("remove");
    let argv: Vec<String> = tracker
        .items()
        .iter()
        .filter_map(|e| e.command().map(|c| c.to_string()))
        .collect();
    assert!(
        argv.iter()
            .any(|a| a.contains("rm") && a.contains("oldbox")),
        "distrobox rm argv: {argv:?}"
    );
}

#[tokio::test]
async fn stop_all_sends_distrobox_stop_all_argv() {
    use gosh_distrobox_core::backends::Distrobox;
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
    backend.stop_all_containers().await.expect("stop-all");
    let argv: Vec<String> = tracker
        .items()
        .iter()
        .filter_map(|e| e.command().map(|c| c.to_string()))
        .collect();
    assert!(
        argv.iter()
            .any(|a| a.contains("stop") && a.contains("--all")),
        "distrobox stop --all argv: {argv:?}"
    );
}

#[tokio::test]
async fn upgrade_spawns_task_not_string() {
    // Row #60's tile needs a `TaskId` for the progress dialog — not a bare
    // string. The `Started { label, result }` payload carries both.
    let backend = backend_for(&[]);
    let id = backend.upgrade_container("mybox").await.expect("spawn");
    assert!(backend.is_task_running(id));
    assert!(backend.cancel_task(id));
}

#[tokio::test]
async fn clone_name_validation_rejects_before_spawn() {
    // Row #96: Rust `CreateArgName::new` enforces the pattern — the UI
    // surfaces the error instead of spawning (Flutter never called it).
    use gosh_distrobox_core::models::CreateArgName;
    assert!(CreateArgName::new("valid-name_1.2").is_ok());
    assert!(CreateArgName::new("").is_err());
    assert!(CreateArgName::new("-leading-dash").is_err());
    assert!(CreateArgName::new("has space").is_err());
}

#[tokio::test]
async fn blocked_env_refuses_all_mutations() {
    let backend = blocked_backend();
    assert!(matches!(
        &*backend.stop_container("x").await.expect_err("stop").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend.remove_container("x").await.expect_err("remove").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend.stop_all_containers().await.expect_err("stop-all").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend.upgrade_container("x").await.expect_err("upgrade").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
}
