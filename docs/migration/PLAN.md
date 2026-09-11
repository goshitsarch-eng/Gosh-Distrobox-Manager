# Migration Plan — Flutter → libcosmic

**App:** Gosh Distrobox Manager (`io.github.gosh_distrobox_manager`), GPL-3.0-or-later.
**Branch:** `cosmic-migration`.
**Nature of the work:** a UI-toolkit swap (Flutter/Dart → libcosmic/Rust) plus deleting the
`flutter_rust_bridge` FFI; the Rust backend moves with history and is fixed, not rewritten.

This document is the single source of truth for Phase 2 and Phase 3. It consolidates
[`ux.md`](ux.md), [`architecture.md`](architecture.md), [`packaging.md`](packaging.md) and the
adversarial [`REVIEW.md`](REVIEW.md). Every decision it depends on is recorded in
[`DECISIONS.md`](DECISIONS.md) (D1–D21).

## What is actually changing

Of ~4,100 lines of hand-written Rust, **~3,500 do not change behaviour at all**.
`backends/distrobox/distrobox.rs` (2,139 lines — every distrobox/podman/docker command),
`fakers/` (the `CommandRunner` + `NullCommandRunner` test harness), `flatpak.rs`,
`host_exec.rs`, `desktop_file.rs`, and `known_distros.rs` contain zero FRB imports
(verified by grep) and move verbatim, plus the nine §6.4 behaviour fixes. The migration
rewrites the ~8.9k-line Dart UI as libcosmic pages, replaces the FRB bridge with a
`Backend` + `Message`/`update`/`Subscription` design, and designs the config persistence
that never existed.

| Area | Disposition |
|---|---|
| `backends/`, `fakers/`, `models/` (T1) | Move verbatim via `git mv rust core`; §6.4 fixes applied (D7) |
| `models/task.rs`, `app_state.rs`, `supported_terminals.rs`, runtime trio (T2) | Adapt per §6.2 (D7, D8, D11) |
| `api.rs`, `frb_generated.rs`, `flutter_rust_bridge.yaml`, `lib/src/rust/*.dart` (T3) | Delete (S7); replaced by `core::service::Backend` + `CoreError` + `TaskId` |
| `lib/`, `test/`, `pubspec.yaml`, platform dirs, 18 MB tarball | Delete in S8 (D3); pre-deletion commit tagged (E13) |
| `app/` (new bin crate) | All 8 pages + wizard + dialogs, per ux.md §2 + the 193-row checklist |
| `flatpak/`, `scripts/verify.sh`, CI | New, from the sibling's proven tooling (D12, D13, D21) |
| `.spec`, `build-rpm.sh`, `AGENTS.md`, `README.md`, `.github/` | Rewrite (tasks T13–T14) |

---

## Status

Updated as tasks close. A task is closed only when: `cargo build`, `cargo clippy
--all-targets -- -D warnings` and `cargo test` all pass; the Flatpak gate is met per D13;
the relevant parity items are ticked *with their verification tier*; the reviewer has
signed off on the diff; and it is committed.

| Phase | State |
|---|---|
| Phase 1 — Plan | **Complete.** All three docs signed off with changes, 45 questions answered (38) or recorded (7), 21 decisions recorded. |
| Phase 2 — Build | **Not started.** T0–T4 unblocked; T5+ sequenced below. |
| Phase 3 — Harden | Not started. |

| # | Task | Owner | State |
|---|---|---|---|
| T0 | Rust CI stage + `.github` cleanup (fmt/clippy/test/`--locked`, `submodules: recursive`, `--check`) | pkg | Queued |
| T1 | S1–S4: workspace scaffold, `git mv rust core`, DTO + task-runtime moves, `clippy.toml` guard | arch | Queued |
| T2 | Port `generate-cargo-sources.py` + sidecars; `flatpak/` skeleton + manifest (D12 feature set) | pkg | Queued |
| T3 | S5 skeleton app + S6 `Backend`/`CoreError`/`TaskId` + read-only browser (containers/images/apps/stats) | arch | Queued |
| T4 | `scripts/verify.sh` v1 + `scripts/smoke-test.sh` with readiness signal (D13, D17) | pkg | Queued |
| T5 | Task runtime + `spawn_task` + per-task subscriptions (B2 line buffering lands here) | arch | Queued |
| T6 | Dashboard + Containers + Details pages | ux | Queued |
| T7 | Create wizard + Images pages (wire `preselectedImage`) | ux | Queued |
| T8 | Package manager page (B1 emerge/table fix) | arch + ux | Queued |
| T9 | Updates + Terminal-launch pages (B5 `start` lands before banners freeze) | arch + ux | Queued |
| T10 | Backups page (snapshots/export/import/clone; xdg-portal picker per D16) | arch + ux | Queued |
| T11 | Activity log + TaskState enum (kills string-sniffing) | arch + ux | Queued |
| T12 | Apps export page + Settings/about + config persistence (one-time DistroShelf import) | arch + ux | Queued |
| T13 | B3 tolerant parsing + `show_skipped_lines`; B4 field codes; B7 runtime helper | arch | Queued |
| T14 | S7 + S8 deletions (tagged pre-deletion commit) + spec/scripts/AGENTS/README/CI rewrite | arch + pkg | Queued |
| T15 | i18n extraction pass 1 (per-screen Fluent strings; user-data placeables rule) | ux | Queued |
| T16 | Harden: full parity walk in the running Flatpak, break-every-flow, file new tasks | reviewer | Queued |

Dependency graph: T0→∅ (first); T1→T0; T2→T1; T3→T1; T4→{T2,T3}; T5→T3; T6→T5; T7→T5;
T8→T5; T9→T5; T10→T5; T11→T5; T12→{T9,T10}; T13→T3; T14→{T6..T13}; T15→{T6..T12};
T16→T14. T6–T11 parallelise across build agents after T5.

## §5.7 Endgame checklist (project-level done criteria)

- [ ] Every one of the 193 parity rows ticked with a verification tier (D19)
- [ ] `scripts/verify.sh` passes from a clean checkout (D13 gates)
- [ ] Phase 3 pass clean, no remaining failures (T16)
- [ ] `docs/migration/REPORT.md` written (build/run/install, deviations, limitations)
- [ ] Residual risks R1–R7 each have a named owner or an accepted disposition

---

## 1. Pinned constants

| Component | Pin | Verified |
|---|---|---|
| libcosmic | `rev = "a401af8b1c54a8abd393b8c5b7c8809402f83850"` (v1.0.0, 2026-09-10) | ux/arch/packaging + REVIEW §B.a |
| → submodule `iced` | `pop-os/iced` @ `ffe1f1dbe3cbfd313f9b5fe8e36a4af462cae5d7` | packaging §1.3 |
| → submodule `cosmic-icons` | `pop-os/cosmic-icons` @ `343c007f37cd71716e68f01c43ecf2764b7f7c47` | packaging §1.3 |
| Further git pointers | `pop-os/dbus-settings-bindings`, `freedesktop-icons` (always compiled, never feature-gated) | REVIEW A8; sidecar covers all five (D12) |
| Toolchain | rustc/cargo 1.98.1 ≥ libcosmic MSRV 1.93 | packaging §0 |
| Runtime/SDK | freedesktop 25.08 + rust-stable extension | packaging §0 |

**Single libcosmic feature list** (D12 — resolves the arch §1.3 vs packaging §1.3
contradiction; written here, referenced by both docs):
`["winit", "tokio", "a11y", "wayland", "x11", "multi-window", "dbus-config", "about",
"xdg-portal"]`, with `default-features = false`.
Rationale per flag: `winit`/`tokio` (executor + windowing); `a11y` (P0 screen-reader
path, REVIEW UX-16); `wayland`/`x11` (both display backends); `multi-window` (kept from
default; page-stack stays single-window per REVIEW §B.f, back affordance mandatory);
`dbus-config` (kept; sibling D27b dropped it, but that app *edits* COSMIC settings and
needed no live watch — ours watches its own config keys via `watch_config`, and the
sibling measured the failure mode as log noise, not breakage); `about`
(`widget::about()`); `xdg-portal` (the only sandbox file-chooser route, D16).
Never: `applet`, `desktop` (unpinned `cosmic-panel-config`/`cosmic-settings-config`).

**Binary name** (D15): `gosh_distrobox_manager` (underscores) everywhere Cargo/desktop
controls it — `app/Cargo.toml` package + `[[bin]]`, manifest `command:`,
`Exec=gosh_distrobox_manager`, `.service.in` (deleted anyway per T14), and the spec's
`_bindir` entry. The `.spec` `Name:` stays `gosh-distrobox-manager` (RPM hyphen
convention; `%{name}` ≠ binary name).

## 2. Feature parity checklist — 193 rows, frozen

Keep all 193 rows (D19). The checklist lives in [`ux.md` §6](ux.md) (ids 1–193,
contiguous); this section records the review-mandated corrections and the status
summary. Counts after correction: **exists 148 · dead 15 · dup 14 · missing 13 ·
bug/fragile 3**.

Corrections applied before freezing (REVIEW §G.2-4): `_getDistroIcon` is in **9** files
(not 8); status helpers are **2 named + logic duplicated under other names in 4 files**;
`AlertDialog` appears **23×** (not 11); PNG total is **3.6 MB across 65 files** (not
~5 MB). Row #88 re-scoped: portal backend has no `directory()`/`file_name()` — prefilled
export names are undeliverable (D16). Row #68/#130 start-banners gated on B5 `start`
(D7). `block_io` ships as the free disk-adjacent column (D18).

The 15 `dead` rows are the deliverable: #23, #28, #29, #68, #88, #95, #102, #103, #122,
#130, #146, #147, #167, #180, #181 — each must become live, re-scoped, or explicitly
dropped per §4, never silently ported.

## 3. Risk list

Blocking pre-conditions (REVIEW §G.1) — all closed by D-decisions above: one feature
list (D12), a11y grant (D12), file picker (D16), binary name (D15), terminals into
cosmic-config (D11), `.github` cleanup (T0/T14), `start` sequencing (T9 + D7).

Residual risks and owners:

| # | Risk | Owner | Disposition |
|---|---|---|---|
| R1 | Side-by-side container comparison lost | lead | Accept; single-window + back stack (REVIEW §B.f) |
| R2 | `disk_usage` scope | lead | Drop mock; ship `block_io` column free (D18) |
| R3 | Brand accent `#137FEC` + Inter lost | lead | Accept; theme roles + system font, brand lives in icon (D18) |
| R4 | ~600-string i18n schedule load | ux | Per-screen extraction during port (T15); never a separate pass |
| R5 | Intended-changes ownership | lead | Table below; lead signs off each row |
| R6 | RPM spec drift | pkg | Maintain in lockstep; `check-versions.sh` gate (D14) |
| R7 | `Message` test bar | arch | Behavioural tests + exhaustive match, not per-variant (D13) |
| R8–R13 | Accepted technical notes (submodule drift, poll-only, registry duality, tarball history, GSettings import, Orca manual step) | lead | Recorded in DECISIONS.md D20 |

## 4. Intended changes vs Flutter (owner: lead)

| # | Change | Parity rows | Sign-off |
|---|---|---|---|
| I1 | 8 dead quick actions become live (#23, #28, #29, #88, #95, #102, #103, #167) | dead ×8 | ☐ |
| I2 | Distro icons unified across pages (fix 5-branch/8-branch divergence) | #184 | ☐ |
| I3 | Real failure feedback (toaster) replaces silent no-ops | #122, #147 | ☐ |
| I4 | Disabled buttons instead of silent validation returns | #90, #140, #146 | ☐ |
| I5 | `widget::about()` replaces the About card | #169 | ☐ |
| I6 | Theme roles replace `Colors.*`; system font replaces Inter | #186 | ☐ |
| I7 | `distrobox_source="bundled"` key dropped (unimplementable) | #171-adjacent | ☐ |
| I8 | `task_page` stub and `disk_usage` mock dropped | #180, #181 | ☐ |
| I9 | Pull-to-refresh, bottom nav, long-press dropped (touch idioms) | #3, #32, #42 | ☐ |
| I10 | `block_io` column ships as the free disk-adjacent figure | A2/D18 | ☐ |
| I11 | File-picker rows re-scoped to portal capabilities (no prefilled names) | #88, #143, #144 | ☐ |
| I12 | `TaskState` enum replaces output string-sniffing | #160 | ☐ |

## 5. Ordered task list (app stays buildable after every task)

Sequenced from architecture.md §7 as amended by REVIEW §§B–C/G: CI first (E8), `start`
in the backend group before banner copy freezes (UX-5), B2 line-buffering inside T5
(not later), S3 before S7, S8 deletions tagged and enumerated. The table in Status
(above) is the task list; the dependency graph there is normative.

Verification tiers (D19): **T1** unit (`cargo test`), **T2** integration
(`NullCommandRunner` + messages), **T3** running-Flatpak observation. T1+T2 gate every
task; T3 gates page tasks (T6–T12) and the release.
