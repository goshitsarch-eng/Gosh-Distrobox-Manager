//! T7 backend-contract tests: the `Backend` surface the wizard depends on.
//!
//! T2 tier: `#[tokio::test]` (§0.2), `NullCommandRunner` fixtures. Honest
//! scope — these never construct `App`/`WizardState` (binary-only crate,
//! no lib target) and cover no `update_wizard` arm; the wizard's pure logic
//! (effective image, name default, filters, preselect) is unit-tested in
//! `wizard.rs`. What is asserted here is the backend contract each wizard
//! arm depends on:
//! - `create_container` sends the full `distrobox create …` argv (image,
//!   name, init, nvidia, home, volumes) — the T2-tier version of Flutter's
//!   `create` test;
//! - an invalid name never spawns (row #96 — the UI validates first, but the
//!   backend type enforces it anyway);
//! - blocked env refuses create;
//! - images catalogue loads (row #100's content source).

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::distrobox::DistroboxCommandRunnerResponse;
use gosh_distrobox_core::models::{CreateArgName, CreateArgs, Volume};
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode, TaskEvent};
use std::str::FromStr;

fn backend_with_tracker(
    responses: &[DistroboxCommandRunnerResponse],
) -> (
    Backend,
    gosh_distrobox_core::fakers::OutputTracker<gosh_distrobox_core::fakers::CommandRunnerEvent>,
) {
    let runner = Distrobox::null_command_runner(responses);
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

fn full_args(name: &str) -> CreateArgs {
    CreateArgs {
        init: true,
        nvidia: true,
        home_path: Some("/home/me/boxes/test".to_string()),
        image: "docker.io/library/ubuntu:latest".to_string(),
        name: CreateArgName::new(name).expect("valid name"),
        volumes: vec![Volume::from_str("/mnt/data:/mnt/data").expect("volume")],
    }
}

#[tokio::test]
async fn create_sends_full_argv() {
    let (backend, tracker) = backend_with_tracker(&[]);
    let id = backend
        .create_container(full_args("wizard-box"))
        .await
        .expect("spawn");
    // Drain to completion (null runner exits 0 instantly).
    let rx = backend.tasks().subscribe(id).expect("subscribes");
    let mut ok = false;
    while let Ok(ev) = rx.recv().await {
        if matches!(ev, TaskEvent::Finished { success: true }) {
            ok = true;
            break;
        }
    }
    assert!(ok, "create task completes");
    let argv = argv(&tracker);
    let create = argv
        .iter()
        .find(|a| a.contains("create"))
        .expect("create argv: {argv:?}");
    for flag in [
        "--image",
        "docker.io/library/ubuntu:latest",
        "--name",
        "wizard-box",
        "--init",
        "--nvidia",
        "--home",
        "/home/me/boxes/test",
        "--volume",
        "/mnt/data:/mnt/data",
    ] {
        assert!(create.contains(flag), "{flag} in {create}");
    }
}

#[test]
fn create_name_type_rejects_bad_names() {
    // Row #96: `CreateArgName::new` is the enforcement point — the wizard
    // surfaces it inline, but even a direct caller cannot construct bad args.
    assert!(CreateArgName::new("").is_err());
    assert!(CreateArgName::new("has space").is_err());
    assert!(CreateArgName::new("-dash").is_err());
}

#[tokio::test]
async fn create_blocked_env_refuses() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    );
    let err = backend
        .create_container(full_args("x"))
        .await
        .expect_err("blocked");
    assert!(matches!(
        &*err.0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
}

#[tokio::test]
async fn images_catalogue_loads() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[DistroboxCommandRunnerResponse::new_common_images()]),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    let images = backend.images().await.expect("images");
    assert!(!images.is_empty());
    assert!(images.iter().any(|i| i.contains("ubuntu")));
}
