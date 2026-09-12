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

use gosh_distrobox_core::models::{ContainerInfo, Status};
use gosh_distrobox_core::{EnvGuard, EnvMode};
use gosh_distrobox_manager::message::{
    AppMsg, BackupsMsg, CardMenuAction, ContainerMsg, Message, Shortcut, TaskMsg,
};
use gosh_distrobox_manager::views::{
    self, HeaderRefresh, IconControl, MenuRowState, Page, ReprobeOutcome, TaskAffordance,
    card_actions, card_menu_message, card_menu_row, gate_header_message, header_refresh,
    page_search_id, reprobe_outcome, shortcut_for,
};

// ------------------------------------------------------------- I26 / row #5

/// Row #5: the nav rail's destinations, in order.
///
/// I26 flagged that this list had **10** entries while `ux.md` §4.2 authorises
/// **8**. D28 ruled the split: `Apps` returned to a details-pushed route (as in
/// Flutter) and `Stats` stayed as the single deliberate addition, so the rail
/// is **9**. This pins the ruled set so the question cannot be re-closed by
/// accident: any change here — adding a destination, dropping one,
/// reordering — must be a deliberate edit that updates this test.
///
/// The order assertion is not decoration. `activate_page` finds a page by
/// `Page::ALL.iter().position(...)` and feeds the index to
/// `nav_model.activate_position(pos as u16)`, so this array's order **is** the nav
/// model's index space; a silent reorder would activate the wrong tab.
#[test]
fn the_nav_rail_has_the_ruled_nine_destinations_in_order() {
    assert_eq!(
        Page::ALL.len(),
        9,
        "D28 ruled 8 ux.md destinations + Stats; Apps is a pushed route, not a \
         rail entry — pin the ruled count rather than let it drift"
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
            Page::Settings,
            Page::Stats,
        ],
        "Page::ALL's order is the nav model's index space (activate_page uses \
         position()), so a reorder silently activates the wrong page"
    );
}

/// I26, decided by D28: the eight Flutter destinations are still present,
/// the only addition is `Stats`, and `Apps` is absent from the rail.
///
/// The first half fails if someone "fixes" the rail by deleting a Flutter
/// destination rather than ruling on the additions; the second half fails if
/// the demotion silently reverts (Apps re-added to `ALL`).
#[test]
fn the_only_destination_beyond_ux_md_is_stats_and_apps_is_not_on_the_rail() {
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
        vec![Page::Stats],
        "D28 keeps Stats as the single deliberate addition; anything else here \
         is either a dropped Flutter destination (regression) or an unruled \
         chrome change"
    );
    assert!(
        !Page::ALL.contains(&Page::Apps),
        "D28 demoted Apps to a details-pushed route: it must not be a rail \
         destination again without a new ruling"
    );
}

/// Row #5: every rail destination has a distinct, well-formed icon name.
///
/// `Page::icon()` is what `init` chains into `nav_model.insert().icon()`,
/// so an empty name renders no glyph and a duplicated name renders two
/// indistinguishable entries. The `-symbolic` suffix is the contract with
/// the icon theme (all ten names verified against pop-icon-theme 3.5.0 in
/// T19; Stats is Pop-only, noted at the table).
#[test]
fn every_rail_destination_has_a_distinct_symbolic_icon() {
    let icons: Vec<&str> = Page::ALL.iter().map(|p| p.icon()).collect();
    for (page, icon) in Page::ALL.iter().zip(&icons) {
        assert!(
            !icon.trim().is_empty(),
            "{page:?} has no nav icon — the rail renders it text-only (row #5)"
        );
        assert!(
            icon.ends_with("-symbolic"),
            "{page:?}'s icon {icon:?} is not a symbolic theme name"
        );
        assert!(
            !icon.contains(' '),
            "{page:?}'s icon {icon:?} is not a single theme token"
        );
    }
    let mut sorted = icons.clone();
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "two destinations share a nav icon, so the rail shows two identical \
         glyphs: {icons:?}"
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
/// this pins every page, so Images cannot silently fall back to the
/// containers reload (the exact regression) and no other page can lose its
/// own. (`Page::Apps` stays in the table: the pushed Apps view owns its
/// actions exactly as the tab did.)
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

// ------------------------------------------------------------ T19 / row #40

/// Row #40: the card's inline actions are running-only.
///
/// Both Stop (#25) and "Open Terminal" (#40, mirroring the details-page
/// control and its gate) render only while the container runs; a stopped
/// card offers neither. `container_row` renders from this decision, so a
/// gate deleted here breaks the card the test cannot otherwise see.
#[test]
fn the_card_offers_stop_and_terminal_only_while_running() {
    let running = card_actions(&Status::Up("2 hours".into()));
    assert!(
        running.stop && running.terminal,
        "a running card must offer both Stop and Open Terminal, got {running:?}"
    );
    for status in [
        Status::Created("".into()),
        Status::Exited("0".into()),
        Status::Other("paused".into()),
    ] {
        let actions = card_actions(&status);
        assert!(
            !actions.stop && !actions.terminal,
            "{status:?} must offer no inline actions, got {actions:?}"
        );
    }
}

// ------------------------------------------------------------ T19 / row #41

/// Row #41 → §6.4 rows #47–#51: the card-menu row table, both states.
///
/// Details/Upgrade/Delete are unconditional; Open Terminal disables when
/// stopped (Flutter #48 verbatim); Stop hides when stopped (Flutter #49 is
/// running-only). The drawer renders from this table, so a row lost here
/// is a row lost in the UI.
#[test]
fn the_card_menu_table_gates_terminal_and_stop_on_running() {
    use CardMenuAction::{Delete, Details, OpenTerminal, Stop, Upgrade};
    for action in [Details, Upgrade, Delete] {
        assert_eq!(
            card_menu_row(action, true),
            MenuRowState::Active,
            "{action:?} must always be pressable"
        );
        assert_eq!(
            card_menu_row(action, false),
            MenuRowState::Active,
            "{action:?} must stay pressable when stopped"
        );
    }
    assert_eq!(
        card_menu_row(OpenTerminal, true),
        MenuRowState::Active,
        "row #48: Open Terminal is offered when running"
    );
    assert_eq!(
        card_menu_row(OpenTerminal, false),
        MenuRowState::Disabled,
        "row #48: Open Terminal disables (stays visible) when stopped"
    );
    assert_eq!(
        card_menu_row(Stop, true),
        MenuRowState::Active,
        "row #49: Stop is offered when running"
    );
    assert_eq!(
        card_menu_row(Stop, false),
        MenuRowState::Hidden,
        "row #49: Stop hides when stopped — it is running-only"
    );
}

/// Row #41 dispatch: every menu row reuses an existing arm.
///
/// The drawer adds triggers, not behaviour — so each row's message must be
/// one an existing arm already handles. Delete especially must route
/// through the shared destructive confirm (`RemoveRequested`), never a
/// direct run.
#[test]
fn every_card_menu_row_redispatches_through_an_existing_arm() {
    use gosh_distrobox_manager::message::{DetailsMsg, TerminalMsg};
    let container = ContainerInfo {
        id: "abc123".into(),
        name: "fussy-box".into(),
        status: Status::Up("".into()),
        image: "docker.io/library/fedora:41".into(),
    };
    match card_menu_message(CardMenuAction::Details, &container) {
        Message::Details(DetailsMsg::OpenRequested(c)) => {
            assert_eq!(c.name, "fussy-box", "Details must open this container");
        }
        other => panic!("Details must open the details page, got {other:?}"),
    }
    match card_menu_message(CardMenuAction::OpenTerminal, &container) {
        Message::Terminal(TerminalMsg::OpenRequested(name)) => {
            assert_eq!(name, "fussy-box", "Terminal must open for this container");
        }
        other => panic!("Open Terminal must open the terminal page, got {other:?}"),
    }
    match card_menu_message(CardMenuAction::Stop, &container) {
        Message::Containers(ContainerMsg::StopRequested(name)) => {
            assert_eq!(name, "fussy-box");
        }
        other => panic!("Stop must run the container stop arm, got {other:?}"),
    }
    match card_menu_message(CardMenuAction::Upgrade, &container) {
        Message::Containers(ContainerMsg::UpgradeRequested(name)) => {
            assert_eq!(name, "fussy-box");
        }
        other => panic!("Upgrade must run the container upgrade arm, got {other:?}"),
    }
    match card_menu_message(CardMenuAction::Delete, &container) {
        Message::Containers(ContainerMsg::RemoveRequested(name)) => {
            assert_eq!(
                name, "fussy-box",
                "Delete must go through the shared destructive confirm"
            );
        }
        other => panic!("Delete must request removal (confirm path), got {other:?}"),
    }
}

// ----------------------------------------------------------- T19 / row #189

/// Row #189: Ctrl+R refreshes, Ctrl+N opens the wizard, nothing else fires.
///
/// `keyboard_nav::subscription` binds Tab/Shift+Tab/Escape/F11/Ctrl+F only,
/// so these two accelerators ride the app's own listener — this pins its
/// decision, including the negatives that matter: no Ctrl, Logo held, a
/// named key, or any other character must all stay silent.
#[test]
fn ctrl_r_and_ctrl_n_fire_and_no_other_chord_does() {
    use cosmic::iced::keyboard::{Key, Modifiers};
    let ctrl = Modifiers::CTRL;
    assert_eq!(
        shortcut_for(&Key::Character("r".into()), ctrl),
        Some(Shortcut::RefreshPage),
        "Ctrl+R is the page refresh (ux.md §5.1.5)"
    );
    assert_eq!(
        shortcut_for(&Key::Character("R".into()), ctrl | Modifiers::SHIFT),
        Some(Shortcut::RefreshPage),
        "Ctrl+Shift+R still refreshes — the match is case-insensitive"
    );
    assert_eq!(
        shortcut_for(&Key::Character("n".into()), ctrl),
        Some(Shortcut::NewContainer),
        "Ctrl+N opens the create wizard (ux.md §5.1.5)"
    );
    // Negatives: each is a real mis-fire shape, not padding.
    assert_eq!(
        shortcut_for(&Key::Character("r".into()), Modifiers::empty()),
        None,
        "bare R must not refresh — typing 'r' in a field is not a chord"
    );
    assert_eq!(
        shortcut_for(&Key::Character("r".into()), ctrl | Modifiers::LOGO),
        None,
        "a window-manager chord must not fire an app action"
    );
    assert_eq!(
        shortcut_for(&Key::Character("f".into()), ctrl),
        None,
        "Ctrl+F belongs to keyboard_nav (focus search), not to this listener"
    );
    assert_eq!(
        shortcut_for(&Key::Character("q".into()), ctrl),
        None,
        "unbound characters stay silent"
    );
    assert_eq!(
        shortcut_for(
            &Key::Named(cosmic::iced::keyboard::key::Named::Escape),
            ctrl
        ),
        None,
        "named keys belong to keyboard_nav, never to this listener"
    );
}

/// Row #189: Ctrl+F focuses the page's own search field.
///
/// Images/Packages/Activity own search inputs; every other rail
/// destination has none and `on_search` no-ops there (the pushed wizard
/// and Apps overlays resolve in the caller, not in this table). A wrong id
/// here focuses nothing — the focus task silently targets an input that
/// does not exist — so the table pins the exact strings.
#[test]
fn ctrl_f_targets_each_search_field_and_no_ops_elsewhere() {
    assert_eq!(page_search_id(Page::Images), Some("search-images"));
    assert_eq!(page_search_id(Page::Packages), Some("search-packages"));
    assert_eq!(page_search_id(Page::Activity), Some("search-activity"));
    for page in [
        Page::Dashboard,
        Page::Containers,
        Page::Updates,
        Page::Backups,
        Page::Settings,
        Page::Stats,
        Page::Apps,
    ] {
        assert_eq!(
            page_search_id(page),
            None,
            "{page:?} has no search field, so Ctrl+F must no-op there"
        );
    }
}

// ----------------------------------------------------------- T19 / row #190

/// Row #190: every icon-only control has a distinct, non-empty label.
///
/// The label feeds both the a11y `description()` and the sighted tooltip
/// from one source (`IconControl::label`), so non-empty pins "announced at
/// all" and distinct pins "announced as itself". The rendered tree itself
/// is T3-observed until T20's harness lands.
#[test]
fn every_icon_only_control_has_a_distinct_non_empty_label() {
    let labels = [
        IconControl::CopyCommand.label(),
        IconControl::CardMenu.label(),
    ];
    for (control, label) in [IconControl::CopyCommand, IconControl::CardMenu]
        .iter()
        .zip(&labels)
    {
        assert!(
            !label.trim().is_empty(),
            "{control:?} renders an unlabelled icon button — invisible to a screen reader"
        );
    }
    assert_ne!(
        labels[0], labels[1],
        "two icon-only controls share a label, so a screen reader announces \
         two identical buttons: {labels:?}"
    );
}
