//! Page views: Dashboard, Containers, Details (T6, ux.md §6.2–§6.5).
//!
//! Shared pieces (ux.md §3.3/§3.4/§3.6): `empty_state` (the ~20-times-repeated
//! icon→title→body→action pattern), `gate_view` (the three global gates —
//! blocked / not-installed / load error — implemented ONCE at the shell
//! level so every page inherits them), `container_row` (distro icon, name,
//! status dot+text, chevron), and the single-modal confirm dialog.
//!
//! Conventions honored: keep content during refresh (inline spinners, never
//! a full-page spinner over displayed data); every mutation reports through
//! the toaster (§3.4); destructive actions use the destructive button class
//! with a warning icon and the consequence in the body.

use crate::fl;
use crate::icons::{distro_icon, is_running, status_label};
use crate::message::{ContainerMsg, DetailsMsg, DialogMsg, EnvMsg, Message, TaskMsg, is_blocked};
use cosmic::iced::Length;
use cosmic::widget::toaster::{Toast, Toasts};
use cosmic::widget::{self, nav_bar};
use gosh_distrobox_core::EnvGuard;
use gosh_distrobox_core::models::ContainerInfo;

/// Pages in the nav bar. Dashboard first (Flutter rail order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Containers,
    Images,
    Packages,
    Updates,
    Backups,
    Activity,
    Apps,
    Settings,
    Stats,
}

impl Page {
    pub fn title(self) -> String {
        match self {
            Page::Dashboard => fl!("nav-dashboard"),
            Page::Containers => fl!("nav-containers"),
            Page::Images => fl!("nav-images"),
            Page::Packages => fl!("nav-packages"),
            Page::Updates => fl!("nav-updates"),
            Page::Backups => fl!("nav-backups"),
            Page::Activity => fl!("nav-activity"),
            Page::Apps => fl!("nav-apps"),
            Page::Settings => fl!("nav-settings"),
            Page::Stats => fl!("nav-stats"),
        }
    }

    pub const ALL: [Page; 10] = [
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
    ];
}

/// Which page is active (nav-bar data lookup with Containers fallback).
pub fn active_page(nav_model: &nav_bar::Model) -> Page {
    nav_model
        .active_data::<Page>()
        .copied()
        .unwrap_or(Page::Containers)
}

/// Activate a tab by page (replaces positional `activate_position` — the
/// T6/T7 numeric literals broke silently when pages were added).
pub fn activate_page(nav_model: &mut nav_bar::Model, page: Page) {
    if let Some(pos) = Page::ALL.iter().position(|p| *p == page) {
        nav_model.activate_position(pos as u16);
    }
}

/// Activate the Containers tab (row #23 "View all" — dead in Flutter).
pub fn activate_containers(nav_model: &mut nav_bar::Model) {
    activate_page(nav_model, Page::Containers);
}

/// Icon/title/body for the empty state of a container list (B3, §6.4).
///
/// An "empty" list has two meanings and they are not interchangeable: a
/// genuinely empty account, where "create one" is the right advice, and a
/// list where every row was returned but failed to parse, where it is a lie —
/// the user has containers, we just could not read the report. All three
/// parts switch together so they cannot contradict each other, and
/// `clean_body` is the caller's own phrasing for the first case ("before
/// managing backups" / "managing packages" / "to get started").
///
/// The three container-gated pages (Containers, Backups, Packages) all render
/// through this. Only the Dashboard carries a skipped-row caption, so on those
/// pages this copy is the *only* explanation of why the list looks empty.
pub fn container_list_copy(
    containers: &gosh_distrobox_core::ContainerList,
    clean_body: &str,
) -> (&'static str, String, String) {
    if containers.is_clean_empty() {
        return (
            "document-open-symbolic",
            fl!("dash-no-containers"),
            clean_body.to_string(),
        );
    }
    let n = containers.skipped.len();
    (
        "dialog-warning-symbolic",
        fl!("dash-rows-unreadable-title", count = n),
        fl!("dash-rows-unreadable-body", count = n),
    )
}

/// The one-line honest account of a list where *every* row came back and none
/// could be read (B3).
///
/// Deliberately **not** gated by `show_skipped_lines`: that preference decides
/// whether the Dashboard volunteers a row count for an otherwise working
/// list, and it defaults to off. When nothing parsed it is not a detail, it is
/// the entire state of the page, so suppressing it would leave the Dashboard
/// saying "no containers configured" to a user who has six — the exact lie
/// `container_list_copy` exists to prevent on the three gated pages.
///
/// Shared by the Dashboard's status card and its container preview so those
/// two surfaces cannot drift apart, and phrased to match
/// `container_list_copy`'s all-rows-failed branch.
pub fn all_rows_failed_copy(skipped: usize) -> String {
    fl!("dash-all-rows-failed", count = skipped)
}

/// The Dashboard status card's body line, as a pure function.
///
/// Takes **no** `show_skipped` argument, and that is the point: the empty-list
/// wording must not be reachable from the preference, or a user with six
/// unreadable containers reads "No containers configured." A future caller that
/// wants the caption for a *working* list wants `skipped_summary`, which does
/// take the flag.
///
/// Pure because the app crate has no lib target: this is the only tier that can
/// pin the copy without driving the widget tree, the same reason `skipped_summary`
/// lives here rather than inline in the view.
pub fn dashboard_status_body(running: usize, total: usize, skipped: usize) -> String {
    if total == 0 {
        if skipped == 0 {
            fl!("dash-no-containers-configured")
        } else {
            all_rows_failed_copy(skipped)
        }
    } else {
        fl!("dash-status-running", running = running, total = total)
    }
}

/// The Dashboard container-preview empty state (icon, title, body), same rule
/// and same reason as `dashboard_status_body` — both surfaces derive from this
/// one decision so they cannot disagree about whether the list was readable.
pub fn dashboard_preview_copy(skipped: usize) -> (&'static str, String, String) {
    if skipped == 0 {
        (
            "document-open-symbolic",
            fl!("dash-preview-empty-title"),
            fl!("dash-preview-empty-body"),
        )
    } else {
        (
            "dialog-warning-symbolic",
            fl!("dash-preview-unreadable-title"),
            all_rows_failed_copy(skipped),
        )
    }
}

/// Shared empty state (§3.4): icon → title → body → optional action.
pub fn empty_state(
    icon: &'static str,
    title: String,
    body: String,
    action: Option<cosmic::Element<'static, Message>>,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::icon::from_name(icon).size(48).icon())
        .push(widget::text::title3(title))
        .push(widget::text::body(body))
        .spacing(12)
        .align_x(cosmic::iced::Alignment::Center);
    if let Some(action) = action {
        col = col.push(action);
    }
    widget::container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

/// The three global gates (§3.4), evaluated in order. `Some` = gate active
/// (render this INSTEAD of the page); `None` = render the page.
pub fn gate(
    blocked: Option<String>,
    distrobox_installed: bool,
    loading_anything: bool,
) -> Option<cosmic::Element<'static, Message>> {
    if let Some(message) = blocked {
        // Row #12: Environment Blocked (icon, title, message, Check Again).
        // "Check Again" re-probes the environment (T17): the probe is no
        // longer frozen at `init`, so installing `distrobox-host-exec`
        // mid-session and checking again recovers without a restart.
        return Some(empty_state(
            "dialog-error-symbolic",
            fl!("dash-env-blocked-title"),
            message,
            Some(
                widget::button::suggested(fl!("dash-check-again"))
                    .on_press(Message::Env(EnvMsg::ReprobeRequested))
                    .into(),
            ),
        ));
    }
    if !distrobox_installed && !loading_anything {
        // Row #13: Distrobox Not Found. Same re-probe path as #12 above —
        // re-running `containers()` against the frozen guard is what made
        // this button inert for the process lifetime (I29).
        return Some(empty_state(
            "dialog-warning-symbolic",
            fl!("dash-distrobox-missing-title"),
            fl!("dash-distrobox-missing-body"),
            Some(
                widget::button::suggested(fl!("dash-check-again"))
                    .on_press(Message::Env(EnvMsg::ReprobeRequested))
                    .into(),
            ),
        ));
    }
    None
}

/// Post-reprobe decision (T17, row #13): what the `Probed` arm owes the user
/// for this guard. Pure so `app/tests/parity_rows.rs` pins it — the arm
/// itself needs a constructed `App` (I31 harness, still open), so without
/// this seam the gate-state transition would be `source` tier again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReprobeOutcome {
    /// The environment answers again: clear the banner and reload
    /// containers + version, the same two loads `init` fires.
    Recovered,
    /// Still gated: the banner to show (`Some` only for `Blocked`, whose
    /// message is the Dart guard's wording verbatim — same as `init`).
    StillGated { error: Option<String> },
}

/// See [`ReprobeOutcome`].
pub fn reprobe_outcome(guard: &EnvGuard) -> ReprobeOutcome {
    if is_blocked(guard) {
        return ReprobeOutcome::StillGated {
            error: guard.message.clone(),
        };
    }
    if guard.distrobox_installed {
        ReprobeOutcome::Recovered
    } else {
        ReprobeOutcome::StillGated { error: None }
    }
}

/// Header Refresh routing (T17, row #97): which domain the shell header's
/// Refresh button reloads for `page`. `None` = the page owns its own header
/// actions (Updates, Apps) and the generic button stays hidden — the Apps
/// special-case this mirrors. Pure so `parity_rows.rs` pins every page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderRefresh {
    Containers,
    Images,
}

/// See [`HeaderRefresh`].
pub fn header_refresh(page: Page) -> Option<HeaderRefresh> {
    match page {
        Page::Images => Some(HeaderRefresh::Images),
        Page::Updates | Page::Apps => None,
        _ => Some(HeaderRefresh::Containers),
    }
}

/// Shared container row (§6.3, rows #24/#37–#39): distro icon, name, status
/// dot + text, chevron. Tap → details; inline Stop when running (row #25).
pub fn container_row(
    container: &ContainerInfo,
    selected: bool,
) -> cosmic::Element<'static, Message> {
    // Rows #24/#34/#38/#45: distro icon, name, status DOT + text, chevron.
    // The dot is a ● glyph in default text colour + the `status_label` copy.
    // `status_color()` (theme success/accent/warning/control) exists and is
    // unit-tested, but NEITHER `Text::color` NOR `SelectableText::color`
    // satisfy `<Theme as Catalog>::Class: From<StyleFn>` in this iced rev —
    // coloured text is structurally unavailable. No hard-coded colours
    // (ux.md §3.5 holds); the coloured dot lands when the bound lifts.
    let status = status_label(&container.status);
    let row = widget::Row::new()
        .push(
            widget::icon::from_name(distro_icon(&container.image))
                .size(20)
                .icon(),
        )
        .push({
            let status_line: cosmic::Element<'static, Message> =
                widget::text::caption(format!("● {status}")).into();
            let name_col: cosmic::Element<'static, Message> = widget::Column::new()
                .push(widget::text::body(container.name.clone()))
                .push(status_line)
                .spacing(2)
                .width(Length::Fill)
                .into();
            name_col
        })
        .push({
            let chev: cosmic::Element<'static, Message> =
                widget::icon::from_name("go-next-symbolic")
                    .size(16)
                    .icon()
                    .into();
            chev
        })
        .spacing(12)
        .align_y(cosmic::iced::Alignment::Center);
    let mut list = widget::list_column::list_column();
    list = list.add(
        widget::list::button(row)
            .on_press(Message::Details(DetailsMsg::OpenRequested(
                container.clone(),
            )))
            .selected(selected),
    );
    let mut col = widget::Column::new().push(list.into_element());
    if is_running(&container.status) {
        col = col.push({
            let stop: cosmic::Element<'static, Message> = widget::button::text(fl!("action-stop"))
                .on_press(Message::Containers(ContainerMsg::StopRequested(
                    container.name.clone(),
                )))
                .into();
            stop
        });
    }
    col.into()
}

/// Dashboard (rows #9–#34): status card, stat tiles, active tasks (only when
/// non-empty), container preview (first 5), quick actions. NO pull-to-refresh
/// (row #32, touch idiom — header Refresh button instead); content stays
/// B3 (§6.4 row B3, and the `containers: ContainerList` field of the §2.3 DRAFT
/// `Message` struct): the one-line account of rows that `Distrobox::list`
/// could not parse. `None` when the setting is off, in which case the
/// Dashboard says nothing at all.
///
/// Pure and separately tested: the app crate has no lib target, so this is
/// the only tier that can cover the copy without driving the whole UI.
pub fn skipped_summary(skipped: usize, show: bool) -> Option<String> {
    if !show || skipped == 0 {
        return None;
    }
    Some(fl!("dash-skipped-rows", count = skipped))
}

/// The numbers the Dashboard status card is built from. Grouped rather than
/// passed as four bare `usize`s so the call site reads as named fields and
/// the signature stays inside clippy's argument limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DashboardCounts {
    pub running: usize,
    pub stopped: usize,
    pub total: usize,
    /// Rows `list()` could not parse (B3). The view turns this into the
    /// caption itself — including applying the setting — so the call site
    /// passes the count, not a pre-formatted string.
    pub skipped: usize,
    /// The `show_skipped_lines` preference (default off).
    pub show_skipped: bool,
}

impl DashboardCounts {
    /// Derive the card's numbers from the list (B3).
    ///
    /// A named constructor rather than an inline struct literal at the call
    /// site, for the reason `dashboard_status_body` and `skipped_summary` are
    /// pure functions: `app/` has no lib target, so a value built inline inside
    /// `view_dashboard` can only be reached by driving the widget tree. I23
    /// found the consequence — the skipped count was un-pinned end to end, so
    /// hardcoding `skipped: 0` here left every test green. This is the seam
    /// that makes it testable.
    /// Delegates the running/stopped split to `message::running_count` /
    /// `stopped_count` rather than re-deriving it: those are the helpers the
    /// call site used before this constructor existed, and a second definition
    /// of "running" is exactly how two surfaces start disagreeing.
    /// (`&ContainerList` reaches their `&[ContainerInfo]` parameter through
    /// `Deref`, the same coercion the ~28 other reads in the app rely on.)
    pub fn from_list(list: &gosh_distrobox_core::ContainerList, show_skipped: bool) -> Self {
        Self {
            running: crate::message::running_count(list),
            stopped: crate::message::stopped_count(list),
            total: list.len(),
            skipped: list.skipped.len(),
            show_skipped,
        }
    }
}

/// during refresh (row #11).
pub fn view_dashboard(
    containers: &[ContainerInfo],
    counts: DashboardCounts,
    active_task_count: usize,
    task_rows: Vec<cosmic::Element<'static, Message>>,
    error: Option<String>,
) -> cosmic::Element<'static, Message> {
    let DashboardCounts {
        running,
        stopped,
        total,
        skipped,
        show_skipped,
    } = counts;
    let skipped_summary = skipped_summary(skipped, show_skipped);
    let healthy = error.is_none();
    let mut col = widget::Column::new().spacing(16);

    // Status card (rows #14–#15).
    let mut status = widget::Column::new()
        .push(widget::text::caption_heading(fl!("dash-system-status")))
        .push(widget::text::title4(if healthy {
            fl!("dash-all-systems-operational")
        } else {
            fl!("dash-attention-required")
        }))
        .push(widget::text::body(dashboard_status_body(
            running, total, skipped,
        )))
        .spacing(4);
    // B3: "12 containers, 3 rows skipped" rather than a silently short list.
    if let Some(summary) = skipped_summary {
        status = status.push(widget::text::caption(summary));
    }
    if let Some(err) = error {
        let warn: cosmic::Element<'static, Message> = widget::warning(err).into();
        status = status.push(warn);
    }
    col = col.push({
        let status_el: cosmic::Element<'static, Message> = status.into();
        status_el
    });

    // Stat tiles (rows #16–#18).
    col = col.push({
        let tiles: cosmic::Element<'static, Message> = widget::Row::new()
            .push(stat_tile(fl!("dash-stat-total"), total.to_string()))
            .push(stat_tile(fl!("dash-stat-running"), running.to_string()))
            .push(stat_tile(fl!("dash-stat-stopped"), stopped.to_string()))
            .spacing(12)
            .into();
        tiles
    });

    // Active tasks, conditional (rows #19–#21). Rows come from the app
    // (mirror state) — this module stays stateless.
    if !task_rows.is_empty() {
        col = col.push({
            let t: cosmic::Element<'static, Message> =
                widget::text::caption_heading(fl!("dash-active-tasks")).into();
            t
        });
        for row in task_rows {
            col = col.push(row);
        }
        let _ = active_task_count;
    }

    // Containers preview (rows #22–#27): first 5 + View all.
    col = col.push({
        let header: cosmic::Element<'static, Message> = widget::Row::new()
            .push(widget::text::caption_heading(fl!("dash-containers-heading")).width(Length::Fill))
            .push({
                let view_all: cosmic::Element<'static, Message> =
                    widget::button::text(fl!("dash-view-all"))
                        .on_press(Message::Containers(ContainerMsg::ViewAllRequested))
                        .into();
                view_all
            })
            .align_y(cosmic::iced::Alignment::Center)
            .into();
        header
    });
    if containers.is_empty() {
        // Same rule as the status card above and as `container_list_copy` on
        // the three gated pages (I16): an unreadable list is not an empty one.
        let (icon, title, body) = dashboard_preview_copy(counts.skipped);
        col = col.push(empty_state(icon, title.to_string(), body, None));
    } else {
        for container in containers.iter().take(5) {
            col = col.push(container_row(container, false));
        }
    }

    // Quick actions (rows #28–#31). New Container → wizard; Upgrade All → confirm → per-container tasks (row #29
    // implemented for real, not a redirect snackbar); Stop All + confirm
    // (row #30, destructive class); Refresh (row #31).
    col = col.push({
        let t: cosmic::Element<'static, Message> =
            widget::text::caption_heading(fl!("app-quick-actions")).into();
        t
    });
    col = col.push({
        let new_btn: cosmic::Element<'static, Message> =
            widget::button::standard(fl!("app-new-container"))
                .on_press(Message::Containers(ContainerMsg::NewContainerRequested))
                .into();
        let upgrade_btn: cosmic::Element<'static, Message> =
            widget::button::standard(fl!("app-upgrade-all"))
                .on_press(Message::Containers(ContainerMsg::UpgradeAllRequested))
                .into();
        let actions: cosmic::Element<'static, Message> = widget::Row::new()
            .push(new_btn)
            .push(upgrade_btn)
            .spacing(12)
            .into();
        actions
    });
    col = col.push({
        let stop_all: cosmic::Element<'static, Message> =
            widget::button::standard(fl!("app-stop-all"))
                .on_press(Message::Containers(ContainerMsg::StopAllRequested))
                .into();
        let refresh: cosmic::Element<'static, Message> =
            widget::button::standard(fl!("action-refresh"))
                .on_press(Message::Containers(ContainerMsg::RefreshRequested))
                .into();
        let row: cosmic::Element<'static, Message> = widget::Row::new()
            .push(stop_all)
            .push(refresh)
            .spacing(12)
            .into();
        row
    });

    widget::scrollable(col).into()
}

fn stat_tile(title: String, value: String) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::title3(value.to_string()))
        .push(widget::text::caption(title.to_string()))
        .spacing(4)
        .width(Length::Fill)
        .into()
}

/// What a task row's trailing affordance is, given the T5 mirror's two flags.
///
/// **Why this is a named type rather than an `if` inside `task_row`.** An
/// `Element` is opaque, so a test cannot inspect what a row rendered — which is
/// how row #20's gap survived to the T16 walk. Making the decision a pure value
/// gives the view's contract something executable to assert against, the same
/// seam `DashboardCounts::from_list` gives the Dashboard's skip count.
///
/// The running variant is deliberately **not** called `Running`. I29/#20 is the
/// finding that a running task row renders no spinner at all — only a Cancel
/// button — while both the frozen row and `task_row`'s doc comment claim
/// "spinner while running". `RunningCancelOnly` puts that gap in the type's own
/// vocabulary, so adding a progress affordance forces this variant's rename and
/// therefore updates the test that pins it, instead of silently leaving two
/// documents describing behaviour that now exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskAffordance {
    /// Completed successfully: a check.
    Succeeded,
    /// Completed with a failure: an error icon, never a check (row #21).
    Failed,
    /// Still running: a Cancel control, and nothing else (row #20 / I29).
    RunningCancelOnly,
}

impl TaskAffordance {
    /// The theme icon name this affordance renders as, or `None` when it is a
    /// control rather than an icon.
    ///
    /// This is the single source for the icon choice. `task_row` previously
    /// re-derived the success/failure icon from `success` while
    /// `task_affordance` independently picked the variant — two sources for one
    /// decision, which is the shape that let row #20's doc comment drift from the
    /// code it describes.
    pub fn icon_name(self) -> Option<&'static str> {
        match self {
            TaskAffordance::Succeeded => Some("object-select-symbolic"),
            TaskAffordance::Failed => Some("dialog-error-symbolic"),
            TaskAffordance::RunningCancelOnly => None,
        }
    }
}

/// The trailing-affordance decision, factored out of `task_row` so it is
/// testable. `completed`/`success` come from the T5 mirror — no
/// string-sniffing (§3.1).
pub fn task_affordance(completed: bool, success: bool) -> TaskAffordance {
    match (completed, success) {
        (true, true) => TaskAffordance::Succeeded,
        (true, false) => TaskAffordance::Failed,
        (false, _) => TaskAffordance::RunningCancelOnly,
    }
}

/// Active-task row (rows #19–#21): label + done/cancel.
///
/// **Row #20 is not met, and I29 filed it as such.** The frozen row and this
/// doc comment both say "spinner while running"; the running branch renders a
/// Cancel button and no progress affordance. See `TaskAffordance::RunningCancelOnly`
/// for why the gap is spelled out rather than papered over.
pub fn task_row(
    id: gosh_distrobox_core::TaskId,
    label: String,
    completed: bool,
    success: bool,
) -> cosmic::Element<'static, Message> {
    // Rows #20/#21: success check vs failure icon when done (a failed task must
    // not render a success check).
    let mut row = widget::Row::new()
        .push(widget::text::body(label).width(Length::Fill))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);
    // The icon name comes off the resolved variant rather than being re-derived
    // from `success` here: `task_affordance` already made that decision, and two
    // sources for one decision is how row #20's doc comment drifted from its code.
    if let Some(icon) = task_affordance(completed, success).icon_name() {
        let done: cosmic::Element<'static, Message> =
            widget::icon::from_name(icon).size(16).icon().into();
        row = row.push(done);
    } else {
        // `RunningCancelOnly`: the Cancel control is the only trailing widget,
        // because there is no progress affordance (I29 / row #20).
        let cancel: cosmic::Element<'static, Message> = widget::button::text(fl!("action-cancel"))
            .on_press(Message::Tasks(TaskMsg::CancelRequested(id)))
            .into();
        row = row.push(cancel);
    }
    row.into()
}

/// Details page (rows #53–#65): nested under Containers. Header back button
/// pops (row #53); hero card with image chip (#56, tap copies — #57);
/// status card with Stop when running (#58–#59); Upgrade tile with inline
/// spinner (#60, shared task component); Applications tile → Apps (#61);
/// Clone tile → dialog (#62); Open Terminal tile, disabled when stopped
/// (#63); Danger Zone delete + confirm (#64). Start/restart (row #65) is
/// P1-blocking per §4.1 — NO start op exists in the backend, so no Start
/// button is rendered (a button that cannot work is worse than none; the
/// not-running copy says terminal/upgrades need a running container).
pub fn view_details(
    container: &ContainerInfo,
    upgrading: bool,
) -> cosmic::Element<'static, Message> {
    let running = is_running(&container.status);
    let mut col = widget::Column::new().spacing(16);

    // Hero card (rows #56–#57): icon, name, image chip (tap = copy).
    col = col.push({
        let hero: cosmic::Element<'static, Message> = widget::Column::new()
            .push({
                let hero_row: cosmic::Element<'static, Message> = widget::Row::new()
                    .push(
                        widget::icon::from_name(distro_icon(&container.image))
                            .size(32)
                            .icon(),
                    )
                    .push(widget::text::title3(container.name.clone()).width(Length::Fill))
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
                    .into();
                hero_row
            })
            .push({
                let copy: cosmic::Element<'static, Message> =
                    widget::button::text(container.image.clone())
                        .on_press(Message::Details(DetailsMsg::CopyImageRequested(
                            container.image.clone(),
                        )))
                        .into();
                copy
            })
            .spacing(8)
            .into();
        hero
    });

    // Status card (rows #58–#59).
    let mut status_row = widget::Row::new()
        .push(widget::text::body(status_label(&container.status)).width(Length::Fill))
        .push(widget::text::caption(fl!(
            "dash-container-id",
            id = container.id.to_string()
        )))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);
    if running {
        status_row = status_row.push({
            let stop: cosmic::Element<'static, Message> =
                widget::button::standard(fl!("action-stop"))
                    .on_press(Message::Details(DetailsMsg::StopRequested(
                        container.name.clone(),
                    )))
                    .into();
            stop
        });
    }
    col = col.push({
        let heading: cosmic::Element<'static, Message> =
            widget::text::caption_heading(fl!("dash-container-status")).into();
        let status_el: cosmic::Element<'static, Message> = status_row.into();
        let card: cosmic::Element<'static, Message> = widget::Column::new()
            .push(heading)
            .push(status_el)
            .spacing(4)
            .into();
        card
    });

    // Quick actions (rows #60–#63). Push widgets directly (T3 pattern):
    // a typed Column accepts widgets; pre-wrapping in Element breaks the
    // Theme/Renderer inference this iced rev needs.
    let upgrade_label = if upgrading {
        fl!("dash-upgrading")
    } else {
        fl!("dash-upgrade-container")
    };
    let actions = widget::Column::new()
        .push(
            widget::button::standard(upgrade_label).on_press_maybe(if upgrading {
                None
            } else {
                Some(Message::Details(DetailsMsg::UpgradeRequested(
                    container.name.clone(),
                )))
            }),
        )
        .push(
            widget::button::standard(fl!("nav-apps")).on_press(Message::Details(
                DetailsMsg::AppsRequested(container.name.clone()),
            )),
        )
        .push(
            widget::button::standard(fl!("dash-clone-container")).on_press(Message::Details(
                DetailsMsg::CloneRequested(container.name.clone()),
            )),
        )
        .spacing(4);
    let actions = actions.push(
        widget::button::standard(fl!("dash-open-terminal")).on_press_maybe(if running {
            Some(Message::Details(DetailsMsg::TerminalRequested(
                container.name.clone(),
            )))
        } else {
            None
        }),
    );
    col = col.push(
        widget::Column::new()
            .push(widget::text::caption_heading(fl!("app-quick-actions")))
            .push(actions)
            .spacing(4),
    );

    // Danger zone (row #64): shared destructive confirm (§3.3).
    col = col.push(
        widget::Column::new()
            .push(widget::text::caption_heading(fl!("dash-danger-zone")))
            .push(
                widget::button::destructive(fl!("dash-delete-container")).on_press(
                    Message::Details(DetailsMsg::RemoveRequested(container.name.clone())),
                ),
            )
            .spacing(4),
    );

    widget::scrollable(col).into()
}

/// Clone dialog body state lives in `App` (T6): source + editable name.
/// Rendered through `Application::dialog()` (single modal slot, §3.3).
/// `CloneNameChanged` (not `CloneConfirmed`) carries keystrokes — confirming
/// with an empty source would clone nothing; the Clone button sends the real
/// `CloneConfirmed { source, name }`.
pub fn clone_dialog(source: &str, name: &str) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::body(fl!(
            "dash-clone-dialog-body",
            source = source
        )))
        .push({
            let input: cosmic::Element<'static, Message> = widget::text_input::text_input(
                fl!("dash-clone-dialog-placeholder"),
                name.to_string(),
            )
            .on_input(|s| Message::Details(DetailsMsg::CloneNameChanged(s)))
            .into();
            input
        })
        .spacing(8)
        .into()
}

/// The single modal (§3.3): confirm dialogs share copy via `ConfirmSpec`
/// (fixing the card-vs-details divergence, row #52); the clone dialog is
/// the only input dialog in T6 scope.
pub fn dialog_view(
    dialog: &Option<crate::app::ActiveDialog>,
) -> Option<cosmic::Element<'static, Message>> {
    let dialog = dialog.as_ref()?;
    match dialog {
        crate::app::ActiveDialog::Confirm(spec) => {
            let primary: cosmic::Element<'static, Message> = if spec.destructive {
                widget::button::destructive(spec.confirm_label.clone())
                    .on_press(Message::Dialog(DialogMsg::Confirmed))
                    .into()
            } else {
                widget::button::suggested(spec.confirm_label.clone())
                    .on_press(Message::Dialog(DialogMsg::Confirmed))
                    .into()
            };
            Some(
                widget::dialog()
                    .title(spec.title.clone())
                    .body(spec.body.clone())
                    .primary_action(primary)
                    .secondary_action({
                        let cancel: cosmic::Element<'static, Message> =
                            widget::button::standard(fl!("action-cancel"))
                                .on_press(Message::Dialog(DialogMsg::Cancelled))
                                .into();
                        cancel
                    })
                    .into(),
            )
        }
        crate::app::ActiveDialog::Clone { source, name } => Some(
            widget::dialog()
                .title(fl!("dash-clone-dialog-title", source = source))
                .control(clone_dialog(source, name))
                .primary_action({
                    let clone: cosmic::Element<'static, Message> =
                        widget::button::suggested(fl!("dash-clone"))
                            .on_press(Message::Details(DetailsMsg::CloneConfirmed {
                                source: source.clone(),
                                name: name.clone(),
                            }))
                            .into();
                    clone
                })
                .secondary_action({
                    let cancel: cosmic::Element<'static, Message> =
                        widget::button::standard(fl!("action-cancel"))
                            .on_press(Message::Dialog(DialogMsg::Cancelled))
                            .into();
                    cancel
                })
                .into(),
        ),
    }
}

/// Wrap the page in the toaster (§3.4): every mutation reports through
/// toasts, never bare snackbars — and package/backups pages' silent
/// failures become visible here by construction.
pub fn with_toasts<'a>(
    toasts: &'a Toasts<Message>,
    page: cosmic::Element<'a, Message>,
) -> cosmic::Element<'a, Message> {
    widget::toaster(toasts, page)
}

/// Push a toast; returns the follow-up `Task` (auto-dismiss timer).
/// `Toasts::push` yields an iced `Task<Message>`; map it into the app's
/// `Task<Action<Message>>` via `From` (`Action::App`).
pub fn push_toast(toasts: &mut Toasts<Message>, text: String) -> cosmic::app::Task<Message> {
    toasts.push(Toast::new(text)).map(cosmic::Action::App)
}

#[cfg(test)]
mod tests {
    use super::{
        DashboardCounts, all_rows_failed_copy, container_list_copy, dashboard_preview_copy,
        dashboard_status_body, skipped_summary,
    };

    /// The Containers page picks its empty state on `is_clean_empty()`, not
    /// `is_empty()` (B3): a list whose rows ALL failed to parse is not an
    /// empty account, and must not be told to "create your first container".
    /// This pins the predicate the branch is written against — including that
    /// it stays distinct from `is_empty` in the one case that matters.
    #[test]
    fn all_rows_skipped_is_not_a_clean_empty() {
        use gosh_distrobox_core::{ContainerList, ParseIssue};
        let all_skipped = ContainerList {
            containers: vec![],
            skipped: vec![ParseIssue {
                line: "garbage".into(),
                error: "expected four fields".into(),
            }],
        };
        assert!(all_skipped.is_empty(), "the cache slice is empty…");
        assert!(
            !all_skipped.is_clean_empty(),
            "…but rows WERE returned, so `is_empty()` alone would show the wrong state"
        );
        assert!(ContainerList::default().is_clean_empty(), "no rows at all");
    }

    /// The app-side half of I23, at the seam `from_list` was introduced to be.
    ///
    /// T13's post-commit review found `app/src/app.rs`'s Dashboard counts
    /// un-pinnable because `app/` has no lib target — so the value was built
    /// inline inside `view_dashboard`, reachable only by driving the widget
    /// tree, and hardcoding `skipped: 0` left the suite green. `from_list` is
    /// the answer to that: named, pure, and callable from here.
    ///
    /// It shipped without a test. Both this file's doc comment above it and
    /// `app/tests/skipped_rows.rs`'s module header asserted the seam *was*
    /// tested, which made the claim worse than not having one — a reader
    /// checking where I23's residual (b) was closed would have found a
    /// reassuring sentence and no test. Pinned here so the sentence is true.
    ///
    /// The two mutations I23 named by hand, both of which this fails:
    /// `skipped: 0` and `show_skipped: true`.
    #[test]
    fn dashboard_counts_do_not_prettify_the_skip_count() {
        use gosh_distrobox_core::models::{ContainerInfo, Status};
        use gosh_distrobox_core::{ContainerList, ParseIssue};

        // Two containers, one of them running, plus a row that did not parse —
        // the shape a real `distrobox ls` with one bad row produces.
        let container = |name: &str, status: Status| ContainerInfo {
            id: name.into(),
            name: name.into(),
            status,
            image: "registry.fedoraproject.org/fedora:39".into(),
        };
        let list = ContainerList {
            containers: vec![
                container("fedora", Status::Up("Up 2 hours".into())),
                container("ubuntu", Status::Created("Created".into())),
            ],
            skipped: vec![ParseIssue::new("77aa11cc22dd | broken", "3 columns")],
        };

        let counts = DashboardCounts::from_list(&list, false);
        assert_eq!(counts.total, 2, "the total counts parsed rows");
        assert_eq!(
            counts.running, 1,
            "the running split is delegated, not re-derived"
        );
        assert_eq!(counts.stopped, 1);
        assert_eq!(
            counts.skipped, 1,
            "mutation: `skipped: 0` here is the exact edit I23 found green — the \
             user is then shown a two-container fleet with no notice that a third \
             row was dropped"
        );

        // Mutation: `show_skipped: true`, which would render the caption for a
        // user who turned the preference off. Both directions asserted, so a
        // hardcoded `true` and a hardcoded `false` each fail one of them.
        assert!(
            !DashboardCounts::from_list(&list, false).show_skipped,
            "the preference must reach the view as the user set it"
        );
        assert!(
            DashboardCounts::from_list(&list, true).show_skipped,
            "…in both directions, or a hardcoded `false` passes the assertion above"
        );

        // The caption is gated on the preference *and* the count, so an
        // all-clean list must stay silent even with the preference on.
        let clean = DashboardCounts::from_list(&ContainerList::default(), true);
        assert_eq!(clean.skipped, 0);
        assert_eq!(
            super::skipped_summary(clean.skipped, clean.show_skipped),
            None
        );
    }

    /// `container_list_copy` feeds all three container-gated empty states
    /// (Containers, Backups, Packages), and only the Dashboard has a skipped
    /// caption — so on those pages this copy is the sole explanation. A
    /// clean-empty must give the caller's advice; an all-skipped must give the
    /// count, switch the icon to the warning, and never offer that advice.
    #[test]
    fn container_list_copy_switches_all_three_parts_on_clean_empty() {
        use gosh_distrobox_core::{ContainerList, ParseIssue};
        let clean = ContainerList::default();
        let (icon, title, body) = container_list_copy(&clean, "Create a container.");
        assert_eq!(icon, "document-open-symbolic");
        assert_eq!(title, crate::fl!("dash-no-containers"));
        assert_eq!(body, "Create a container.");

        let skipped = ContainerList {
            containers: vec![],
            skipped: vec![
                ParseIssue {
                    line: "a".into(),
                    error: "e".into(),
                },
                ParseIssue {
                    line: "b".into(),
                    error: "e".into(),
                },
            ],
        };
        let (icon, title, body) = container_list_copy(&skipped, "Create a container.");
        assert_eq!(icon, "dialog-warning-symbolic", "not the benign glyph");
        assert!(title.contains('2'), "the count leads the title: {title}");
        assert!(body.contains('2'), "and is stated in the body: {body}");
        assert!(
            !body.contains("Create"),
            "the create advice is a lie when rows were returned: {body}"
        );

        // Singular, since one unreadable row is the common case.
        let one = ContainerList {
            containers: vec![],
            skipped: vec![ParseIssue {
                line: "a".into(),
                error: "e".into(),
            }],
        };
        let (_, title, body) = container_list_copy(&one, "Create a container.");
        assert!(
            strip_isolation(&title).contains("1 row "),
            "singular noun in title: {title}"
        );
        assert!(
            strip_isolation(&body).contains("reported 1 container,"),
            "singular noun: {body}"
        );
        assert!(
            body.contains("it did not match"),
            "singular pronoun: {body}"
        );
        assert!(!body.contains("1 containers"), "{body}");
        assert!(
            !body.contains("none of them"),
            "plural-only phrasing: {body}"
        );
    }

    /// The words a user actually reads, with Fluent's bidi-isolation markers
    /// (U+2068/U+2069) taken back out.
    ///
    /// Every `{ $placeable }` that is not the only element of its pattern is
    /// wrapped in those two characters — `use_isolating` defaults to true and
    /// i18n-embed never turns it off — so `"reported { $count } containers"`
    /// renders as `"reported \u{2068}1\u{2069} containers"`. Assertions about
    /// the *copy* have to look past them; assertions about the count being
    /// present at all do not (`contains('1')` is unaffected).
    fn strip_isolation(s: &str) -> String {
        s.replace(['\u{2068}', '\u{2069}'], "")
    }

    /// B3 (§6.4): the Dashboard mentions skipped rows only when the setting
    /// is on AND there is something to mention. Default-off means a user who
    /// never asked sees no change from before this feature existed.
    #[test]
    fn skipped_summary_is_off_by_default_and_silent_when_empty() {
        assert_eq!(skipped_summary(3, false), None, "setting off → nothing");
        assert_eq!(skipped_summary(0, true), None, "nothing skipped → nothing");
        assert_eq!(skipped_summary(0, false), None);
    }

    /// "12 containers, 3 rows skipped" (architecture.md §6.4, row B3) — and the
    /// singular form, since a one-row skip is the common case.
    #[test]
    fn skipped_summary_counts_rows() {
        // Compare against the catalogue, not against a copy of its text: the
        // copy now lives in the .ftl and restating it here is how the two
        // drift apart.
        assert_eq!(
            skipped_summary(1, true).as_deref(),
            Some(crate::fl!("dash-skipped-rows", count = 1).as_str())
        );
        assert_eq!(
            skipped_summary(3, true).as_deref(),
            Some(crate::fl!("dash-skipped-rows", count = 3).as_str())
        );
        assert_ne!(
            skipped_summary(1, true),
            skipped_summary(3, true),
            "the plural selector must still distinguish one row from several"
        );
    }

    /// `all_rows_failed_copy` is deliberately NOT gated by
    /// `show_skipped_lines`, unlike `skipped_summary` above, and the two must
    /// not be collapsed into one helper. The preference hides a *detail* about
    /// a working list; when nothing parsed there is no working list, and the
    /// Dashboard would otherwise say "No containers configured" to a user who
    /// has six. Pinned as a contrast so a future refactor that routes both
    /// through the setting fails here rather than shipping the lie.
    #[test]
    fn all_rows_failed_copy_is_ungated_and_singular_aware() {
        assert_eq!(
            all_rows_failed_copy(1),
            crate::fl!("dash-all-rows-failed", count = 1)
        );
        assert_eq!(
            all_rows_failed_copy(6),
            crate::fl!("dash-all-rows-failed", count = 6)
        );
        assert_ne!(
            all_rows_failed_copy(1),
            all_rows_failed_copy(6),
            "the singular branch must not have collapsed into the plural one"
        );
        // The distinction the assertion above cannot make on its own: the same
        // count through the setting-gated helper is `None`, so if the Dashboard
        // ever used it for the empty case the user would see no explanation.
        assert_eq!(skipped_summary(6, false), None);
        assert_ne!(all_rows_failed_copy(6), skipped_summary(6, true).unwrap());
    }

    /// Both Dashboard surfaces must report an all-rows-unreadable list as a
    /// parse failure, never as an empty account. These take no `show_skipped`
    /// argument by construction, so no setting can suppress them — the earlier
    /// shape of this code had the branch inline in the view, where gating it
    /// behind the preference compiled and passed every test.
    #[test]
    fn dashboard_surfaces_never_call_an_unreadable_list_empty() {
        // The lie, spelled out: this is the string the user must NOT see.
        let lie = crate::fl!("dash-no-containers-configured");
        assert_ne!(dashboard_status_body(0, 0, 6), lie);
        assert!(dashboard_status_body(0, 0, 6).contains("6"));
        // A genuinely empty account still gets the advice.
        assert_eq!(dashboard_status_body(0, 0, 0), lie);
        // A working list is described by its counts, not by the skip count.
        assert_eq!(
            dashboard_status_body(2, 5, 3),
            crate::fl!("dash-status-running", running = 2, total = 5)
        );
        assert_eq!(
            dashboard_status_body(1, 1, 0),
            crate::fl!("dash-status-running", running = 1, total = 1)
        );
        assert!(
            dashboard_status_body(2, 5, 3).contains('2')
                && dashboard_status_body(2, 5, 3).contains('5'),
            "both counts survive the move into the catalogue"
        );

        let (icon, title, body) = dashboard_preview_copy(6);
        assert_eq!(icon, "dialog-warning-symbolic");
        assert_eq!(title, crate::fl!("dash-preview-unreadable-title"));
        assert!(body.contains("6"));
        assert!(!body.contains(&crate::fl!("dash-preview-empty-body")));
        // And the clean case is unchanged from what T6 shipped.
        let (icon, title, body) = dashboard_preview_copy(0);
        assert_eq!(icon, "document-open-symbolic");
        assert_eq!(title, crate::fl!("dash-preview-empty-title"));
        assert_eq!(body, crate::fl!("dash-preview-empty-body"));
        // Both surfaces agree: one skip count, one verdict.
        assert_eq!(dashboard_status_body(0, 0, 6), all_rows_failed_copy(6));
        assert_eq!(body_of(dashboard_preview_copy(6)), all_rows_failed_copy(6));
    }

    fn body_of(copy: (&'static str, String, String)) -> String {
        copy.2
    }
}
