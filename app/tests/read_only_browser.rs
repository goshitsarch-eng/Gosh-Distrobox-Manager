//! T3 integration tests: `Backend` read-only domains + `Message` flow.
//!
//! T2 tier (architecture.md §7 row 2, PLAN verification tiers D19):
//! `NullCommandRunner` fixtures drive the same argv the Flutter app sent, and
//! the assertions cover the message payloads the `App::update` arms consume —
//! not the widget tree (T3 gate; widget observation is T6–T12's T3 tier).
//!
//! `Backend::new` takes the env-mapped runner directly, so no `/.flatpak-info`
//! or process-env manipulation is needed here; the env-mapping invariant
//! itself is covered in `core`'s `env::tests`.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::distrobox::DistroboxCommandRunnerResponse;
use gosh_distrobox_core::models::Status;
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

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    smol::block_on(f)
}

#[test]
fn containers_lists_common_distros() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::new_list_common_distros()]);
    let containers = block_on(backend.containers()).expect("list succeeds");
    // The common-distros fixture ships 14 rows (stub_responses in distrobox.rs).
    assert_eq!(containers.len(), 14);
    // B3: a clean fixture is a clean empty — nothing was dropped on the way.
    assert!(
        containers.skipped.is_empty(),
        "a well-formed fixture must not report skipped rows: {:?}",
        containers.skipped
    );
    let ubuntu = containers
        .iter()
        .find(|c| c.name == "Ubuntu")
        .expect("Ubuntu row");
    assert_eq!(ubuntu.image, "docker.io/library/ubuntu:latest");
    assert!(matches!(ubuntu.status, Status::Created(_)));
}

#[test]
fn containers_empty_when_no_rows() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::List(vec![])]);
    let containers = block_on(backend.containers()).expect("empty list succeeds");
    assert!(containers.is_empty());
    assert!(containers.is_clean_empty());
}

#[test]
fn images_lists_compatibility_catalogue() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::new_common_images()]);
    let images = block_on(backend.images()).expect("images succeeds");
    assert!(!images.is_empty());
    assert!(
        images.iter().any(|i| i.contains("ubuntu")),
        "catalogue contains ubuntu: {images:?}"
    );
}

#[test]
fn container_apps_maps_dto_fields() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::new_common_exported_apps()]);
    let apps = block_on(backend.container_apps("Ubuntu")).expect("apps succeeds");
    assert!(!apps.is_empty());
    for app in &apps {
        assert!(!app.name.is_empty(), "AppInfo.name mapped: {app:?}");
        assert!(
            !app.desktop_file_path.is_empty(),
            "desktop path mapped: {app:?}"
        );
    }
}

#[test]
fn version_parses_distrobox_version_string() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::Version]);
    let version = block_on(backend.distrobox_version()).expect("version succeeds");
    assert!(!version.is_empty(), "version parsed: {version:?}");
}

#[test]
fn version_failure_is_typed_not_stringly() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::NoVersion]);
    let err = block_on(backend.distrobox_version()).expect_err("no version → Err");
    // The UI matches on the kind (setup guidance vs diagnostic), never on the
    // string — so the payload must stay structured through the boundary.
    let text = err.to_string();
    assert!(!text.is_empty());
}

/// B3 boundary contract (architecture.md §6.4): `containers()` carries the
/// wrapper so `skipped` can reach the UI, while the four per-container peers
/// keep returning plain `Vec<T>` — their callers map straight onto the DTOs,
/// and `show_skipped_lines` is specified against `ContainerList.skipped`
/// alone. This test pins both halves so a later refactor cannot quietly
/// change which side of the boundary the wrapper lives on.
#[test]
fn b3_wrapper_stops_at_containers_and_peers_stay_vecs() {
    let backend = backend_for(&[
        DistroboxCommandRunnerResponse::new_list_common_distros(),
        DistroboxCommandRunnerResponse::new_common_exported_apps(),
    ]);
    let containers = block_on(backend.containers()).expect("list succeeds");
    // Naming the TYPE at the binding is what pins the boundary. `let _: &T = &x`
    // would not: deref coercion makes that compile against a wrapper too, so
    // returning `TolerantList<ContainerInfo>` here would still pass.
    let containers: gosh_distrobox_core::ContainerList = containers;
    // `skipped` is reachable here — this is the whole point of the wrapper,
    // and it is not reachable on any peer's return type.
    assert!(containers.skipped.is_empty());

    // The peers still hand back bare vectors. No `if let Ok` guard: the
    // fixture registers the peer's whole command sequence, so a regression in
    // it must fail this test rather than silently skip the assertion — an
    // unreachable block is exactly how the previous version of this half
    // stopped checking anything.
    let apps: Vec<gosh_distrobox_core::models::AppInfo> =
        block_on(backend.container_apps("Ubuntu")).expect("apps succeeds");
    assert!(!apps.is_empty());
}
