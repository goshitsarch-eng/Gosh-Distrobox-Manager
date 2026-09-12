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
| Phase 2 — Build | **Underway.** T0–T12 done; T13–T15 sequenced below (T13→T3, T14→{T6..T13}, T15→{T6..T12}); T16 gates Phase 3. |
| Phase 3 — Harden | Not started. |

| # | Task | Owner | State |
|---|---|---|---|
| T0 | Rust CI stage + doc-side G.1/G.2 set + `AGENTS.md` header + `release.prompt.md` fix (9 clippy errors fixed or scoped-allowlisted per D22; `dbus-config` dropped per D23; G.1-6 `flatpak.yml` deferred to T14, recorded partial; sign-off rulings D24) | pkg | **Done** (`a1c2556`; gates green, 70 passed; + D24 commit) |
| T1 | S1–S4: workspace scaffold, `git mv rust core`, DTO + task-runtime moves, `clippy.toml` guard | arch | **Done** (`f2ffa5c`; 70+1 tests; D25) |
| T2 | Port `generate-cargo-sources.py` + sidecars; `flatpak/` skeleton + manifest (D12 feature set) | pkg | **Done** (gates per D26: `--check` exit 0, surrogate offline build EXIT=0, sidecar 11/51 validated; rulings D26) |
| T3 | S5 skeleton app + S6 `Backend`/`CoreError`/`TaskId` + read-only browser (containers/images/apps/stats) + regenerate `cargo-sources.json` for the `app/` lockfile (plain generate; `--refresh-git` only if git set differs — D26) | arch | **Done** (`76b69fe`; 79 core + 6 app + 1 doctest; `CoreFailure` Arc-wrapper; pins dirs 6/thiserror 2; cargo-sources 1385, `--check` green, surrogate `--locked` build clean) |
| T4 | `scripts/verify.sh` v1 + `scripts/smoke-test.sh` with readiness signal (D13, D17) | pkg | **Done** (`e50923c`; verify.sh EXIT=0 end-to-end: 79+0+6+1 tests, 17/17 packaging, offline build+install, both smoke modes PASS; D26b/c/d closed; advocate no-blocking-objections with 6 findings fixed) |
| T5 | Task runtime + `spawn_task` + per-task subscriptions (B2 line buffering lands here) | arch | **Done** (`042646c`; 94+0+6+4+1 green; advocate 4 blocking objections fixed + re-probed, signed off) |
| T6 | Dashboard + Containers + Details pages | ux | **Done** (`ad984d5`; 94+3+6+6+4+1 green; advocate 2 blocking + 9 secondary fixed, signed off) |
| T7 | Create wizard + Images pages (wire `preselectedImage`) | ux | **Done** (`abc1629`; 94+7+4+6+6+4+1 green; advocate O1–O6 + cancel-strand fixed, signed off) |
| T8 | Package manager page (B1 emerge/table fix) | arch + ux | **Done** (`0a89a78`; 98+8+4+6+4+6+4+1 green; advocate 1 blocking + 9 secondary fixed, signed off) |
| T9 | Updates + Terminal-launch pages (B5 `start` lands before banners freeze) | arch + ux | **Done** (`e2845d4`; 99+8+5+4+6+4+6+4+1 green; advocate O1–O6 + strand/latch rounds fixed, signed off) |
| T10 | Backups page (snapshots/export/import/clone; xdg-portal picker per D16) | arch + ux | **Done** (`48160a0`; 99+9+5+4+6+4+6+4+5+1 green; advocate 3 blocking + guard gap fixed, signed off) |
| T11 | Activity log + TaskState enum (kills string-sniffing) | arch + ux | **Done** (`fa507ce`; 99+13+5+4+6+4+6+4+5+1 green; advocate 5 + doc rounds fixed, signed off) |
| T12 | Apps export page + Settings/about + config persistence (one-time DistroShelf import) | arch + ux | **Done** (`59ef837`; 109+18+5+5+4+6+4+6+4+5+1 green; advocate 7 (2 HIGH) + re-review 5 fixed, signed off; `verify.sh` 11/11 stages) |
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

**Single libcosmic feature list** (D12 as amended by D23 — resolves the arch §1.3 vs
packaging §1.3 contradiction; written here, referenced by both docs):
`["winit", "tokio", "a11y", "wayland", "x11", "multi-window", "about", "xdg-portal"]`,
with `default-features = false`.
Rationale per flag: `winit`/`tokio` (executor + windowing); `a11y` (P0 screen-reader
path, REVIEW UX-16); `wayland`/`x11` (both display backends); `multi-window` (kept from
default; page-stack stays single-window per REVIEW §B.f, back affordance mandatory);
`about` (`widget::about()`); `xdg-portal` (the only sandbox file-chooser route, D16).
`dbus-config` is **dropped**: D12's keep-rationale was contradicted by libcosmic's own
code (`app/cosmic.rs:119-124` unconditional proxy + `core.rs:392-404` early-return —
the branch depends on the proxy, not on which keys are watched), so `watch_config`
goes through the file watcher on COSMIC and GNOME alike (D23, sibling D27b parity).
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

Corrections applied before freezing (T0, verified by grep at sign-off): `_getDistroIcon`
is in **9 files** (not 8); status helpers are **2 named + 2 inline copies across 4 files**;
`AlertDialog` appears **23×**, of which **10** are confirm/destructive (the old "11" was an
off-by-one on that subset); repo PNG total is **65 files / 3.57 MiB** (recorded in the
ux.md §6 corrections paragraph; REVIEW's "~5 MB" figure refers to no text in any doc and
is not reproduced). Row #88 re-scoped: portal backend has no `directory()`/`file_name()` —
prefilled export names are undeliverable (D16). Row #68/#130 start-banners gated on B5
`start` (D7). `block_io` ships as the free disk-adjacent column (D18).

The 15 `dead` rows are the deliverable: #23, #28, #29, #68, #88, #95, #102, #103, #122,
#130, #146, #147, #167, #180, #181 — each must become live, re-scoped, or explicitly
dropped per §4, never silently ported.

## 3. Risk list

Blocking pre-conditions (REVIEW §G.1) — closed by D-decisions above and applied to
the docs inside T0: one feature list (D12+D23), a11y grant (D12, applied to
packaging.md §1.4 in T0 — T2 authors the manifest from it), file picker
(D16, `rfd`-configure language deleted from both docs in T0), binary name (D15),
terminals into cosmic-config (D11, grant removed + `xdg-config/cosmic:rw`, D24),
`.github` cleanup (T0: `AGENTS.md` header + `release.prompt.md` fix; T14:
`flatpak.yml` — recorded partial, not closed), `start` sequencing (T9 + D7).
PLAN §2's "Corrections applied" sentence becomes true at T0 close (verified by
grep at sign-off, per D19).

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
| I13 | Tolerant parsing surfaced: `show_skipped_lines` preference + Dashboard skipped-rows caption (no Flutter counterpart, so no parity row — T13/B3, D19) | — | ☐ |
| I14 | Header row dropped by identity, not position — `.skip(1)` predated T13 (T1 `git mv rust core`) and silently ate the first row of a headerless response (T13/B3, D27) | — | ☐ |
| I15 | `launch_app` refuses an `Exec` that leaves no command rather than spawning a bare interactive shell (T13/B4, D27) | — | ☐ |
| I16 | Container-gated empty states (Containers/Backups/Packages/Dashboard/Updates) branch on `is_clean_empty()` so an all-rows-unreadable list is never told to "create your first container"; the lie is ungated by `show_skipped_lines`, which gates only the caption (T13/B3, D27) | — | ☐ |
| I17 | `Exec` line continuation is context-dependent: both characters vanish outside quotes, the newline is kept inside them (T13/B4, D27) | — | ☐ |
| I18 | `distrobox ls` header recognized by field 0 + arity floor, accepting both the 1.5.x six-column and 1.6+ four-column layouts (T13/B3, D27) | — | ☐ |

## 5. Ordered task list (app stays buildable after every task)

Sequenced from architecture.md §7 as amended by REVIEW §§B–C/G: CI first (E8), `start`
in the backend group before banner copy freezes (UX-5), B2 line-buffering inside T5
(not later), S3 before S7, S8 deletions tagged and enumerated. The table in Status
(above) is the task list; the dependency graph there is normative.

Verification tiers (D19): **T1** unit (`cargo test`), **T2** integration
(`NullCommandRunner` + messages), **T3** running-Flatpak observation. T1+T2 gate every
task; T3 gates page tasks (T6–T12) and the release.
