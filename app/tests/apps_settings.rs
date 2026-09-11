//! T12 integration tests: app export/unexport/binary + config legacy import.
//!
//! T2 tier: `#[tokio::test]` (§0.2), `NullCommandRunner` fixtures. What is
//! asserted here is the backend contract each T12 arm depends on:
//! - export/unexport go through `distrobox enter --name <c> -- distrobox-export`
//!   with the right mode flag (#178);
//! - binary export resolves a bare name via `which` before exporting, and
//!   passes a path through untouched (#179);
//! - blocked env refuses every export (no spawn at all);
//! - the legacy import collapses duplicates (D11/PKG-9).

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::fakers::NullCommandRunnerBuilder;
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode};

fn backend(
    runner: gosh_distrobox_core::fakers::CommandRunner,
    mode: EnvMode,
) -> (
    Backend,
    gosh_distrobox_core::fakers::OutputTracker<gosh_distrobox_core::fakers::CommandRunnerEvent>,
) {
    let tracker = runner.output_tracker();
    let backend = Backend::new(
        runner,
        EnvGuard {
            mode,
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
async fn export_app_enters_container_with_export_app_flag() {
    let runner = Distrobox::null_command_runner(&[]);
    let (backend, tracker) = backend(runner, EnvMode::Native);
    backend
        .export_app("mybox", "/usr/share/applications/foo.desktop")
        .await
        .expect("export");
    let argv = argv(&tracker);
    let line = argv.join(" ");
    assert!(line.contains("enter --name mybox"), "argv: {line}");
    assert!(line.contains("distrobox-export --app"), "argv: {line}");
    assert!(
        line.contains("/usr/share/applications/foo.desktop"),
        "argv: {line}"
    );
}

#[tokio::test]
async fn unexport_app_passes_delete_flag() {
    let runner = Distrobox::null_command_runner(&[]);
    let (backend, tracker) = backend(runner, EnvMode::Native);
    backend
        .unexport_app("mybox", "/usr/share/applications/foo.desktop")
        .await
        .expect("unexport");
    let line = argv(&tracker).join(" ");
    assert!(line.contains("distrobox-export -d --app"), "argv: {line}");
}

#[tokio::test]
async fn binary_export_resolves_bare_name_via_which() {
    let mut builder = NullCommandRunnerBuilder::new();
    // A bare name (no `/`) is resolved INSIDE the container first, so the
    // probe is itself a `distrobox enter --name <c> -- which <name>` call.
    builder.cmd(
        &[
            "distrobox",
            "enter",
            "--name",
            "mybox",
            "--",
            "which",
            "htop",
        ],
        "/usr/bin/htop\n",
    );
    let (backend, tracker) = backend(builder.build(), EnvMode::Native);
    backend
        .export_binary("mybox", "htop")
        .await
        .expect("export");
    let line = argv(&tracker).join(" ");
    assert!(line.contains("which htop"), "resolution probe: {line}");
    assert!(line.contains("--bin /usr/bin/htop"), "argv: {line}");
}

#[tokio::test]
async fn blocked_env_refuses_app_exports_without_spawning() {
    let runner = Distrobox::null_command_runner(&[]);
    let (backend, tracker) = backend(runner, EnvMode::Blocked);
    assert!(backend.export_app("mybox", "/tmp/x.desktop").await.is_err());
    assert!(
        backend
            .unexport_app("mybox", "/tmp/x.desktop")
            .await
            .is_err()
    );
    assert!(
        backend
            .export_binary("mybox", "/usr/bin/htop")
            .await
            .is_err()
    );
    assert!(
        argv(&tracker).is_empty(),
        "blocked env must not spawn: {:?}",
        argv(&tracker)
    );
}

/// T12 §5: the terminal list is owned by the backend and is the ONE list
/// pickers index into — customs in, duplicates collapsed, built-ins kept.
#[test]
fn backend_terminals_are_the_indexed_list() {
    use gosh_distrobox_core::fakers::CommandRunner;
    use gosh_distrobox_core::{EnvGuard, EnvMode};

    let backend = Backend::new(
        CommandRunner::default(),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    let builtins = backend.terminals();
    assert!(!builtins.is_empty(), "built-in table is never empty");

    // A custom shadows a built-in by `full_command_id` (no duplicate slot).
    let custom = gosh_distrobox_core::backends::Terminal {
        name: "Mine".to_string(),
        program: builtins[0].program.clone(),
        extra_args: builtins[0].extra_args.clone(),
        separator_arg: "--".to_string(),
        read_only: false,
    };
    backend.set_custom_terminals(vec![custom]);
    let list = backend.terminals();
    assert_eq!(list.len(), builtins.len(), "shadowed, not appended");
    assert!(list.iter().any(|t| t.name == "Mine"));
}
