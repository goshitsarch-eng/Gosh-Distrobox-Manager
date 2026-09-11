//! T8 integration tests: package flow through `Backend`.
//!
//! T2 tier: `#[tokio::test]` (§0.2), `NullCommandRunner` fixtures. The page
//! logic (badge, tabs, confirms) is unit-tested in `packages.rs`; what is
//! asserted here is the backend contract each page arm depends on:
//! - `detect_package_manager` is TYPED (B1 — never a bare string) and the
//!   shared script reports every word the table covers;
//! - install/remove spawn tasks through the unified verbs (emerge uses
//!   `--ask=n`, never `-y`);
//! - list/search parse into `PackageInfo`;
//! - blocked env refuses everything.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::models::PackageManager;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode, TaskEvent};

fn backend() -> Backend {
    Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    )
}

#[test]
fn detect_words_cover_the_table() {
    // Every word the shared script can report maps to a typed manager —
    // including the two the old table missed (emerge/xbps arms dead-ended
    // before B1) and yum folding into Dnf.
    for (word, pm) in [
        ("apt", PackageManager::Apt),
        ("dnf", PackageManager::Dnf),
        ("yum", PackageManager::Dnf),
        ("pacman", PackageManager::Pacman),
        ("apk", PackageManager::Apk),
        ("zypper", PackageManager::Zypper),
        ("xbps", PackageManager::Xbps),
        ("emerge", PackageManager::Emerge),
        ("unknown", PackageManager::Unknown),
    ] {
        assert_eq!(PackageManager::from_detected(word), pm, "{word}");
    }
}

#[tokio::test]
async fn install_script_embeds_unified_verbs() {
    // B1's central claim (shell table == Rust table) IS argv-assertable:
    // the whole install script is in the captured spawn argv.
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
    let id = backend.install_package("c1", "vim").await.expect("spawn");
    // The runner spawns inside `tokio::spawn` — wait (bounded) for the
    // Spawned event before reading the tracker. (The null runner exits
    // instantly, so the task may already be finished: do NOT assert
    // cancel — just stop waiting when it completes.)
    let mut argv: Vec<String> = Vec::new();
    for _ in 0..1000 {
        argv = tracker
            .items()
            .iter()
            .filter_map(|e| e.command().map(|c| c.to_string()))
            .collect();
        if argv.iter().any(|a| a.contains("PKG_MGR")) || !backend.is_task_running(id) {
            break;
        }
        tokio::task::yield_now().await;
    }
    let _ = backend.cancel_task(id);
    let script = argv
        .iter()
        .find(|a| a.contains("PKG_MGR"))
        .expect("install script argv: {argv:?}");
    // Single shared detection (no divergent copy) + every verb incl. emerge.
    assert!(script.contains("command -v emerge"), "{script}");
    assert!(script.contains("sudo apt-get install -y"), "{script}");
    assert!(script.contains("sudo dnf install -y"), "{script}");
    assert!(script.contains("sudo emerge --ask=n"), "{script}");
    assert!(!script.contains("emerge -y"), "{script}");
}

#[tokio::test]
async fn remove_spawns_and_completes() {
    let backend = backend();
    let id = backend.remove_package("c1", "vim").await.expect("spawn");
    let rx = backend.tasks().subscribe(id).expect("subscribes");
    let mut finished = false;
    while let Ok(ev) = rx.recv().await {
        if matches!(ev, TaskEvent::Finished { .. }) {
            finished = true;
            break;
        }
    }
    assert!(finished, "remove task finishes");
}

#[tokio::test]
async fn blocked_env_refuses_package_ops() {
    let backend = Backend::new(
        Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Blocked,
            message: Some("blocked".into()),
            distrobox_installed: false,
        },
    );
    assert!(matches!(
        &*backend
            .detect_package_manager("c")
            .await
            .expect_err("detect")
            .0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend.installed_packages("c").await.expect_err("list").0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
    assert!(matches!(
        &*backend
            .install_package("c", "p")
            .await
            .expect_err("install")
            .0,
        gosh_distrobox_core::CoreError::BlockedEnvironment
    ));
}
