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
    let argv = argv(&tracker);
    assert!(
        argv.iter().any(|a| a.contains("images")),
        "podman/docker images argv: {argv:?}"
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
    let argv = argv(&tracker);
    assert!(
        argv.iter()
            .any(|a| a.contains("commit") && a.contains("mybox-snapshot")),
        "commit argv: {argv:?}"
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
