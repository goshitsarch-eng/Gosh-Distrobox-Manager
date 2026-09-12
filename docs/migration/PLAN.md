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
| Phase 2 — Build | **Complete.** T0–T16 done. The last T-item closed with findings rather than clean: the break-every-flow half of T16 could not be performed (§4.4 of REPORT.md) and five `bug` + seven `missing` rows are filed open (I29, I30). |
| Phase 3 — Harden | **Pass run, findings filed.** No crash or data-loss path. See REPORT.md §10 for the criterion-by-criterion result. |

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
| T13 | B3 tolerant parsing + `show_skipped_lines`; B4 field codes; B7 runtime helper | arch | **Done** (`c738ff2`; 162+24+5+5+4+6+4+7+4+5+1 green; GLib differential 39/0 at tokenizer + launch tiers; advocate 5 confirmed (1 blocking-class: quoted line continuation) + 1 refuted by measurement + 1 deferred to T14, signed off; `verify.sh` 11/11 stages) |
| T14 | S7 + S8 deletions (tagged pre-deletion commit) + spec/scripts/AGENTS/README/CI rewrite | arch + pkg | **Done** (`b73099a` S7 FRB removal; `c8e6cd6` S6+S8 deletions + RPM rewrite; `7b2b95f` docs/CI/.gitignore/spec; S6+S8 kept in one commit because the spec and `build-rpm.sh` read `linux/data` — the E12 trap; 179 files, 177 deletions + exactly 2 rewrites, zero Rust files; tag `pre-t14-flutter-parity` = `be90524` (annotated; the last commit where `lib/` was complete *and* `core/src/api.rs` still existed, which is what the parity rows cite); E14 already satisfied, nine sources agree at 1.0.2; `verify.sh` 11/11 stages at `c8e6cd6` in an isolated worktree — the slow stages 9+10 included, so the Flatpak offline build and both smoke variants ran with the Flutter tree gone; `7b2b95f` then touches no Rust and no gate input; RPM rewrite is reviewed but **not executed** — no `rpmbuild` on the host and no gate invokes it) |
| T15 | i18n extraction pass 1 (per-screen Fluent strings; user-data placeables rule) | ux | **Done** (`e8b624c` scaffold + catalogue; `53f0885` pass 2 — the fixes only a real compiler and a real Fluent bundle could find; 365 entries, 443 call sites, 0 dead, 0 missing; `verify.sh --fast` stages 1–8 green, clippy `--all-targets -D warnings` clean, 28 unit + 35 integration tests green). Three defect classes the extraction agents could not have caught (they were forbidden from running cargo) and one they could not have known: **(a)** `-> &'static str` returns mixed `fl!` output with borrowed literals — Fluent formats at call time, so the tuple must own its `String`s (2 fns + a test helper); **(b)** `secs / 60` inside `fl!` cannot infer its `Div` target, because `fl!` expands to `args.insert(k, v.into())` and `into()` then has to choose between `u64: Div<u64>` and the `Div` impls glam puts in scope — the quotient is now bound to a `u64` first; **(c)** `FluentValue: From<&str>` yields `FluentValue::String`, which **never** matches a Fluent plural selector, so all 16 plural messages would have rendered `*[other]` forever — "1 Containers Available" in English, not just in translation. Integer counts route through `From<u64>` → `FluentValue::Number`, the only pluralising path; the call sites were already passing integers, so the fix was to make that guarantee explicit and testable. Four guards in `app/src/i18n.rs` now pin it (numeric-pluralises with a string-does-not negative control; every plural message enumerated from the FTL must reach `[one]`; no call site may stringify a count; every catalogue entry must have a caller) — each verified against an injected violation. 7 unreachable entries deleted (`action-copy`/`action-copied`/`action-search`/`app-about`/`nav-terminal`/`state-empty`/`state-error`, each superseded by a more specific id) — the catalogue mirror of I24. Also fixed here: both the Updates page's `Upgrade ` filter and the wizard's two `Create ` prefix tests matched **translated** text, dropping every task outside the fallback locale; routing now goes through a `TaskKind` discriminant on `TaskMsg::Started`. `fluent-bundle` added as a **dev**-dependency only (test-side `FluentValue`); nothing else changed in the lockfile, so `generate-cargo-sources.py --check` stayed green |
| T16 | Harden: full parity walk in the running Flatpak, break-every-flow, file new tasks; **owns `docs/migration/REPORT.md`** (assigned here by T14 — §5.7 lists it as an endgame criterion but no task owned it) | reviewer | **Done** (`fda45d5` I23 app-side closure + `4b073ae`): **the walk half is done, the break-every-flow half was not** — all four routes to interactive input are closed and the `ydotoold` denial is recorded with its reason (REPORT.md §4.4), so T3 evidence is one non-interactive Dashboard capture and 148 of 193 rows rest on `source`. All 193 rows walked and tiered (REPORT.md appendix A); the adversarial pass corrected 14 rows, nine of them **inflated tiers** recorded for tests that exercised a neighbouring core type rather than the row's own subject. All 15 `dead` rows accounted for. Five `bug` rows (I29), seven `missing` rows (I30), and the systemic ceiling behind them — `app/` has no lib target, so no integration test can construct `App` (I31). Two gate defects found and fixed: the smoke test signalled a foreign instance and blamed the app (I27, reproduced pre/post), and two committed doc comments asserted a `from_list` test that did not exist (I28, added and killed under both mutations I23 named). `verify.sh` EXIT=0, all 11 stages (stage 11 was mis-numbered 10) |

Dependency graph: T0→∅ (first); T1→T0; T2→T1; T3→T1; T4→{T2,T3}; T5→T3; T6→T5; T7→T5;
T8→T5; T9→T5; T10→T5; T11→T5; T12→{T9,T10}; T13→T3; T14→{T6..T13}; T15→{T6..T12};
T16→T14. T6–T11 parallelise across build agents after T5.

## §5.7 Endgame checklist (project-level done criteria)

- [x] Every one of the 193 parity rows ticked with a verification tier (D19) — **REPORT.md appendix A**, one row per id, verdict + tier. Tiers are post-adversarial-correction (14 rows re-examined, nine corrected downward).
- [x] `scripts/verify.sh` passes from a clean checkout (D13 gates) — **all 11 stages, EXIT=0.** Stage 10 passed only after I27 was fixed: the smoke test had been signalling a foreign instance and blaming the app.
- [x] Phase 3 pass clean, no remaining failures (T16) — **met with findings.** No crash or data-loss path; five `bug` rows (I29) and seven `missing` rows (I30) filed with evidence rather than waived. The break-every-flow half of the pass could not be performed — REPORT.md §4.4.
- [x] `docs/migration/REPORT.md` written (build/run/install, deviations, limitations) — **done**, 674 lines incl. the 193-row appendix.
- [x] Residual risks R1–R7 each have a named owner or an accepted disposition — **§3 table, extended to R1–R13.** R6 (RPM drift) and R13 (Orca) are open *with owners* rather than accepted, and REPORT.md §8 says so.

**Reading this checklist honestly:** four of the five criteria are met without qualification. The third is met in the sense that the pass ran and its findings are recorded — which is a weaker claim than "clean", and it is the one to weigh before releasing. The walk's own summary: 131 live, 41 rescoped, 9 dropped, 7 missing, 5 bug; and 148 of 193 rows resting on a traced call path rather than an executed behaviour.

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
`flatpak.yml` — **closed**, but not the way this line expected: T4 had already
rewritten the workflow, so the only defect left was its comment claiming an
artifact-publish step that does not exist, which T14 corrected. The missing
upload is a feature gap, filed as I20 rather than treated as cleanup), `start`
sequencing (T9 + D7).
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
| I19 | The RPM spec and `build-rpm.sh` are **reviewed but never executed** — `rpmbuild` is absent and no `verify.sh` stage invokes it, so both are unverified against a real build (T14) | — | ☐ |
| I20 | CI advertises no artifact: `flatpak.yml` triggers on tags but uploads nothing, and the tag build is the only thing that would produce a shippable bundle. Either add an upload step or stop triggering on tags (T14, packaging.md §2.5) | — | ☐ |
| I21 | `images/` (13 tracked PNGs) and `core/data/screenshots/` (3 tracked PNGs) are referenced by nothing. Disposition deferred by T14 rather than deleted: they are the only screenshot material that exists for the AppStream `<screenshots>` slot, which is currently unfilled (T14) | — | ☐ |
| I22 | `core/data/icons/*.svg` (24 distro SVGs) are dead payload: `app/src/icons.rs::distro_icon` returns **theme icon names** (`ubuntu`, `archlinux`), resolved by the host icon theme, and no code reads these files. The Flatpak still installs all 24 into `share/gosh_distrobox_manager/distro-icons/`; the T14 RPM spec deliberately does not, since copying dead files to match would be copying a bug. Consequence to check instead: the Flatpak gets pop-icon-theme from the Cosmic BaseApp, while the RPM relies on the host theme providing those names — unverified on a host without it (T14) | — | ☐ |
| I24 | **Callerless backend helpers T14 deliberately did not delete.** `architecture.md` §5.3 recorded these as "candidates for the T14 deletion pass" and called `get_container_runtime` "worth deleting in T14"; the deletion pass (S8) was scoped to the Flutter tree, FRB layer and the cluster that fed it, so none of them were removed and that forward-looking promise went stale (corrected in place at `architecture.md` §5.3, T14). Still callerless as of `92d0eb0`: `merge_flatpak_terminals` (`supported_terminals.rs:203`), `fetch_flatpak_terminals` (`:181`), `is_read_only` (`:224`), `terminal_by_name` (`:233`), `default_terminal` (`:255`), `get_container_runtime` (`container_runtime.rs:83`), and `launch_app` (`distrobox.rs:1102`, which has tests). Each needs a wire-or-drop ruling, not a blanket sweep: `get_container_runtime`'s claim is that it cannot serve the six `runtime_output` sites, while `is_read_only`/`terminal_by_name`/`default_terminal` correspond to parity rows that may yet be wired (§4.1 read-only badge, terminal picker) (T14) | — | ☐ |
| I25 | **The stale source-line references in `architecture.md` were never cleared.** `DECISIONS.md` counted 161 (83 `file:line` + 78 bare `:NNN`) and assigned **all of them to T14**, which already owned the docs rewrite; T14 rewrote only the *prose* about deleted paths and `git show` over all four of its commits confirms it touched **zero** `.rs:NNN` refs (T14). A re-measurement found 163 refs and 12 resolving unambiguously to `core/`+`app/`, of which **8 distinct targets are genuinely stale** — hand-verified and corrected in this commit: `container_runtime.rs:37`→`:83` (×2 sites) and `supported_terminals.rs` `:164`→`:203`, `:185`→`:224` (×2 sites). Left unverified and therefore **not** rewritten: `distrobox.rs:1215` (§2.3/row B4), `known_distros.rs:45` (row B4), `host_exec.rs:20` (row B4), `supported_terminals.rs:27`, `distrobox.rs:1905`. The rest are citations to *external* pinned sources (libcosmic at `a401af8b`, `async-process 2.5.0`, `cosmic-config`, `iced/futures`) whose numbering cannot be checked from this tree, plus `api.rs`/`app_state.rs` refs that T13/T14 made deliberately historical. T16 should sweep the remainder as section names (`§6.4, row B4`) per the convention `DECISIONS.md` already adopted for the five Rust-side sites (T14). **Done (T16), and the count was wrong in both directions.** The §0.3 list had grown from 5 items to 8 by the time the §6.4 rows were written, so its citations were never in the 161 either. Swept to section names and symbols: §0.2, §0.3 (all 8 items, the first 5 now citing their fix rows), §0.4, §1.3, §2.3's draft comment, §4.1, §4.3, §5.2, §5.3, §6.1, §6.2, §6.3, §6.4's B-row preamble, §8. **Two deliberate non-changes:** §0.1 external pins stay numbered (their numbering is not checkable from this tree and is the *point* of the pin), and `api.rs`/`app_state.rs`/`frb_generated` refs stay as written — they name files T14 deleted, and §1.3's plan sections are frozen because §1.3.1 exists to record where execution diverged from them. Five of the critic's corrections were adopted and verified line-by-line (`upgrade`→`:1443`, `install_package`→`:1690`, `remove_package`→`:1728`, `export_container`→`:1987`, `import_container`→`:2002`); one was **rejected** — `(2139)`→`(3384)` in §1.4, because that paragraph declares its basis as the `e5436a0` tree and the `2139 + 599 = 2738` arithmetic closes only with 2139, whereas the file is now 3,384 lines. A symbol-reference sweep over the result found and fixed one defect of my own: §4.1 said `read_lines`, which is not a symbol in this tree (`read_lines_to_registry` is). | — | ☑ |
| I23 | **B3's skipped-rows plumbing is un-pinned end to end** — six experimentally-confirmed coverage holes from T13's post-commit adversarial review (`subagents/workflows/wf_1426a25a-4b25*/journal.jsonl`; all 3-vote, 0-refute, severities should-fix×4 / nit×2). Each is a mutation that deletes real behaviour and leaves the suite green: (a) `core/src/service.rs:114` `Backend::containers` → `out.skipped.clear()`, (b) `app/src/app.rs:3409` hardcoding `skipped: 0` or `show_skipped: true`, (c) `app/src/views.rs:392` deleting the Dashboard caption render, (d) `app/src/updates.rs:161` deleting the Updates caption block, (e) `core/src/backends/desktop_file.rs:233` shrinking the double-quote escape set to one char, (f) `:218` giving single quotes backslash escaping. **(a)–(d) share one root cause:** every app fixture reaches `Backend::containers` via `List(Vec<ContainerInfo>)`, rendered by `build_list_response` as always-well-formed rows, so `skipped` is structurally empty in every test and an `is_empty()` assertion passes for both the real and cleared value. The fix is one fixture that injects an unparseable row across the service boundary; T16 owns it (T13). **Closed by T16, except that (f) was never a hole — five holes, not six:** **(a)–(d)** `DistroboxCommandRunnerResponse::RawList(String)` was added so a fixture can carry raw `ls` stdout past the generated table, and `app/tests/skipped_rows.rs` drives the real parser for real: `backend_containers_preserves_skipped_rows` (one malformed row among two good ones must reach the app as `skipped.len() == 1`, quoting the row) with `backend_containers_reports_nothing_for_a_clean_list` as the fabrication control, `a_short_total_is_accompanied_by_a_nonempty_skip_list` (d — the page's total counts *parsed* rows, so the qualifier must have something to fire on or the branch is unreachable), and `an_all_unreadable_list_is_not_a_clean_empty_account` (the I16 gate: an all-rows-unreadable list must not read as an empty account). The two (b)/(c) halves live in `app/`, which has **no lib target** — an integration test cannot import it — so the app-side call site was given a testable seam, `views::DashboardCounts::from_list`, unit-tested in `app/src/views.rs` and now what `view_dashboard` actually calls. `ParseIssue::new` + `ContainerList::with_skipped` exist so a fixture can carry a `skipped` list without hand-writing a malformed table, pinned by `with_skipped_actually_attaches_the_issues` (a fixture helper that quietly dropped its input would make every dependent test vacuous while still passing). **(e) is a genuine hole, confirmed by re-measurement:** narrowing the arm to `matches!(n, '"')` left all 35 `desktop_file` tests green, because `\"` was the only member of the set any test exercised (`\n` never reaches the arm — the *value* pass has already decoded it to a real LF). `split_exec_double_quote_escapes_the_full_four_character_set` now asserts each of the four spec characters individually plus the `\q` negative control, and was verified to fail under that exact mutation before being kept. **(f) was refuted:** giving single quotes backslash escaping makes **two** existing tests fail (`split_exec_single_quotes_are_fully_literal`, `split_exec_undefined_escape_loads_and_composes_with_the_tokenizer`), so it was already covered and the claimed severity was wrong. Residual risk: (b)'s `show_skipped: true` half is pinned only through `from_list`, not through a rendered frame — the `app/` crate cannot be reached from an integration test, and a headless render harness is out of scope here (T16) | — | ☐ |
| I27 | **The smoke gate signalled the wrong process, and blamed the app for it.** `smoke-test.sh` §(3) located the binary with `pgrep -f '^gosh_distrobox_manage[r]' \| head -1`, which matches *any* installed-and-running copy and returns the **lowest** PID. With a second instance alive — e.g. a hand-run app left open, which is exactly what a parity walk does — the SIGTERM went to that foreign process while the smoke test's own instance kept running; the unscoped final check then saw the smoke instance still alive and printed `FAIL — binary lingers after SIGTERM`. **The app does exit cleanly on TERM; the gate was measuring the wrong PID.** Reproduced deterministically (one instance left running → `verify.sh` fails at stage 10; none → passes) and fixed in T16 by snapshotting PIDs *before* launch and subtracting them, plus a `NOTE — ignoring pre-existing instance(s)` line so a future reader can see why a foreign process was skipped. Caught only because T16 ran the gate with an instance of the app already open — the gate had been passing for every prior task purely because nothing else was running (T16) | — | ☑ fixed in T16 |
| I28 | **Two committed doc comments asserted a test that did not exist.** `app/src/views.rs:344-347` ("This is the seam that makes it testable") and `app/tests/skipped_rows.rs:15` ("`App`'s call site is unit-tested in `app/src/views.rs`") both claimed `DashboardCounts::from_list` was unit-tested — the seam T16/T13 introduced to close I23's residual (b), where `app/`'s lack of a lib target had left the Dashboard's `skipped` count un-pinnable. No test called it. A reassuring sentence in place of a test is worse than no sentence: a reader checking where I23 closed would have stopped there. Fixed in T16 by `dashboard_counts_do_not_prettify_the_skip_count`, verified to fail under both mutations I23 named by hand (`skipped: 0`, `show_skipped: true`) before being kept (T16) | — | ☑ fixed in T16 |
| I29 | **Five parity rows regressed to `bug` in the walk** — each a frozen row whose *live* half works but whose named affordance is absent, with no ux.md decision authorising the omission. **#13** "Check Again" is inert for the process lifetime: the button sends `ContainerMsg::RefreshRequested`, which re-runs `backend.containers()` but never re-probes the environment, because `EnvGuard` is built once in `Application::init` (`app.rs:425`) and `Backend` exposes no `set_env`/re-probe API — so a user who installs distrobox while the app is open is told to check again, and checking again changes nothing. **#20** the task card renders no spinner and no "In progress…/Completed" copy while its own doc comment (`views.rs:522`) claims "spinner while running"; running is signalled only by the Cancel button's presence. **#97** the Images page's Refresh is the *global containers* refresh (`app.rs:1544-1548` → `refresh_containers` only), so `backend.images()` — one call site, `app.rs:234` — can never be re-fetched after its single lazy load; the tree already special-cases Apps for exactly this mis-wiring (`app.rs:1537-1540`) and did not for Images. **#134** the FAB bug is only half fixed: the FAB is gone, but in the blocked and not-installed states the shell gate replaces the page while `header_end` still renders "New Snapshot" (`app.rs:1582-1588`), which can only toast "Select a container first." — the button is visible in precisely the states the row called out, and the `app.rs:1575` comment claiming the bug "disappears with the move" overstates. **#186** `status_color` (`icons.rs:96`) is `#[allow(dead_code)]` and **callerless** — a repo-wide grep finds only its definition, its four test assertions, and a comment saying "NOT YET RENDERED" — so the theme-role mapping is tested but no colour reaches any widget, and `distro_colour` was never written at all (T16 walk) | #13 #20 #97 #134 #186 | ☐ open |
| I30 | **Seven parity rows are `missing`** — absent, not rescoped, and each needs a wire-or-drop ruling rather than a silent pass. **#5** per-destination nav icons: the single `nav_model.insert()` call (`app.rs:421`) never chains `.icon()`, so the rail is text-only for all ten destinations and `Page::title()` has no `Page::icon()` sibling; the `Entity::icon` API exists at the pinned rev. **#40** the running-only inline "Open Terminal" on the container *card* — the details page has the same control with the same running gate, but the card offers no terminal path. **#41** the card's ⋮ quick-actions trigger and its sheet: no `context_menu`, `popover` or `context_drawer` exists anywhere in `app/src`, and `app.rs:3545-3548` explicitly defers a card-menu shell. **#161** task history is still in-memory only — ux.md:266 classes persistence as P1, and the Activity page shows the live session exactly as Flutter did. **#185** the colour duplication §3.6 asked to consolidate: `status_color` is callerless (see I29/#186) and `distro_colour` was never written, so the ×3 distro-colour and ×4 status-colour duplication is unresolved while the icon/label halves did land. **#189** keyboard shortcuts: `keyboard_nav::subscription()` and `set_keyboard_nav` are verified present at the pinned rev; nothing in `app/` uses them, so the Ctrl+R the row's approach column names does not exist. **#190** a11y semantics: §5.3's priority-1 item (labels on icon-only controls) is untouched and the Orca acceptance gate was never run (T16 walk) | #5 #40 #41 #161 #185 #189 #190 | ☐ open |
| I31 | **The app crate's missing lib target is a systemic verification ceiling, not a page-level gap.** `app/` declares only `[[bin]]` and has no `src/lib.rs`, so no integration test can import `App`, any page module, or the widget tree. Consequence measured across the walk: of 193 rows, **148 rest on `source` tier** (a verified call path read in the code, not an executed behaviour), against 26 T1 and 12 T2. Every page-level row is affected, and the two tiers that do exist are reachable only because the row's subject happened to coincide with a pure helper or a `core`-side backend contract. The walk's adversarial pass found that **14 of 193 rows carried inflated tiers** for exactly this reason — a T2 recorded for a row whose cited test exercised a *neighbouring* core type or a backend argv, never the row's own subject. Fixing this properly means either promoting `app/` to a lib + thin bin (so integration tests can construct `App`) or adding a headless render harness; both are out of T16's scope and are recorded here rather than silently absorbed into tier counts (T16 walk) | — | ☐ open |
| I26 | **The app ships 10 nav destinations; `ux.md` §4.2 authorises 8.** `ux.md:68` says "**8 destinations:** keep all 8, same order and icons (Dashboard, Containers, Images, PackageManager, Updates, Backups, ActivityLogs, Settings)" — which is exactly `home_screen.dart:76-83`, the entire Flutter rail. `app/src/views.rs:53` (`Page::ALL`) has those 8 **plus `Apps` and `Stats`**. Neither was in the rail: `apps_page.dart` was a pushed route from the details page, and nothing in the Flutter tree renders a "Stats" destination at all (the only `Stats` token is `app_state.dart`'s `containerStats`/`loadContainerStats`, the data method the details sheet used). So both are additions the port made, not parity. `Stats` is not ux.md's warned-off placeholder: `app.rs:344 view_stats()` renders real measured values (`stats.cpu_percent` formatted at the call site into a placeable, per §Q17). `ux.md:262` offered "a nav destination or (better) a section on Settings" and the port took the first. Consequence to settle in the walk: whether a destination reachable in neither Flutter nor the parity rows belongs in the chrome, and whether `Apps` (contextual, a pushed route in Flutter) should stay top-level when `Details` and `Terminal` — the other two pushed routes, per `ux.md:74` — did **not** get destinations. Flagged rather than silently kept (T15) | — | ☐ |

## 5. Ordered task list (app stays buildable after every task)

Sequenced from architecture.md §7 as amended by REVIEW §§B–C/G: CI first (E8), `start`
in the backend group before banner copy freezes (UX-5), B2 line-buffering inside T5
(not later), S3 before S7, S8 deletions tagged and enumerated. The table in Status
(above) is the task list; the dependency graph there is normative.

Verification tiers (D19): **T1** unit (`cargo test`), **T2** integration
(`NullCommandRunner` + messages), **T3** running-Flatpak observation. T1+T2 gate every
task; T3 gates page tasks (T6–T12) and the release.

---

## 6. Hardening tasks T17–T21 (ordered; T0–T16 above are frozen record)

T16 closed with findings, not clean: five `bug` rows (I29), seven `missing`
rows (I30), the I31 verification ceiling (structural half since fixed — lib +
thin bin, `app/tests/parity_rows.rs` — render-harness half open), the I26
destination question (ruled by D28, recorded with this plan), and the I20 CI
gap. T17–T21 close those findings in dependency order. Sources: the three
hardening plans (arch, ux, pkg/QA) plus the devil's-advocate review; review
objections O1–O3 are dispositioned in T21 and the I26 disagreement in D28.
Nothing above this section changes: the status table and §5.7 stay as T16 left
them.

| # | Task | Owner | State |
|---|---|---|---|
| T17 | EnvGuard re-probe (#13) + Images refresh routing (#97) | arch | ☐ open |
| T18 | Task-card spinner (#20) + gated header actions (#134) + status/distro colours (#186/#185) | ux | ☐ open |
| T19 | Nav icons (#5) + card terminal (#40) + card menu (#41) + shortcuts (#189) + a11y labels (#190), on the D28 destination set | ux | ☐ open |
| T20 | Task-history persistence (#161) + I31 render-harness half | arch | ☐ open |
| T21 | CI release artifact (I20) + full gate + REPORT.md appendix | pkg | ☐ open |

Dependency graph: T17→T16; T18→T17 (refresh/message arms settle before the
header gate is built on them); T19→{T18, D28} (colours land before the a11y
label pass touches the same controls; #5's icon table needs the ruled
destination set); T20→T19 (the harness asserts finished widgets); T21→{T17,
T18, T19, T20} (certifies everything, re-tiers the appendix). T1+T2 gate every
task; T3 gates the render claims in T18/T19 and the release in T21. The app
stays buildable after every task.

### T17 — EnvGuard re-probe + Images refresh (owner: arch)

Scope: **#13** — add `Backend::reprobe()` (or `set_env`) in core that re-runs
`env::detect` on a fresh base runner, swaps the env-mapped runner,
`Distrobox`, and `TerminalRepository` under interior mutability, and returns
the new guard; keep the blocking `check_installed` helper-thread shape
(`env.rs:122-140`) so it stays callable without an ambient runtime.
`RefreshRequested` (or a new `EnvMsg::ReprobeRequested`) dispatches one Task
producing `EnvMsg::Probed(guard)`; the now-live `Probed` arm (`app.rs:1378`,
today an explicit no-op) updates gate state. Fixes live in core + the
message/state model; `app/` only renders and dispatches (AGENTS.md rules 1–2,
§0.2 runtime rule: core spawns only from Task/Subscription futures).
**#97** — route Images-page Refresh to `backend.images()`, mirroring the Apps
special-case (`app.rs:1537-1540`): today Refresh is the global containers
refresh (`refresh_containers` only), so images are never re-fetched after the
single lazy load (`app.rs:234`).
Verification tiers: **T2** — NullCommandRunner: Backend with
`distrobox_installed=false`, flip fixture to success, `reprobe()` →
`is_distrobox_installed()==true` and subsequent `containers()` succeeds;
images list changes reflected after Refresh. **T1** — `Probed` arm gate-state
transition; refresh-routing unit tests.
DoD: "Check Again" recovers a mid-session distrobox install without a
restart; Images Refresh re-fetches; `cargo build`, `clippy --all-targets -D
warnings`, `cargo test`, `verify.sh` green; I29 #13/#97 rows re-tiered with
evidence; reviewer sign-off; committed.

### T18 — Spinner + gated header + colours (owner: ux)

Scope: **#20** — lead ruling: **add** the spinner (the row's claim is the
parity source; dropping it would need a re-scope plus a §5.7 note, which is
the documented fallback only if the widget proves unimplementable at the
pinned rev). Render `widget::progress_bar::indeterminate_circular()` plus an
"In progress…/Completed" caption beside Cancel, following the existing
pattern in `app/src/updates.rs:60-68`; rename `RunningCancelOnly` (e.g. to
`Running`) — the variant name deliberately forces a `parity_rows.rs` edit for
either choice. **#134** — gate `header_end` actions on env state: in the
blocked and not-installed states the shell gate replaces the page while
Backups still renders "New Snapshot" (`app-new-snapshot`), which can only
toast; hide or disable page actions that cannot act, and correct the
`app.rs:1575` comment claiming the bug "disappears with the move".
**#186/#185** — wire `status_color` (`icons.rs:96`, today `#[allow(dead_code)]`
and callerless) into real widgets, write the missing `distro_colour`, and
consolidate the ×3 distro-colour / ×4 status-colour duplication per ux.md
§3.6; theme roles only (D10, D18).
Verification tiers: **T1** — `parity_rows.rs` affordance table updated for the
renamed running variant; header-gating pure-helper tests; colour-mapping
unit tests. **T2** — message-level dispatch for the gated actions.
**T3** — running-Flatpak observation of spinner, gated header, and rendered
colours (rendered output is unassertable until T20's harness lands).
DoD: running tasks show spinner + status copy; no dead header action in gated
states; colours reach widgets with zero `Colors.*`-style literals; gates
green; I29 #20/#134/#186 and I30 #185 rows re-tiered; reviewer sign-off;
committed.

### T19 — Nav/icons/cards/shortcuts/a11y on the D28 set (owner: ux)

Scope: destination set is D28 (9: the 8 ux.md destinations plus Stats; Apps
demoted to a details-pushed route) — #5's icon table covers exactly that set.
**#5** — add `Page::icon() -> &'static str` returning freedesktop symbolic
names (verify each exists in pop-icon-theme/BaseApp, fall back to generic)
and chain `.icon(...)` in `nav_model.insert()` (`app.rs:421`); keep text
labels. **#40** — running-only inline "Open Terminal" on the container card,
mirroring the details-page control and its running gate. **#41** — card ⋮
trigger plus sheet via `context_drawer` (already used for the activity
full-output drawer, `app.rs:1605`) or `popover`; single-window per D9; lifts
the `app.rs:3545-3548` deferral. **#189** — keyboard shortcuts (Ctrl+R per the
row's approach column) via `keyboard_nav::subscription()` +
`set_keyboard_nav` at the pinned rev. **#190** — ux.md §5.3 priority-1 labels
on icon-only controls, then run the Orca acceptance gate (R13's open owner).
Verification tiers: **T1** — icon table (non-empty, distinct, theme-name
format; order test already guards the index space) plus gating/routing helper
tests. **T2** — dispatch tests for card terminal/menu and shortcut messages.
**T3** — Orca pass and shortcut observation in the running Flatpak.
DoD: 9-destination rail with icons and no reorder/regress (I26 pins updated:
8 + Stats, Apps-absent-from-rail test added); card terminal + menu live;
shortcuts work; §5.3 priority-1 complete with an Orca result recorded (pass
or filed finding, not silence); gates green; I30 #5/#40/#41/#189/#190 rows
re-tiered; reviewer sign-off; committed.

### T20 — Task-history persistence + render harness (owner: arch)

Scope: **#161** — persist task history (ux.md:266 classes it P1; Activity
shows the live session only): a bounded completed-task ring, persisted via
cosmic-config alongside existing prefs (D11) unless the owner measures cause
for an xdg-data file; Activity renders persisted + live; eviction and schema
versioning covered. **I31 render-harness half** — a headless harness that
constructs `App` (needs a `cosmic::app::Core` and a running executor) so
*rendered* output becomes assertable rather than only the decisions behind
it; migrate a seed set of rows onto it. Deliberately not claimed: raising all
148 `source`-tier rows — each migrated row is re-evidenced individually and
no tier count moves without its test.
Verification tiers: **T2** — persistence round-trip (write → re-read →
Activity shows) with NullCommandRunner-backed tasks. **T1** — ring bound,
eviction, and schema-migration unit tests; harness seed-row assertions.
DoD: history survives restart within the documented bound; harness constructs
`App` headless and asserts rendered output for the seed rows; gates green;
I30 #161 and I31 rows updated with per-row evidence; reviewer sign-off;
committed.

### T21 — CI release artifact + full gate + appendix (owner: pkg)

Scope: **I20, arm (a) only.** `actions/upload-artifact` alone does not
satisfy packaging.md:964/:1227 — run storage is not release assets (review
O1): attach the bundle to the GitHub Release on tags (job `permissions` to
`contents: write`), and retain/export an OSTree repo so `flatpak
build-bundle` has something to bundle from (`verify.sh` builds `--user
--install` with no `--repo` today). Arm (b) (drop the tags trigger) is
rejected (review O2): job 3 is specified "tags + workflow_dispatch only" and
D19 requires T3 to gate the release, so deleting the trigger removes the only
shippable-artifact certifier. **verify.sh health** — single-source the stage
count (today `flatpak.yml:3,71` and `rust.yml:23` still say "1-10" and
packaging.md:611-627 describes 10 while `verify.sh` has 11); `rust.yml` must
call `verify.sh` instead of re-spelling fmt/build/clippy/test (AGENTS.md:53);
preflight the stage-6 validators (`desktop-file-validate`, `appstreamcli`)
and the BaseApp. **REPORT.md appendix** — re-tier every row T17–T20 touched
and update the counts and tier distribution. Full `verify.sh` stays the
certifier throughout.
Verification tiers: **T1** — CI-YAML assertions in `tests/test_packaging.py`
(stage 8): upload step present, tags trigger present, permissions correct,
stage-count guard; no ad-hoc grep as evidence (review O3). **T3** —
`workflow_dispatch` dry run plus a fork tag showing the bundle attached to
the release.
DoD: tag build publishes a shippable bundle to the release; `verify.sh` 11/11
green from a clean checkout; zero stale stage-count references; REPORT.md
appendix current with residual risks named; reviewer sign-off; committed.
