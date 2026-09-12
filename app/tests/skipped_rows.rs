//! B3 skipped-rows coverage at the service boundary (I23 in `docs/migration/PLAN.md`).
//!
//! Six experimentally-confirmed mutations from T13's post-commit adversarial
//! review each deleted real behaviour and left the suite green. Four shared one
//! root cause: **every** fixture reached the app through
//! `DistroboxCommandRunnerResponse::List`, which renders well-formed rows out of
//! `ContainerInfo`s — so `skipped` was structurally empty in every test, and an
//! `is_empty()` assertion passed identically for the real value and a cleared
//! one. A count of `0` is not evidence that the plumbing works; it is what
//! missing plumbing looks like too.
//!
//! These tests drive the real path instead: raw `distrobox ls` stdout, through
//! the actual parser, out through `Backend`. The two app-side halves of I23 are
//! covered at the tier that can reach them — `App`'s call site is unit-tested in
//! `app/src/views.rs` (`DashboardCounts::from_list`), since `app/` has no lib
//! target and an integration test cannot import it. The `desktop_file.rs`
//! escaping holes (e/f) live in `core/` and are covered there.

use gosh_distrobox_core::backends::Distrobox;
use gosh_distrobox_core::backends::distrobox::{DistroboxCommandRunnerResponse, ParseIssue};
use gosh_distrobox_core::service::Backend;
use gosh_distrobox_core::{EnvGuard, EnvMode};

/// Stdout with a header, two parseable rows, and one that is not: `broken` has
/// only three columns where `ContainerInfo`'s parser needs four.
const LS_WITH_ONE_BAD_ROW: &str = "\
ID           | NAME                 | STATUS             | IMAGE
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest
77aa11cc22dd | broken               | Created
9008f7e6d5c4 | fedora               | Up 2 hours         | registry.fedoraproject.org/fedora:39";

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

/// (a) — the mutation was `Backend::containers` → `out.skipped.clear()`, which
/// left every test green because no fixture ever produced a non-empty `skipped`.
#[tokio::test]
async fn backend_containers_preserves_skipped_rows() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::RawList(
        LS_WITH_ONE_BAD_ROW.into(),
    )]);

    let list = backend
        .containers()
        .await
        .expect("list succeeds despite one bad row");

    assert_eq!(
        list.containers.len(),
        2,
        "the two well-formed rows must survive their malformed neighbour"
    );
    assert_eq!(
        list.skipped.len(),
        1,
        "`skipped` must reach the app — if this is 0, B3's report is dead \
         plumbing and the user is shown a 2-container fleet without being told \
         a third row was dropped"
    );
    assert!(
        list.skipped[0].line.contains("77aa11cc22dd"),
        "the reported issue must quote the row, got {:?}",
        list.skipped[0].line
    );
}

/// The counterpart: a clean response must report nothing. Without this, a
/// mutation that fabricated skips would pass the test above.
#[tokio::test]
async fn backend_containers_reports_nothing_for_a_clean_list() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::List(
        DistroboxCommandRunnerResponse::common_distros().to_owned(),
    )]);
    let list = backend.containers().await.expect("list");
    assert!(
        list.skipped.is_empty(),
        "well-formed rows must not be reported as skips"
    );
    assert_eq!(list.containers.len(), 14);
}

/// (d) — the mutation deleted the Updates page's caption block, whose whole job
/// is to qualify a total the page just printed. The page prints `list.len()` as
/// "N Containers Available" — a count of the rows that *parsed* — so it has to
/// say when the list was short.
#[tokio::test]
async fn a_short_total_is_accompanied_by_a_nonempty_skip_list() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::RawList(
        LS_WITH_ONE_BAD_ROW.into(),
    )]);
    let list = backend.containers().await.expect("list");

    assert_eq!(list.len(), 2, "the page's total counts parsed rows only");
    assert!(
        !list.skipped.is_empty(),
        "the page's `if !containers.skipped.is_empty()` qualifier must have \
         something to fire on, or that branch is unreachable code"
    );
}

/// The gate that decides "genuinely empty" vs "everything failed" — the I16
/// rule. A list whose every row failed must not render as "create your first
/// container".
#[tokio::test]
async fn an_all_unreadable_list_is_not_a_clean_empty_account() {
    let backend = backend_for(&[DistroboxCommandRunnerResponse::RawList(
        "ID | NAME | STATUS | IMAGE\nnonsense\nmore nonsense\n".into(),
    )]);
    let list = backend.containers().await.expect("list");

    assert!(list.containers.is_empty());
    assert_eq!(list.skipped.len(), 2);
    assert!(
        !list.is_clean_empty(),
        "an all-rows-unreadable list must not be shown to the user as an empty \
         account — that is the lie `is_clean_empty` exists to prevent"
    );
}

/// `with_skipped`/`ParseIssue::new` exist so a fixture can carry a `skipped`
/// list without hand-writing a malformed table. Pinned because a fixture helper
/// that silently dropped what it was given would make every test that depends
/// on it vacuous while still passing.
#[test]
fn with_skipped_actually_attaches_the_issues() {
    let list = gosh_distrobox_core::ContainerList::default()
        .with_skipped(vec![ParseIssue::new("bad row", "3 columns, expected 4")]);
    assert_eq!(list.skipped.len(), 1);
    assert_eq!(list.skipped[0].error, "3 columns, expected 4");
    assert!(!list.is_clean_empty());
}
