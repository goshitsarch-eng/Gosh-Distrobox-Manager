//! T5 integration tests: `spawn_task` through `Backend` + `TaskMsg` flow.
//!
//! T2 tier (architecture.md §7, PLAN verification tiers D19): `Backend`
//! methods run under `#[tokio::test]` (they internally `tokio::spawn` — §0.2,
//! so `smol::block_on` would have no ambient runtime), driving the same argv
//! the Flutter app sent via `NullCommandRunner` fixtures. Assertions cover
//! the registry state and message payloads the `App::update` arms consume —
//! not the widget tree (widget observation is T6–T12's T3 tier).
//!
//! `Backend::new` takes the env-mapped runner directly, so no
//! `/.flatpak-info` manipulation is needed; the mapping invariant itself is
//! covered in `core`'s `env::tests`.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::distrobox::DistroboxCommandRunnerResponse;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode, TaskEvent};

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

/// Drain a task's subscription until `Finished`, collecting output lines.
/// The registry sender drops at finish, so the receiver closes afterwards.
async fn drain_to_finished(backend: &Backend, id: gosh_distrobox_core::TaskId) -> Vec<String> {
    let rx = backend
        .tasks()
        .subscribe(id)
        .expect("spawned task subscribes");
    let mut lines = Vec::new();
    let mut success = false;
    while let Ok(ev) = rx.recv().await {
        match ev {
            TaskEvent::Output(line) => lines.push(line),
            TaskEvent::Finished { success: s } => {
                success = s;
                break;
            }
        }
    }
    assert!(
        success,
        "Null-runner task completes successfully: {lines:?}"
    );
    lines
}

#[tokio::test]
async fn create_spawns_task_with_flutters_argv_and_messages() {
    use gosh_distrobox_core::models::CreateArgs;
    let backend = backend_for(&[]);
    let args = CreateArgs {
        init: false,
        nvidia: false,
        home_path: None,
        image: "docker.io/library/ubuntu:latest".to_string(),
        name: gosh_distrobox_core::models::CreateArgName::new("test-box").expect("valid name"),
        volumes: vec![],
    };
    let id = backend
        .create_container(args)
        .await
        .expect("spawn succeeds");
    // The task is registered and running until drained.
    assert!(backend.is_task_running(id));
    assert!(backend.active_tasks().contains(&id));
    let lines = drain_to_finished(&backend, id).await;
    // Success line is the Flutter body's "Task completed successfully".
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Task completed successfully")),
        "success line streamed: {lines:?}"
    );
    assert!(!backend.is_task_running(id));
}

#[tokio::test]
async fn upgrade_spawns_with_upgrade_argv() {
    let backend = backend_for(&[]);
    let id = backend.upgrade_container("mybox").await.expect("spawn");
    let lines = drain_to_finished(&backend, id).await;
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Upgrade completed successfully")),
        "upgrade success line: {lines:?}"
    );
}

#[tokio::test]
async fn cancel_marks_task_unsuccessful() {
    use gosh_distrobox_core::models::CreateArgs;
    let backend = backend_for(&[]);
    let args = CreateArgs {
        init: false,
        nvidia: false,
        home_path: None,
        image: "docker.io/library/ubuntu:latest".to_string(),
        name: gosh_distrobox_core::models::CreateArgName::new("cancel-box").expect("valid name"),
        volumes: vec![],
    };
    let id = backend
        .create_container(args)
        .await
        .expect("spawn succeeds");
    assert!(backend.cancel_task(id));
    assert!(!backend.is_task_running(id));
    // Second cancel is a no-op (already completed).
    assert!(!backend.cancel_task(id));
}

#[tokio::test]
async fn blocked_env_refuses_spawn() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    );
    let err = backend
        .upgrade_container("mybox")
        .await
        .expect_err("blocked env refuses spawn");
    assert!(matches!(
        &*err.0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
}
