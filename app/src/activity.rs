//! Activity log page (T11, ux.md §6.12, rows #152–#162).
//!
//! Header clear-completed (#152, `header_end` — Flutter's Refresh was a
//! bare `setState(() {})` with nothing to reload: the mirror updates live
//! via subscription, so no Refresh is wired), search over
//! description + output (#153), filter chips All/Running/Success/Errors
//! (#154 — suggested/standard button pair with selected state, matching
//! the #107 tab precedent; `segmented_control` exists but needs a
//! `Model<SelectionMode>` carried on `App` for a 4-way single-select —
//! the pair is one tab/chip idiom across pages with no new state),
//! stats bar
//! Total/Running/Completed/Failed (#155), empty states (#156), timeline
//! rows with status icon + description + relative time + status label +
//! last-line preview (#157 — no coloured pill: coloured `Text` is
//! structurally unavailable in this iced rev, icons.rs T6 note),
//! full-output drawer via `context_drawer`
//! (#158 — the draggable 0.5–0.95 resize range is lost, accepted), severity
//! in output (#159 — body vs caption lines, same rule as the wizard
//! console), TaskState from the mirror — NEVER string-sniffing (#160),
//! in-memory only (#161 — persistence is T12 config scope, not this page),
//! relative times (#162 — no Fluent dependency; plain English rules).

use crate::app::TaskView;
use crate::message::{ActivityMsg, Message, TaskMsg};
use crate::views::empty_state;
use cosmic::iced::Length;
use cosmic::widget;
use gosh_distrobox_core::TaskId;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::time::Instant;

/// Task state — the real enum that kills string-sniffing (row #160, T11
/// title deliverable). Derived ONLY from the T5 mirror fields
/// (`completed`/`success`), never from output text. Flutter derived it
/// three inconsistent ways (`TaskInfo.failed` on any "error"/"failed" line,
/// last-line sniff for "completed"/"failed", `isTaskRunning` polling);
/// all three are gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Success,
    Failed,
}

impl TaskState {
    pub fn of(view: &TaskView) -> Self {
        if !view.completed {
            TaskState::Running
        } else if view.success {
            TaskState::Success
        } else {
            TaskState::Failed
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TaskState::Running => "In Progress",
            TaskState::Success => "Completed",
            TaskState::Failed => "Failed",
        }
    }
}

/// Activity filter (row #154).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActivityFilter {
    #[default]
    All,
    Running,
    Success,
    Errors,
}

/// Activity page state. Lives on `App` (row #7).
#[derive(Clone, Debug, Default)]
pub struct ActivityState {
    pub search: String,
    pub filter: ActivityFilter,
    /// Task id with the full-output drawer open (row #158).
    pub expanded: Option<TaskId>,
}

/// Filter + sort tasks (row #153–#154, newest first — Flutter parity).
/// Search covers description + output; filter uses `TaskState`, never text.
pub fn filter_tasks<'a>(
    tasks: &'a BTreeMap<TaskId, TaskView>,
    search: &str,
    filter: ActivityFilter,
) -> Vec<(TaskId, &'a TaskView)> {
    let q = search.to_lowercase();
    let mut out: Vec<(TaskId, &'a TaskView)> = tasks
        .iter()
        .map(|(id, v)| (*id, v))
        .filter(|(_, v)| {
            if !q.is_empty()
                && !(v.label.to_lowercase().contains(&q)
                    || v.output.iter().any(|l| l.to_lowercase().contains(&q)))
            {
                return false;
            }
            match filter {
                ActivityFilter::All => true,
                ActivityFilter::Running => TaskState::of(v) == TaskState::Running,
                ActivityFilter::Success => TaskState::of(v) == TaskState::Success,
                ActivityFilter::Errors => TaskState::of(v) == TaskState::Failed,
            }
        })
        .collect();
    out.sort_by_key(|(_, v)| Reverse(v.started_at));
    out
}

/// Stats bar counts (row #155) from `TaskState`.
pub fn stats(tasks: &BTreeMap<TaskId, TaskView>) -> (usize, usize, usize, usize) {
    let mut running = 0;
    let mut success = 0;
    let mut failed = 0;
    for v in tasks.values() {
        match TaskState::of(v) {
            TaskState::Running => running += 1,
            TaskState::Success => success += 1,
            TaskState::Failed => failed += 1,
        }
    }
    (tasks.len(), running, success, failed)
}

/// Relative time (row #162): Just now / Nm ago / Nh ago / Nd ago.
/// (Flutter fell through to `MMM d, yyyy` past 7 days; without a date
/// dependency this renders `Nd ago` instead — T15 may promote it.)
pub fn relative_time(started_at: Instant) -> String {
    let secs = started_at.elapsed().as_secs();
    if secs < 60 {
        "Just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        // 24h+: day count (no date-format dependency; T15 may promote to a
        // Fluent date message).
        format!("{}d ago", secs / 86400)
    }
}

/// Timeline row (row #157): status icon, description, relative time, status
/// label, last-line preview. Output button on EVERY row (running included
/// — watching a task in flight is the case that matters most); Cancel
/// additionally on running rows.
pub fn timeline_row(id: TaskId, view: &TaskView) -> cosmic::Element<'static, Message> {
    let state = TaskState::of(view);
    let icon = match state {
        TaskState::Running => "process-working-symbolic",
        TaskState::Success => "object-select-symbolic",
        TaskState::Failed => "dialog-error-symbolic",
    };
    let preview = view.output.last().cloned().unwrap_or_default();
    widget::Column::new()
        .push({
            let head: cosmic::Element<'static, Message> = widget::Row::new()
                .push({
                    let ic: cosmic::Element<'static, Message> =
                        widget::icon::from_name(icon).size(20).icon().into();
                    ic
                })
                .push(
                    widget::Column::new()
                        .push(widget::text::body(view.label.clone()))
                        .push(widget::text::caption(format!(
                            "{} · {}",
                            relative_time(view.started_at),
                            state.label()
                        )))
                        .spacing(2)
                        .width(Length::Fill),
                )
                .push(
                    widget::button::text("Output")
                        .on_press(Message::Activity(ActivityMsg::Expanded(id))),
                )
                .push_maybe(if state == TaskState::Running {
                    Some(
                        widget::button::text("Cancel")
                            .on_press(Message::Tasks(TaskMsg::CancelRequested(id))),
                    )
                } else {
                    None
                })
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .into();
            head
        })
        .push(widget::text::caption(preview))
        .spacing(4)
        .into()
}

/// Full-output drawer body (row #158): severity-split lines (row #159).
pub fn output_drawer(view: &TaskView) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new()
        .push(widget::text::title3(view.label.clone()))
        .push(widget::text::caption(format!(
            "{} · {}",
            relative_time(view.started_at),
            TaskState::of(view).label()
        )))
        .spacing(8);
    if view.output.is_empty() {
        col = col.push(widget::text::caption("No output yet."));
    } else {
        for line in &view.output {
            let lower = line.to_lowercase();
            if lower.contains("error") || lower.contains("failed") {
                col = col.push(widget::text::body(line.clone()));
            } else {
                col = col.push(widget::text::caption(line.clone()));
            }
        }
    }
    widget::scrollable(col).into()
}

/// Full activity page body.
pub fn view_activity(
    tasks: &BTreeMap<TaskId, TaskView>,
    state: &ActivityState,
) -> cosmic::Element<'static, Message> {
    let mut col = widget::Column::new().spacing(12);
    // Search (#153).
    col = col.push({
        let search: cosmic::Element<'static, Message> =
            widget::text_input::search_input("Search logs...", state.search.clone())
                .on_input(|s| Message::Activity(ActivityMsg::SearchChanged(s)))
                .into();
        search
    });
    // Filter chips (#154).
    col = col.push({
        let chips: cosmic::Element<'static, Message> = widget::Row::new()
            .push(filter_chip(
                "All".to_string(),
                ActivityFilter::All,
                state.filter,
            ))
            .push(filter_chip(
                "Running".to_string(),
                ActivityFilter::Running,
                state.filter,
            ))
            .push(filter_chip(
                "Success".to_string(),
                ActivityFilter::Success,
                state.filter,
            ))
            .push(filter_chip(
                "Errors".to_string(),
                ActivityFilter::Errors,
                state.filter,
            ))
            .spacing(8)
            .into();
        chips
    });
    // Stats bar (#155).
    let (total, running, success, failed) = stats(tasks);
    col = col.push({
        let bar: cosmic::Element<'static, Message> = widget::Row::new()
            .push(stat_cell("Total".to_string(), total))
            .push(stat_cell("Running".to_string(), running))
            .push(stat_cell("Completed".to_string(), success))
            .push(stat_cell("Failed".to_string(), failed))
            .spacing(16)
            .into();
        bar
    });
    // Timeline (#156–#157).
    let filtered = filter_tasks(tasks, &state.search, state.filter);
    if filtered.is_empty() {
        col = col.push(empty_state(
            "document-open-symbolic",
            if tasks.is_empty() {
                "No activity yet".to_string()
            } else {
                "No matching activities".to_string()
            },
            if tasks.is_empty() {
                "Your container operations will appear here".to_string()
            } else {
                "Try adjusting your search or filters".to_string()
            },
            None,
        ));
    } else {
        for (id, view) in filtered {
            col = col.push(timeline_row(id, view));
        }
    }
    widget::scrollable(col).into()
}

fn filter_chip(
    label: String,
    filter: ActivityFilter,
    active: ActivityFilter,
) -> cosmic::Element<'static, Message> {
    let btn = if active == filter {
        widget::button::suggested(label)
    } else {
        widget::button::standard(label)
    };
    btn.on_press(Message::Activity(ActivityMsg::FilterSelected(filter)))
        .into()
}

fn stat_cell(label: String, count: usize) -> cosmic::Element<'static, Message> {
    widget::Column::new()
        .push(widget::text::title3(count.to_string()))
        .push(widget::text::caption(label.to_string()))
        .spacing(2)
        .align_x(cosmic::iced::Alignment::Center)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn view(label: &str, completed: bool, success: bool) -> TaskView {
        TaskView {
            label: label.into(),
            output: vec![],
            completed,
            success,
            started_at: Instant::now(),
        }
    }

    #[test]
    fn state_never_sniffs_output() {
        // Row #160: a FAILED line in the output of an incomplete task must
        // NOT flip it to Failed; a "completed" line in a failed task must
        // NOT flip it to Success. Only the mirror fields classify.
        let mut running = view("Upgrade x", false, false);
        running.output.push("Error: something failed".into());
        assert_eq!(TaskState::of(&running), TaskState::Running);
        let mut failed = view("Install y", true, false);
        failed.output.push("Task completed successfully".into());
        assert_eq!(TaskState::of(&failed), TaskState::Failed);
        assert_eq!(TaskState::of(&view("z", true, true)), TaskState::Success);
    }

    #[test]
    fn filter_search_and_sort() {
        let now = Instant::now();
        let mut tasks = BTreeMap::new();
        let mut mk = |label: &str, completed: bool, success: bool, secs_ago: u64| {
            let id = TaskId::new();
            tasks.insert(
                id,
                TaskView {
                    label: label.into(),
                    output: vec![format!("out {label}")],
                    completed,
                    success,
                    started_at: now - Duration::from_secs(secs_ago),
                },
            );
            id
        };
        mk("Upgrade a", false, false, 10);
        mk("Install b", true, true, 20);
        mk("Remove c", true, false, 5);
        // Newest first.
        let all = filter_tasks(&tasks, "", ActivityFilter::All);
        assert_eq!(all[0].1.label, "Remove c");
        // Filter by state (not text).
        assert_eq!(filter_tasks(&tasks, "", ActivityFilter::Running).len(), 1);
        assert_eq!(filter_tasks(&tasks, "", ActivityFilter::Success).len(), 1);
        assert_eq!(filter_tasks(&tasks, "", ActivityFilter::Errors).len(), 1);
        // Search covers description + output.
        assert_eq!(
            filter_tasks(&tasks, "upgrade", ActivityFilter::All).len(),
            1
        );
        assert_eq!(
            filter_tasks(&tasks, "install b", ActivityFilter::All).len(),
            1
        );
    }

    #[test]
    fn relative_times() {
        let now = Instant::now();
        assert_eq!(relative_time(now), "Just now");
        assert_eq!(relative_time(now - Duration::from_secs(300)), "5m ago");
        assert_eq!(relative_time(now - Duration::from_secs(7200)), "2h ago");
        assert_eq!(
            relative_time(now - Duration::from_secs(3 * 86400)),
            "3d ago"
        );
    }

    #[test]
    fn stats_count_by_state() {
        let mut tasks = BTreeMap::new();
        tasks.insert(TaskId::new(), view("a", false, false));
        tasks.insert(TaskId::new(), view("b", true, true));
        tasks.insert(TaskId::new(), view("c", true, false));
        let (total, running, success, failed) = stats(&tasks);
        assert_eq!((total, running, success, failed), (3, 1, 1, 1));
    }
}
