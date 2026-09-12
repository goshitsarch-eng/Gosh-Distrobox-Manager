//! Bounded completed-task history ring (T20, row #161).
//!
//! ux.md:266 classes task-history persistence P1: the Activity page showed the
//! live session only (500-line cap, lost on exit). Completed tasks now persist
//! as a bounded ring in cosmic-config alongside the existing prefs (D11) — one
//! more per-key file (`task_history`), read with the same degrade-don't-crash
//! rule: corrupt or absent reads as an empty ring, never a crash.
//!
//! Why cosmic-config and not an xdg-data file: measured, no cause found for a
//! separate file. The ring is 50 entries × 20 tail lines of short strings
//! (tens of KB worst case), the per-key atomic write already exists, and the
//! prefs load path already handles corrupt keys. A separate file would need
//! its own atomic-write + corrupt-handling for no measured gain.
//!
//! What an entry does NOT carry: the task's routing discriminant (`TaskKind`
//! lives in `app/`, and core takes no libcosmic dep). Seeded views render as
//! `Other`, which is strictly more than before (previously a restarted task
//! was gone entirely) and changes no routing — routing only matters for live
//! tasks. The label, outcome, finish time and output preview all survive.

use serde::{Deserialize, Serialize};

/// Maximum persisted completed tasks. The 51st completion evicts the oldest
/// entry — the ring never grows past this, so the config key stays small.
pub const HISTORY_BOUND: usize = 50;

/// Entry schema version. Bump when `HistoryEntry` gains or loses a field;
/// [`migrate_history`] drops entries written by a newer (unknown) schema and
/// stamps older ones current.
pub const HISTORY_SCHEMA: u32 = 1;

/// Persisted output preview per entry: the LAST N lines, not the 500-line
/// ring. The preview is what the Activity timeline shows (`last-line` plus
/// drawer context); the full log was always session-scoped.
pub const HISTORY_TAIL_LINES: usize = 20;

/// One completed task, as persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Schema version this entry was written with. `#[serde(default)]` so
    /// JSON written before the field existed (schema 0) still parses and
    /// migrates forward instead of failing the whole key.
    #[serde(default)]
    pub schema: u32,
    /// User-visible description (already translated at persist time — the
    /// entry is a record, not a message id).
    pub label: String,
    /// Task outcome. Never sniffed from output — the mirror's own flag.
    pub success: bool,
    /// Completion time as `SystemTime` seconds. A persisted clock is needed
    /// because `Instant` cannot cross a restart; seeding converts back to an
    /// `Instant` by subtracting the elapsed span from now.
    pub finished_unix: u64,
    /// Last output lines (preview, oldest-first). `#[serde(default)]` so a
    /// future field addition the other way (a v2 reader over v1 JSON missing
    /// a new field) degrades to empty rather than failing the key.
    #[serde(default)]
    pub tail: Vec<String>,
}

impl HistoryEntry {
    /// Current time in persisted-clock seconds. `0` when the system clock is
    /// before the epoch (seeding then treats the entry as just finished).
    pub fn now_unix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Build from a finished task's mirror fields, capping the preview to
    /// the last [`HISTORY_TAIL_LINES`] lines (oldest-first, as displayed).
    pub fn new(label: String, success: bool, output: &[String]) -> Self {
        let tail = if output.len() > HISTORY_TAIL_LINES {
            output[output.len() - HISTORY_TAIL_LINES..].to_vec()
        } else {
            output.to_vec()
        };
        Self {
            schema: HISTORY_SCHEMA,
            label,
            success,
            finished_unix: Self::now_unix(),
            tail,
        }
    }
}

/// Push an entry, evicting the oldest first past [`HISTORY_BOUND`]. The ring
/// is oldest-first, so eviction drains from the front.
pub fn push_history(ring: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    ring.push(entry);
    if ring.len() > HISTORY_BOUND {
        let drain_to = ring.len() - HISTORY_BOUND;
        ring.drain(0..drain_to);
    }
}

/// Schema migration at the disk→memory choke point: drop entries written by
/// a NEWER schema (unreadable by definition — keeping them would surface
/// fields this build cannot interpret) and stamp older ones current so a
/// re-save normalises the key.
pub fn migrate_history(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    entries
        .into_iter()
        .filter(|e| e.schema <= HISTORY_SCHEMA)
        .map(|mut e| {
            e.schema = HISTORY_SCHEMA;
            e
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> HistoryEntry {
        HistoryEntry {
            schema: HISTORY_SCHEMA,
            label: label.into(),
            success: true,
            finished_unix: 1_700_000_000,
            tail: vec![],
        }
    }

    #[test]
    fn ring_evicts_oldest_beyond_bound() {
        let mut ring = Vec::new();
        for i in 0..HISTORY_BOUND + 2 {
            push_history(&mut ring, entry(&format!("task {i}")));
        }
        assert_eq!(
            ring.len(),
            HISTORY_BOUND,
            "the ring never grows past the bound"
        );
        assert_eq!(
            ring.first().unwrap().label,
            "task 2",
            "eviction is oldest-first"
        );
        assert_eq!(
            ring.last().unwrap().label,
            format!("task {}", HISTORY_BOUND + 1),
            "the newest entry is always kept"
        );
    }

    #[test]
    fn migrate_drops_future_schema_and_stamps_current() {
        let mut future = entry("future");
        future.schema = HISTORY_SCHEMA + 98;
        let mut legacy = entry("legacy");
        legacy.schema = 0;
        let current = entry("current");
        let got = migrate_history(vec![future, legacy, current]);
        assert_eq!(
            got.len(),
            2,
            "the future-schema entry is unreadable and dropped"
        );
        assert!(
            got.iter().all(|e| e.schema == HISTORY_SCHEMA),
            "survivors are stamped current so a re-save normalises the key: {got:?}"
        );
        assert_eq!(got[0].label, "legacy");
        assert_eq!(got[1].label, "current");
    }

    #[test]
    fn json_missing_newer_fields_still_parses() {
        // The forward-compat contract: adding a field with `#[serde(default)]`
        // must not break reading JSON written before it existed. Simulate a
        // pre-`tail`/pre-`schema` writer by omitting both fields.
        let raw = r#"{"label":"Upgrade demo","success":false,"finished_unix":1700000000}"#;
        let parsed: HistoryEntry = serde_json::from_str(raw).expect("older JSON parses");
        assert_eq!(parsed.label, "Upgrade demo");
        assert!(!parsed.success);
        assert!(parsed.tail.is_empty());
        let migrated = migrate_history(vec![parsed]);
        assert_eq!(migrated[0].schema, HISTORY_SCHEMA);
    }

    #[test]
    fn new_keeps_the_last_tail_lines_in_order() {
        let output: Vec<String> = (0..HISTORY_TAIL_LINES + 5)
            .map(|i| format!("line {i}"))
            .collect();
        let got = HistoryEntry::new("label".into(), true, &output);
        assert_eq!(got.tail.len(), HISTORY_TAIL_LINES);
        assert_eq!(
            got.tail.first().unwrap(),
            "line 5",
            "the preview is the tail, not the head"
        );
        assert_eq!(
            got.tail.last().unwrap().as_str(),
            format!("line {}", HISTORY_TAIL_LINES + 4).as_str(),
            "the newest line is always kept"
        );
        // Short output passes through whole.
        let short = vec!["only".to_string()];
        assert_eq!(HistoryEntry::new("s".into(), false, &short).tail, short);
    }

    #[test]
    fn entry_serde_round_trip() {
        let before = HistoryEntry {
            schema: HISTORY_SCHEMA,
            label: "Upgrade demo".into(),
            success: false,
            finished_unix: 1_700_000_000,
            tail: vec!["a".into(), "b".into()],
        };
        let json = serde_json::to_string(&before).expect("serializes");
        let after: HistoryEntry = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(before, after);
    }
}
