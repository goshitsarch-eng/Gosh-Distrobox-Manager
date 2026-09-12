//! Parity rows that could not be pinned before I31.
//!
//! **What this file is.** Until T16 the `app` package was binary-only: the module
//! tree lived in `main.rs` and was therefore private to the binary crate, so an
//! integration test could not import `App`, a page module or the view helpers.
//! That was I31 in `docs/migration/PLAN.md`, and it is why 148 of 193 parity rows
//! could only reach `source` tier (a call path read in the code) rather than T1.
//!
//! With `app/src/lib.rs` in place, the page model and the view helpers are
//! importable. Every assertion in this file was unreachable from a test before
//! that change, which is the point: it is the evidence that the ceiling moved.
//!
//! **Scope, stated plainly.** These pin the *pure* app-side surfaces — the nav
//! model, the title table, and the task row's affordance decision. They do **not**
//! construct `App`, which needs a `cosmic::app::Core` and a running executor; a
//! headless render harness is the larger half of I31's fix and remains open. No
//! assertion here inspects a rendered `Element`, because `Element` is opaque by
//! design — where a row's behaviour needed pinning, the decision was factored into
//! a pure function first (see `views::task_affordance`).

use gosh_distrobox_core::{EnvGuard, EnvMode};
use gosh_distrobox_manager::message::{AppMsg, BackupsMsg, ContainerMsg, Message, TaskMsg};
use gosh_distrobox_manager::views::{
    self, HeaderRefresh, Page, ReprobeOutcome, TaskAffordance, gate_header_message, header_refresh,
    reprobe_outcome,
};

// ------------------------------------------------------------- I26 / row #5

/// Row #5: the nav rail's destinations, in order.
///
/// I26 flagged that this list has **10** entries while `ux.md` §4.2 authorises
/// **8** — `Apps` and `Stats` are additions the port made, since `apps_page.dart`
/// was a pushed route in Flutter and no Flutter tree renders a "Stats"
/// destination at all. That is an open question about the chrome, not a bug, so
/// this test does not decide it. It pins the list so the question cannot be
/// closed by accident: any change here — adding a destination, dropping one,
/// reordering — must be a deliberate edit that updates this test.
///
/// The order assertion is not decoration. `activate_page` finds a page by
/// `Page::ALL.iter().position(...)` and feeds the index to
/// `nav_model.activate_position(pos as u16)`, so this array's order **is** the nav
/// model's index space; a silent reorder would activate the wrong tab.
#[test]
fn the_nav_rail_has_the_ten_destinations_i26_flagged() {
    assert_eq!(
        Page::ALL.len(),
        10,
        "ux.md §4.2 authorises 8 (see I26); 10 is the shipped count and the two \
         extras are the open question — pin it rather than let it drift"
    );
    assert_eq!(
        Page::ALL,
        [
            Page::Dashboard,
            Page::Containers,
            Page::Images,
            Page::Packages,
            Page::Updates,
            Page::Backups,
            Page::Activity,
            Page::Apps,
            Page::Settings,
            Page::Stats,
        ],
        "Page::ALL's order is the nav model's index space (activate_page uses \
         position()), so a reorder silently activates the wrong page"
    );
}

/// I26's finding made checkable: the eight Flutter destinations are still
/// present, and the two additions are exactly `Apps` and `Stats`.
///
/// This is the assertion that fails if someone "fixes" I26 by deleting a Flutter
/// destination rather than reconsidering the two additions.
#[test]
fn the_only_destinations_beyond_ux_md_are_apps_and_stats() {
    let flutter_eight = [
        Page::Dashboard,
        Page::Containers,
        Page::Images,
        Page::Packages,
        Page::Updates,
        Page::Backups,
        Page::Activity,
        Page::Settings,
    ];
    for page in flutter_eight {
        assert!(
            Page::ALL.contains(&page),
            "{page:?} is one of ux.md §4.2's authorised 8 and must not be dropped"
        );
    }
    let added: Vec<Page> = Page::ALL
        .iter()
        .copied()
        .filter(|p| !flutter_eight.contains(p))
        .collect();
    assert_eq!(
        added,
        vec![Page::Apps, Page::Stats],
        "the additions beyond ux.md §4.2 are the I26 question; a change here is \
         either a dropped Flutter destination (regression) or a decision on I26"
    );
}

/// Every destination has a distinct, non-empty title.
///
/// `Page::title()` is what the rail renders, so two pages sharing a title would
/// render two indistinguishable entries — a real user-visible bug that no test
/// covered while the enum was unreachable from a test crate.
#[test]
fn every_destination_has_a_distinct_non_empty_title() {
    let titles: Vec<String> = Page::ALL.iter().map(|p| p.title()).collect();
    for (page, title) in Page::ALL.iter().zip(&titles) {
        assert!(
            !title.trim().is_empty(),
            "{page:?} renders an empty nav label"
        );
    }
    let mut sorted = titles.clone();
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "two destinations share a title, so the rail shows two identical entries: \
         {titles:?}"
    );
}

// ---------------------------------------------------------------- row #20/#21

/// Row #21: a failed task must not render a success check.
///
/// `TaskMsg::Completed` carries `success` rather than a string the view sniffs
/// (§3.1), so the split is available without parsing. Pinned on the *message*
/// type, which the binary could never expose to a test.
#[test]
fn a_task_completion_carries_its_success_flag_rather_than_implying_it() {
    let id = gosh_distrobox_core::TaskId::new();
    let ok = TaskMsg::Completed { id, success: true };
    let failed = TaskMsg::Completed { id, success: false };
    match (&ok, &failed) {
        (TaskMsg::Completed { success: a, .. }, TaskMsg::Completed { success: b, .. }) => {
            assert!(*a, "a successful completion must carry success: true");
            assert!(!*b, "a failed completion must carry success: false");
        }
        _ => unreachable!("both constructed as Completed above"),
    }
    assert_ne!(
        format!("{ok:?}"),
        format!("{failed:?}"),
        "the two completions must be distinguishable to the view without \
         string-sniffing (row #21)"
    );
}

/// Rows #20/#21: the trailing-affordance decision, for all four flag
/// combinations.
///
/// The `(false, _)` arms resolve to `Running` **for both values of `success`**,
/// because while a task is running the success flag is not yet meaningful and
/// must not influence the render. The variant used to be `RunningCancelOnly` —
/// the name filed the #20 gap (no spinner) in the type itself — and T18's
/// rename to `Running` is what forced this edit, as designed.
#[test]
fn the_task_row_affordance_covers_all_four_flag_combinations() {
    assert_eq!(
        views::task_affordance(true, true),
        TaskAffordance::Succeeded,
        "a completed, successful task shows a check"
    );
    assert_eq!(
        views::task_affordance(true, false),
        TaskAffordance::Failed,
        "row #21: a completed, failed task must never resolve to a check"
    );
    // The #20 arms: running renders spinner + "In progress…" + Cancel.
    assert_eq!(
        views::task_affordance(false, true),
        TaskAffordance::Running,
        "running resolves to the running affordance regardless of `success`"
    );
    assert_eq!(
        views::task_affordance(false, false),
        TaskAffordance::Running,
        "`success` must not influence a running row: it is not yet meaningful"
    );
}

/// The running variant is not a `Succeeded`/`Failed`.
///
/// Trivial on its face, but it is the assertion that would catch a refactor
/// collapsing the three variants into a bool pair — which would silently delete
/// #20's only recorded evidence in the type system.
#[test]
fn a_running_row_is_distinguishable_from_a_finished_one() {
    let running = views::task_affordance(false, false);
    assert_ne!(running, TaskAffordance::Succeeded);
    assert_ne!(running, TaskAffordance::Failed);
    assert_eq!(
        running,
        views::task_affordance(false, true),
        "both running cases are the same affordance, so the render branch cannot \
         depend on `success`"
    );
}

// ------------------------------------------------------------ T17 / row #97

/// Row #97: the Images page's header Refresh reloads the catalogue, not the
/// container list.
///
/// I29 filed this as `bug`: Refresh was the *global containers* refresh on
/// every page, so `backend.images()` could never be re-fetched after its
/// single lazy load. `header_start` now routes through `header_refresh`;
/// this pins all ten pages, so Images cannot silently fall back to the
/// containers reload (the exact regression) and no other page can lose its
/// own.
#[test]
fn the_images_header_refresh_reloads_images_not_containers() {
    assert_eq!(
        header_refresh(Page::Images),
        Some(HeaderRefresh::Images),
        "row #97: the Images Refresh must re-fetch the catalogue via \
         `ImageMsg::LoadRequested`, not the container list"
    );
    for page in [
        Page::Dashboard,
        Page::Containers,
        Page::Packages,
        Page::Backups,
        Page::Activity,
        Page::Settings,
        Page::Stats,
    ] {
        assert_eq!(
            header_refresh(page),
            Some(HeaderRefresh::Containers),
            "{page:?} keeps the generic containers refresh"
        );
    }
    for page in [Page::Updates, Page::Apps] {
        assert_eq!(
            header_refresh(page),
            None,
            "{page:?} owns its own header actions, so the generic button hides"
        );
    }
}

// ------------------------------------------------------------ T17 / row #13

fn guard(mode: EnvMode, installed: bool, message: Option<&str>) -> EnvGuard {
    EnvGuard {
        mode,
        message: message.map(str::to_string),
        distrobox_installed: installed,
    }
}

/// Row #13: the `Probed` arm's gate-state transition.
///
/// "Check Again" re-probes the environment; this pins what the arm owes the
/// user for each guard. A mid-session install (`Native` + installed) is the
/// recovery the row names — the arm clears the banner and reloads. A still-
/// missing distrobox stays gated with no banner (the gate page itself is the
/// message); `Blocked` stays gated WITH the Dart guard's wording, same as
/// `init`.
#[test]
fn a_reprobe_that_finds_distrobox_recovers_and_one_that_does_not_stays_gated() {
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::Native, true, None)),
        ReprobeOutcome::Recovered,
        "row #13: a mid-session install must clear the gate on Check Again"
    );
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::FlatpakHost, true, None)),
        ReprobeOutcome::Recovered,
        "recovery is not Native-only: any installed mode recovers"
    );
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::DistroboxHostExec, true, None)),
        ReprobeOutcome::Recovered,
        "recovery is not Native-only: any installed mode recovers"
    );
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::Native, false, None)),
        ReprobeOutcome::StillGated { error: None },
        "a still-missing distrobox stays on the gate page with no banner"
    );
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::FlatpakHost, false, None)),
        ReprobeOutcome::StillGated { error: None },
        "installed-ness, not mode, decides recovery outside Blocked"
    );
}

/// `Blocked` dominates `distrobox_installed`: the guard's message is the
/// banner, exactly as `init` shows it. (A `Blocked` + installed pair cannot
/// arise from `detect` — `detect` reports `false` there — so pinning the
/// precedence, not just the reachable pair, is what makes a future
/// reorder fail.)
#[test]
fn a_blocked_reprobe_keeps_the_guard_message_as_the_banner() {
    let message = "Detected a Distrobox environment, but `distrobox-host-exec` \
         is not available.";
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::Blocked, false, Some(message))),
        ReprobeOutcome::StillGated {
            error: Some(message.to_string())
        },
        "Blocked must surface the guard's own wording, not a generic error"
    );
    assert_eq!(
        reprobe_outcome(&guard(EnvMode::Blocked, true, Some(message))),
        ReprobeOutcome::StillGated {
            error: Some(message.to_string())
        },
        "Blocked dominates: even a stale installed=true must not recover"
    );
}

// ------------------------------------------------------------ T18 / row #134

/// Row #134: header actions that need a working backend are gated in the
/// blocked and not-installed states; the recovery and local actions stay live.
///
/// I29 filed this as `bug`: the shell header renders even while the gate
/// replaces the page body, so Backups showed "New Snapshot" in every state
/// and the press could only toast. `header_end` now routes every action
/// through `gate_header_message` (`on_press_maybe`), so this table IS the
/// header's enabled matrix — one entry per header button, pinned in both
/// gated states and the healthy state.
#[test]
fn gated_header_actions_disable_while_recovery_and_local_actions_stay_live() {
    use ContainerMsg::{NewContainerRequested, RefreshRequested, UpgradeAllRequested};
    // (message, gated-in-blocked/not-installed-states?)
    let table: Vec<(Message, bool)> = vec![
        (Message::Containers(NewContainerRequested), true),
        (Message::Containers(UpgradeAllRequested), true),
        (Message::Backups(BackupsMsg::CreateDialogRequested), true),
        (Message::Apps(AppMsg::ReloadRequested("box".into())), true),
        // Refresh re-probes from a gated state (T17): the recovery path.
        (Message::Containers(RefreshRequested), false),
        // Clear Completed touches only the local task mirror.
        (Message::Tasks(TaskMsg::ClearCompleted), false),
    ];
    for (msg, gated) in &table {
        for (blocked, installed, state) in
            [(true, true, "blocked"), (false, false, "not-installed")]
        {
            let verdict = gate_header_message(msg.clone(), blocked, installed);
            if *gated {
                assert!(
                    verdict.is_none(),
                    "{msg:?} must disable in the {state} state: it cannot act there"
                );
            } else {
                assert!(
                    verdict.is_some(),
                    "{msg:?} must stay live in the {state} state: it is the recovery/local path"
                );
            }
        }
        assert!(
            gate_header_message(msg.clone(), false, true).is_some(),
            "{msg:?} must stay live in the healthy state"
        );
    }
}

/// The gate constrains header actions only: any other message passes through
/// unchanged, so a future header button cannot be silently swallowed by an
/// over-broad match.
#[test]
fn the_header_gate_passes_non_header_messages_through() {
    let id = gosh_distrobox_core::TaskId::new();
    let msg = Message::Tasks(TaskMsg::CancelRequested(id));
    assert!(
        gate_header_message(msg, true, false).is_some(),
        "non-header messages are outside the gate's contract"
    );
}
