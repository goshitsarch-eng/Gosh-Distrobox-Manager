//! T10 integration tests: snapshot/export/import/clone through `Backend`.
//!
//! T2 tier: `#[tokio::test]` (§0.2), `NullCommandRunner` fixtures. Page logic
//! (defaults, validation, dialogs) is unit-tested in `backups.rs`; what is
//! asserted here is the backend contract each T10 arm depends on:
//! - snapshot list/create/delete go through podman/docker with the right argv;
//! - restore/export/import spawn tasks (failures surface via Started Err,
//!   never silent — #147);
//! - clone validates `.trim()` consistency (#148);
//! - blocked env refuses everything.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::models::CreateArgName;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode, TaskEvent};

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

#[tokio::test]
async fn snapshot_list_queries_images() {
    let (backend, tracker) = backend_with_tracker();
    backend.list_snapshots().await.expect("list");
    // B7: the whole ordered argv, not a `contains("images")` that would pass
    // for either runtime and for a drifted argv alike. Exactly one command
    // also proves podman succeeding did not trigger the docker fallback.
    assert_eq!(
        argv(&tracker),
        vec![
            "podman images --format {{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Created}}\t{{.Size}}"
                .to_string()
        ]
    );
}

#[tokio::test]
async fn snapshot_create_commits() {
    // create_snapshot resolves the container ID first (`podman ps`), so
    // the fixture answers the ID lookup, then the commit is asserted.
    use gosh_distrobox_core::fakers::NullCommandRunnerBuilder;
    let mut builder = NullCommandRunnerBuilder::new();
    builder.cmd(
        &[
            "podman",
            "ps",
            "-a",
            "--filter",
            "name=^mybox$",
            "--format",
            "{{.ID}}",
        ],
        "abc123",
    );
    let runner = builder.build();
    let tracker = runner.output_tracker();
    let backend = Backend::new(
        runner,
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    backend
        .create_snapshot("mybox", "mybox-snapshot")
        .await
        .expect("create");
    // Both commands, in order: the ID lookup, then the commit. The commit
    // commits `abc123` (the ID) rather than the name, so both runtimes commit
    // the same object.
    assert_eq!(
        argv(&tracker),
        vec![
            "podman ps -a --filter name=^mybox$ --format {{.ID}}".to_string(),
            "podman commit abc123 mybox-snapshot".to_string(),
        ]
    );
}

#[tokio::test]
async fn restore_export_import_spawn_tasks() {
    use gosh_distrobox_core::models::CreateArgs;
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    let args = CreateArgs {
        init: false,
        nvidia: false,
        home_path: None,
        image: "snap".to_string(),
        name: CreateArgName::new("restored").expect("valid"),
        volumes: vec![],
    };
    for id in [
        backend
            .restore_from_snapshot("snap", "restored")
            .await
            .expect("restore"),
        backend
            .export_container("mybox", "/tmp/x.tar")
            .await
            .expect("export"),
        backend
            .import_container("/tmp/x.tar", "img")
            .await
            .expect("import"),
        backend.clone_container("mybox", args).await.expect("clone"),
    ] {
        let rx = backend.tasks().subscribe(id).expect("subscribes");
        let mut finished = false;
        while let Ok(ev) = rx.recv().await {
            if matches!(ev, TaskEvent::Finished { .. }) {
                finished = true;
                break;
            }
        }
        assert!(finished, "task finishes");
    }
}

#[test]
fn clone_name_validation_matches_siblings() {
    // Row #148: the shared clone path trims before `CreateArgName::new`
    // (like CreateRequested + RestoreConfirmed + export/import arms), so
    // padded names validate instead of erroring. `CreateArgName` itself
    // stays strict (anchored regex) — trimming is the caller's job.
    assert!(CreateArgName::new("padded").is_ok());
    // Complement (pins WHY the caller trims): the type itself is strict —
    // padded input errors without the caller's `.trim()`.
    assert!(CreateArgName::new("  padded  ").is_err());
    assert!(CreateArgName::new("  padded  ".trim()).is_ok());
    assert!(CreateArgName::new("a b").is_err());
    assert!(CreateArgName::new("").is_err());
}

#[tokio::test]
async fn blocked_env_refuses_backup_ops() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    );
    assert!(
        backend
            .list_snapshots()
            .await
            .expect_err("list")
            .0
            .to_string()
            .contains("Distrobox container")
    );
}

// ------------------------------------------------------------ T18 / row #134

/// T2: the header gate reads the LIVE `Backend::env()`, not a re-spelled
/// fixture. Both backends are real `Backend`s over `NullCommandRunner`; the
/// only difference is the guard, and the gate's verdict on the exact header
/// `Message`s flips with it. (`header_end`'s call itself is source-verified —
/// every button goes through `gate_header_message` — per the T17 precedent:
/// no App harness exists until T20.)
#[tokio::test]
async fn header_gate_follows_the_live_backend_env() {
    use gosh_distrobox_manager::message::{
        AppMsg, BackupsMsg, ContainerMsg, Message, TaskMsg, is_blocked,
    };
    use gosh_distrobox_manager::views::gate_header_message;

    fn backend_for(mode: EnvMode, installed: bool) -> Backend {
        Backend::new(
            Distrobox::null_command_runner(&[]),
            EnvGuard {
                mode,
                message: None,
                distrobox_installed: installed,
            },
        )
    }

    // The six header messages `header_end` sends, in page order.
    let header_messages = || {
        vec![
            Message::Containers(ContainerMsg::NewContainerRequested),
            Message::Containers(ContainerMsg::RefreshRequested),
            Message::Containers(ContainerMsg::UpgradeAllRequested),
            Message::Tasks(TaskMsg::ClearCompleted),
            Message::Backups(BackupsMsg::CreateDialogRequested),
            Message::Apps(AppMsg::ReloadRequested("box".into())),
        ]
    };

    for backend in [
        backend_for(EnvMode::Blocked, false),
        backend_for(EnvMode::Native, false),
    ] {
        let guard = backend.env();
        let verdicts: Vec<bool> = header_messages()
            .into_iter()
            .map(|m| {
                gate_header_message(m, is_blocked(&guard), guard.distrobox_installed).is_some()
            })
            .collect();
        assert_eq!(
            verdicts,
            vec![false, true, false, true, false, false],
            "in a gated env only Refresh (re-probe) and Clear Completed (local) stay live"
        );
    }

    let healthy = backend_for(EnvMode::Native, true);
    let guard = healthy.env();
    for msg in header_messages() {
        assert!(
            gate_header_message(msg, is_blocked(&guard), guard.distrobox_installed).is_some(),
            "every header action stays live on a healthy backend"
        );
    }
}
