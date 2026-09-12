//! I31 render-harness half (T20): headless `App` construction + executor.
//!
//! **What this file is.** Until T20 no test could construct `App`: it needs a
//! `cosmic::app::Core` and a running executor, and both were assumed to need a
//! window and a compositor. Neither does. `headless_app` builds a real `App`
//! from `Core::default()` with no display, and every seed test drives it the
//! way the runtime would — `update` for state, `view` (plus the header,
//! drawer and dialog renders) for the widgets, `drain_app_messages` for the
//! follow-up tasks — under `#[tokio::test]`.
//!
//! **What is deliberately NOT claimed.** The rendered `Element` is opaque at
//! the pinned rev: no `Debug`, no introspection API (verified in the vendored
//! `iced` sources — `element.rs` has no `fmt` impl at all). So no assertion
//! here inspects a widget tree, and no pixel claim is made. Each seed row pins
//! the state transition ([`HarnessSnapshot`], the I31 seam on `App` — the same
//! precedent as `DashboardCounts` and `task_affordance`: factor the decision
//! into something a test can reach) *alongside* the headless render: the
//! render proves the row's widgets build over the row's state (render code
//! that panics on missing data fails here, not in a click-through), the
//! snapshot proves the state is the row's. Rows whose subject is the rendered
//! pixels still need T3, and no tier count moves without its test.
//!
//! **The drain rule.** `drain_app_messages` polls a returned `Task` to
//! completion via `iced::runtime::task::into_stream` — the same
//! produce→poll→feed-back loop the real runtime runs, minus the window. Only
//! tasks the producing arm guarantees to be done/effect-shaped (immediate
//! values: `Task::done`, `Task::batch` of those, `clipboard::write`-style
//! effects) may be drained. Future/stream tasks must NEVER be drained: polling
//! runs the future, so a toast task would sleep out its full duration
//! (`toaster::push` is `sleep(duration)` then `on_close`) and a backend task
//! would execute real commands against the live environment. Those are dropped
//! (`let _task`) with a comment at the call site. Every drain carries a 10 s
//! timeout so a rule violation fails loudly instead of hanging the suite.
//!
//! **Config isolation.** `init` reads and the #161 arms write through
//! cosmic-config, which resolves `dirs::config_dir()` (`$XDG_CONFIG_HOME`) at
//! every call. Without isolation the harness would read the developer's real
//! config and the round-trip test would WRITE it. Every test therefore takes
//! `ENV_LOCK` for its whole body (env is process-global; the lock makes the
//! redirect race-free) and points `$XDG_CONFIG_HOME` at a fresh per-test dir,
//! so each `init` starts from a known-empty config.

use cosmic::app::Application as _;
use gosh_distrobox_core::{EnvGuard, EnvMode, TaskId};
use gosh_distrobox_manager::activity::ActivityFilter;
use gosh_distrobox_manager::app::{App, TaskKind};
use gosh_distrobox_manager::message::{ActivityMsg, DetailsMsg, Message, TaskMsg};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Serialises every test in this binary: env is process-global, and both the
/// redirect and all config IO must be atomic against each other. Held for the
/// whole test body (returned guard), never across tests. A tokio mutex (not
/// std): the guard is held across `drain_app_messages().await`, and parking
/// an executor thread on a std mutex there is exactly what
/// `await_holding_lock` forbids.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Fresh isolated config dir + the lock guard that keeps it ours. Removes any
/// previous run's dir first, so a stale `task_history` can never leak across
/// runs and seed a test that expects an empty mirror.
async fn fresh_config_dir(test: &str) -> (PathBuf, tokio::sync::MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().await;
    let dir = std::env::temp_dir().join(format!("gdm-t20-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("harness config dir is writable");
    // SAFETY: `ENV_LOCK` serialises all env mutation and all config IO in
    // this binary, and no other thread in this process touches these vars
    // (every test here goes through this function). Set-then-read is therefore
    // atomic by construction, and the values are only ever read back by
    // `dirs`/`cosmic-config` path resolution.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        // cosmic-config prefers `HOST_XDG_CONFIG_HOME` when set — a Flatpak'd
        // `cargo test` would otherwise escape the isolation.
        std::env::remove_var("HOST_XDG_CONFIG_HOME");
    }
    (dir, guard)
}

/// Build a headless `App` over an already-isolated config dir: no window, no
/// compositor, no display server — just `Core::default()` and `init`. The
/// init task is dropped undrained: it holds backend futures (container list,
/// version probe) that would run real commands if polled.
fn headless_app(config_dir: &Path) -> App {
    assert!(
        std::env::var_os("XDG_CONFIG_HOME").as_deref() == Some(config_dir.as_os_str()),
        "headless_app requires fresh_config_dir first (config isolation)"
    );
    let core = cosmic::app::Core::default();
    let (app, _init_task) = App::init(core, ());
    app
}

/// Poll a returned `Task` to completion, recovering re-dispatchable follow-up
/// `App` messages. Only for done/effect-shaped tasks (see the module docs) —
/// the timeout turns a rule violation into a loud failure, not a hung suite.
async fn drain_app_messages(task: cosmic::app::Task<Message>) -> Vec<Message> {
    use futures::StreamExt as _;
    let mut out = Vec::new();
    let Some(mut stream) = cosmic::iced::runtime::task::into_stream(task) else {
        return out;
    };
    loop {
        let next = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
        match next {
            Ok(Some(action)) => {
                // `Output(App(m))` is the only re-dispatchable shape: effects
                // (clipboard, window, …) have no runtime headless to interpret
                // them, so they are polled (a panicking stream fails here) and
                // dropped. `Task::done` follow-ups are recovered below.
                if let cosmic::iced::runtime::Action::Output(cosmic::Action::App(m)) = action {
                    out.push(m);
                }
            }
            Ok(None) => break,
            Err(_) => panic!(
                "drain timed out: the task contains a future/stream (toast sleep? \
                 backend call?) and must not be drained — see the module docs"
            ),
        }
    }
    out
}

/// The full render surface, headless: page body, both header halves, the
/// context drawer and the dialog slot. Each builds an `Element` tree over the
/// driven state; a render path that panics on missing data fails here.
fn render_everything(app: &App) {
    let _body = app.view();
    let _header_start = app.header_start();
    let _header_end = app.header_end();
    let _drawer = app.context_drawer();
    let _dialog = app.dialog();
    let _subscriptions = app.subscription();
}

/// Drive a task through Started → Output → Completed, returning the
/// completion's task for the caller to drain-or-drop per the drain rule.
fn finish_task(app: &mut App, id: TaskId, label: &str, lines: &[&str], success: bool) {
    let _started = app.update(Message::Tasks(TaskMsg::Started {
        label: label.to_string(),
        kind: TaskKind::Other,
        result: Ok(id),
    }));
    let _output = app.update(Message::Tasks(TaskMsg::Output {
        id,
        lines: lines.iter().map(|s| s.to_string()).collect(),
    }));
    let completion = app.update(Message::Tasks(TaskMsg::Completed { id, success }));
    // Success completions return `none()`; failure completions return a toast
    // task (a sleep future — dropped per the drain rule, never drained).
    let _completion_task = completion;
}

// ------------------------------------------------------- construction smoke

/// The harness foundation: a headless `App` builds and its whole render
/// surface runs with no display. The snapshot pins the empty start state (the
/// isolated config holds no history, so nothing seeds the mirror).
#[tokio::test]
async fn headless_init_builds_an_app_that_renders() {
    let (dir, _guard) = fresh_config_dir("init-renders").await;
    let app = headless_app(&dir);
    let snap = app.harness_snapshot();
    assert_eq!(snap.tasks_total, 0, "fresh config seeds no tasks");
    assert_eq!(snap.history_len, Some(0), "fresh config holds no ring");
    assert!(snap.task_labels.is_empty());
    // `has_error` is environment-dependent (blocked gate vs healthy host), so
    // the smoke test reads it without asserting a value.
    let _ = snap.has_error;
    render_everything(&app);
}

// ------------------------------------------------- executor half (I31 drain)

/// The executor half: a real arm's done-shaped task polls to completion under
/// the running tokio runtime and its follow-up message re-enters `update`.
///
/// Vehicle is row #57 (`CopyImageRequested` → batch of a clipboard effect +
/// `done(CopiedToClipboard)`): command-free and sleep-free, so the whole task
/// polls immediately. The effect is filtered (no runtime headless to interpret
/// it) and exactly the one `App` message is recovered — the assertion that
/// fails if the drain misroutes, drops, or invents messages.
#[tokio::test]
async fn done_tasks_drain_to_followup_messages_under_a_running_executor() {
    let (dir, _guard) = fresh_config_dir("executor-drain").await;
    let mut app = headless_app(&dir);
    let task = app.update(Message::Details(DetailsMsg::CopyImageRequested(
        "docker.io/library/ubuntu:latest".to_string(),
    )));
    let followups = drain_app_messages(task).await;
    assert_eq!(
        followups.len(),
        1,
        "exactly the done follow-up is recovered (the clipboard effect is \
         polled and dropped — it has no runtime headless to interpret it)"
    );
    match &followups[0] {
        Message::Ui(gosh_distrobox_manager::message::UiMsg::CopiedToClipboard(what)) => {
            assert_eq!(what, "docker.io/library/ubuntu:latest");
        }
        other => panic!("wrong follow-up recovered: {other:?}"),
    }
    // Feed back: the clipboard arm answers with a toast task (a sleep future
    // — dropped per the drain rule, never drained).
    let _toast = app.update(followups.into_iter().next().unwrap());
    render_everything(&app);
}

// ------------------------------------------------- seed row: #161 + #21

/// Seed row #161: a finished task enters the persisted ring AND the mirror,
/// and the render surface builds over both.
#[tokio::test]
async fn a_finished_task_is_recorded_in_history_and_renders() {
    let (dir, _guard) = fresh_config_dir("history-records").await;
    let mut app = headless_app(&dir);
    let id = TaskId::new();
    finish_task(
        &mut app,
        id,
        "Upgrade demo",
        &["Reading package lists", "Task completed successfully"],
        true,
    );
    let snap = app.harness_snapshot();
    assert_eq!(snap.tasks_total, 1);
    assert_eq!(
        snap.tasks_success, 1,
        "row #21: success is the flag, not a sniff"
    );
    assert_eq!(snap.task_labels, vec!["Upgrade demo".to_string()]);
    assert_eq!(
        snap.history_len,
        Some(1),
        "row #161: the completion entered the persisted ring"
    );
    render_everything(&app);
    // The write half of the round trip: a re-read sees the same ring (the
    // full restart — fresh `App` over this dir — is the T2 below).
    let (cfg, _) =
        gosh_distrobox_manager::settings::load_entry().expect("isolated config re-reads");
    assert_eq!(cfg.task_history.len(), 1);
    assert_eq!(cfg.task_history[0].label, "Upgrade demo");
    assert!(cfg.task_history[0].success);
}

/// Seed row #21: a failed completion marks failed (never a success check) and
/// renders. The harness migration of the pure affordance pin: same outcome,
/// now through a real `update` + headless `view` instead of the helper.
#[tokio::test]
async fn a_failed_completion_marks_failed_and_renders() {
    let (dir, _guard) = fresh_config_dir("failed-marks").await;
    let mut app = headless_app(&dir);
    finish_task(&mut app, TaskId::new(), "Install flop", &["E: boom"], false);
    let snap = app.harness_snapshot();
    assert_eq!(snap.tasks_total, 1);
    assert_eq!(
        snap.tasks_failed, 1,
        "row #21: failed must never read as success"
    );
    assert_eq!(snap.tasks_success, 0);
    assert_eq!(snap.history_len, Some(1), "failures persist like successes");
    render_everything(&app);
}

// ------------------------------------------------- seed row: #152

/// Seed row #152: Clear Completed empties the mirror AND the persisted ring —
/// without the ring half the button would lie (entries resurrect on restart)
/// — and the cleared page renders.
#[tokio::test]
async fn clear_completed_clears_mirror_and_persisted_history() {
    let (dir, _guard) = fresh_config_dir("clear-completed").await;
    let mut app = headless_app(&dir);
    finish_task(&mut app, TaskId::new(), "Upgrade one", &["ok"], true);
    finish_task(&mut app, TaskId::new(), "Upgrade two", &["E: nope"], false);
    assert_eq!(app.harness_snapshot().history_len, Some(2));
    // Direct `Tasks` arm (the `Settings` route re-dispatches through it; the
    // re-dispatch loop itself is pinned by the executor test above).
    let _clear = app.update(Message::Tasks(TaskMsg::ClearCompleted));
    let snap = app.harness_snapshot();
    assert_eq!(snap.tasks_total, 0, "the mirror drops finished tasks");
    assert_eq!(
        snap.history_len,
        Some(0),
        "row #161: the ring clears with the mirror, or cleared entries resurrect"
    );
    render_everything(&app);
    let (cfg, _) =
        gosh_distrobox_manager::settings::load_entry().expect("isolated config re-reads");
    assert!(
        cfg.task_history.is_empty(),
        "the clear reached the disk key"
    );
}

// ------------------------------------------------- seed rows: #153 + #154

/// Seed rows #153/#154: search + filter drive the Activity list and the
/// filtered page renders. The snapshot pins the driven state; the render call
/// exercises the filtered-list code path headless (empty-match and populated
/// branches across the two phases below).
#[tokio::test]
async fn activity_search_and_filter_drive_the_rendered_list() {
    let (dir, _guard) = fresh_config_dir("search-filter").await;
    let mut app = headless_app(&dir);
    finish_task(&mut app, TaskId::new(), "Upgrade demo", &["ok"], true);
    finish_task(&mut app, TaskId::new(), "Install flop", &["E: boom"], false);
    // Phase 1: a search matching one row + the Success filter.
    let _search = app.update(Message::Activity(ActivityMsg::SearchChanged(
        "upgrade".to_string(),
    )));
    let _filter = app.update(Message::Activity(ActivityMsg::FilterSelected(
        ActivityFilter::Success,
    )));
    let snap = app.harness_snapshot();
    assert_eq!(
        snap.activity_search, "upgrade",
        "row #153: search state drives"
    );
    assert_eq!(
        snap.activity_filter,
        ActivityFilter::Success,
        "row #154: filter state drives"
    );
    render_everything(&app);
    // Phase 2: back to unfiltered — the populated-list branch renders.
    let _search = app.update(Message::Activity(ActivityMsg::SearchChanged(String::new())));
    let _filter = app.update(Message::Activity(ActivityMsg::FilterSelected(
        ActivityFilter::All,
    )));
    assert_eq!(app.harness_snapshot().activity_filter, ActivityFilter::All);
    render_everything(&app);
}

// ------------------------------------------------- T2: restart round trip

/// Drain a registry subscription until `Finished`, collecting output lines.
/// Same contract as `task_lifecycle.rs`'s drain: the sender drops at finish.
async fn drain_to_finished(
    backend: &gosh_distrobox_core::Backend,
    id: TaskId,
) -> (bool, Vec<String>) {
    use gosh_distrobox_core::TaskEvent;
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
    (success, lines)
}

/// T2 — persistence round trip (row #161): a NullCommandRunner-backed task
/// runs to completion, its finish persists through the real config path, and
/// a FRESH `App` over the same config dir ("restart") shows it in Activity.
///
/// Three phases, each asserting its own leg: (1) the null-backed spawn really
/// finishes (not a fabricated completion); (2) driving the resulting
/// `Completed` through `update` writes the ring to disk; (3) the restart
/// seeds the mirror from disk — labels, counts and the render surface.
#[tokio::test]
async fn history_survives_a_restart_round_trip() {
    // Phase 1: real spawn over NullCommandRunner, drained to `Finished`.
    let backend = gosh_distrobox_core::Backend::new(
        gosh_distrobox_core::backends::Distrobox::null_command_runner(&[]),
        EnvGuard {
            mode: EnvMode::Native,
            message: None,
            distrobox_installed: true,
        },
    );
    let registry_id = backend
        .upgrade_container("mybox")
        .await
        .expect("null-backed spawn succeeds");
    let (success, lines) = drain_to_finished(&backend, registry_id).await;
    assert!(success, "the null-runner task really finishes: {lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Upgrade completed successfully")),
        "upgrade success line streamed: {lines:?}"
    );

    // Phase 2: the app's mirror + ring learn about the finish through the
    // same messages the subscription delivers (the registry id rides along,
    // exactly as in production).
    let (dir, _guard) = fresh_config_dir("restart-round-trip").await;
    {
        let mut app = headless_app(&dir);
        let _started = app.update(Message::Tasks(TaskMsg::Started {
            label: "Upgrade mybox".to_string(),
            kind: TaskKind::Upgrade,
            result: Ok(registry_id),
        }));
        let _output = app.update(Message::Tasks(TaskMsg::Output {
            id: registry_id,
            lines,
        }));
        let _completed = app.update(Message::Tasks(TaskMsg::Completed {
            id: registry_id,
            success,
        }));
        // Dropped with the guard held: the phase-3 `App` re-reads this dir.
        let snap = app.harness_snapshot();
        assert_eq!(snap.history_len, Some(1), "the finish reached the ring");
    }

    // Phase 3: "restart" — a fresh `App` over the same config dir seeds its
    // mirror from disk, and Activity shows it.
    {
        let app = headless_app(&dir);
        let snap = app.harness_snapshot();
        assert_eq!(snap.tasks_total, 1, "the restart reseeds the mirror");
        assert_eq!(snap.tasks_success, 1);
        assert_eq!(snap.task_labels, vec!["Upgrade mybox".to_string()]);
        assert_eq!(snap.history_len, Some(1));
        render_everything(&app);
    }
}
