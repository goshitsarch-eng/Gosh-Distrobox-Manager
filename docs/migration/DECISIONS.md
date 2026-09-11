# Migration decisions

Standing tie-break priority (project brief, applies to all decisions below):
parity with the shipped app → correct Flatpak sandbox behavior → accessibility →
simplicity/maintainability → COSMIC conventions.

## D1 — Parity source is the Flutter app, not GTK4

- **Question:** The brief assumes a GTK4 codebase; the repo ships Flutter + Rust. What counts as "the current app" for feature parity?
- **Options:** (a) Flutter UI in `lib/`; (b) upstream DistroShelf GTK4 code.
- **Choice:** (a). The Flutter bundle is what the RPM spec, desktop file, and metainfo ship.
- **Why:** Parity must be measured against what users run. Recorded in memory as
  [[cosmic-migration-flutter-source]].

## D2 — libcosmic pinned by git SHA, never crates.io

- **Question:** How to depend on libcosmic?
- **Options:** (a) crates.io `cosmic`; (b) tag `v0.12`; (c) `branch = "master"`; (d) pinned `rev`.
- **Choice:** (d) `rev = "a401af8b1c54a8abd393b8c5b7c8809402f83850"` (2026-09-10, v1.0.0).
- **Why:** (a) is an unrelated squat crate; (b) is a stale 2024 pre-1.0 API; (c) is
  unreproducible against a daily-committing repo. Revisit the pin only deliberately.

## D3 — Backend moves with history, Flutter tree removed last

- **Question:** How does `rust/` become the core crate, and when does Flutter code go away?
- **Options:** (a) copy backend into a new crate; (b) `git mv rust core`, strip FRB shims
  in small green steps, delete the Flutter tree in the final cleanup task.
- **Choice:** (b).
- **Why:** Preserves history; the "buildable after every task" guarantee applies to the new
  Cargo workspace (`core` + `app`), not the retired Flutter shell (no Dart SDK exists here,
  so the Flutter tree is unverifiable anyway).

## D4 — Architecture findings accepted pending devil's advocate review

- **Question:** Accept the architecture teammate's verified API traps into the plan?
- **Options:** (a) trust-but-verify during review; (b) re-verify now.
- **Choice:** (a) — carried as review items: `Application::init` (no `new()`),
  `iced::Task<cosmic::Action<M>>` vs bare `cosmic::Task`, no `cosmic::Subscription`
  (use `cosmic::iced::Subscription`), and no `tokio::spawn` from `update()`
  (use `cosmic::task::future`).
- **Why:** Single review pass over all three docs is cheaper than piecemeal verification.

## D5 — Adopt the sibling project's measured answers

- **Question:** The org's `Gosh-Yubico-Authenticator-for-Linux` already completed a
  libcosmic migration with measured decisions. Reuse or redo?
- **Options:** (a) rebuild vendoring/CI/smoke tooling from scratch; (b) port
  `flatpak/generate-cargo-sources.py` + `git-manifests/` sidecars + manifest shape +
  measured `dbus-config`/a11y findings.
- **Choice:** (b) — tasks T0/T2/T4 are ports, not greenfield builds.
- **Why:** Measured beats theorised: 15-errors-per-20s `dbus-config` behaviour, the
  `--talk-name=org.a11y.Bus` requirement, and the offline-vendoring procedure are on
  record with commits. Recorded by the devil's advocate (REVIEW §E1).

## D6 — Start-container blocks release, not page work

- **Question:** No `start` op exists anywhere in the stack, yet three screens promise
  it. Block the migration or sequence it?
- **Options:** (a) build `start` before any page work; (b) build pages, gate the three
  banners, land `start` in the backend group (T9) before banner copy freezes.
- **Choice:** (b).
- **Why:** Shipping the banners' promise without the capability is the worse evil
  (those rows are `dead` — the defect class this migration exists to remove), but
  blocking all page work on one backend op serialises the team for no benefit.

## D7 — Backend fixes B1–B9 land with the port

- **Question:** The nine confirmed backend bugs (emerge dead-end, chunk-based lines,
  fail-whole-list parse, naive %-strip, missing start, cancel leaks child, 7× runtime
  fallback, env-guard drift, per-chunk write lock) — fix during or after?
- **Options:** (a) port verbatim, fix later; (b) fix as part of the owning task
  (B2 inside T5, B5 before banner freeze, B1 in T8, B3/B4/B7 in T13).
- **Choice:** (b).
- **Why:** Parity first does not mean bug-for-bug: B2/B5/B1 gate page behaviour, and
  the line-buffered reader lives in the same function T5 rewrites. Doing it later
  means touching the subscription contract twice.

## D8 — Terminals revive; `PodmanEventStream` defers

- **Question:** Two dead modules: `supported_terminals.rs` (312 lines, unwired) and
  `podman.rs` event streaming. Revive both?
- **Options:** (a) revive both; (b) revive terminals (needed for the terminal page +
  `selected_terminal` config), defer the event stream to a post-parity backlog with
  watchdog/reconnect + its own tests.
- **Choice:** (b).
- **Why:** The event stream is the plan's most seductive untested path — no podman
  exists here, so a live stream would be a long-lived, silently-failing code path.
  Poll (`refresh_interval_secs`) for Phase 2.

## D9 — Single window + back stack; no multi-window

- **Question:** Flutter pushes details/terminal/apps/create as routes. Multi-window or
  in-app page stack?
- **Options:** (a) `multi-window` feature; (b) single window + page stack with explicit
  back affordance on every nested page.
- **Choice:** (b).
- **Why:** The Flutter app is single-window (no measured multi-compare use); (a)
  multiplies the `Message` enum work. Nested pages must carry an explicit back control
  — keyboard users lose the exit otherwise (REVIEW §B.f).

## D10 — Drop theme toggle, Inter, and hardcoded palette

- **Question:** The app brands on `#137FEC` + Inter + `Colors.*` literals. Keep via
  `Application::style()` override?
- **Options:** (a) preserve brand chrome; (b) theme roles + system font, brand lives
  in the icon/logo.
- **Choice:** (b).
- **Why:** a11y + i18n outrank cosmetic parity (standing priority): hardcoding the
  accent defeats high-contrast/reduced-transparency and text-scaling preferences.

## D11 — cosmic-config; terminals fold in; legacy keys import once

- **Question:** `cosmic-config` vs `dirs`+`serde` for persistence? And the
  `distroshelf-terminals.json` file, and the born-dead gschema?
- **Options:** (a) hand-rolled `dirs`+`serde` store; (b) cosmic-config with
  `custom_terminals: Vec<Terminal>` folded in, one-time import of upstream
  `com.ranfdev.DistroShelf` keys (`distrobox-executable`, geometry,
  `selected-terminal` via program-match, first-and-log on ambiguity), then delete.
- **Choice:** (b).
- **Why:** Sandbox-correctness outranks simplicity: (b) deletes a `finish-args`
  entry, a path-mismatch failure mode, and a second source of truth. Our own
  `io.github.*` gschema was born dead in `844e55e` (nothing ever read it); the only
  real migration source is upstream's id. `distrobox_source="bundled"` is dropped —
  nothing bundles distrobox.

## D12 — One feature list; a11y grant; port the generator

- **Question:** Three contradictions to resolve: feature lists differ across docs;
  a11y rated P0 but `--talk-name=org.a11y.Bus` missing; build-vs-vendor strategy open.
- **Choice:** `default-features = false` +
  `["winit","tokio","a11y","wayland","x11","multi-window","dbus-config","about","xdg-portal"]`
  written once in PLAN.md §1; add `--talk-name=org.a11y.Bus` and
  `--filesystem=xdg-config/cosmic:ro`; port the sibling's `generate-cargo-sources.py`
  + `git-manifests/` sidecars (extended to all five git pointers) with `--check` in CI.
- **Why:** `dbus-config` stays (unlike the sibling's D27b): that app edits COSMIC
  settings and needed no live watch; ours watches its own keys via `watch_config`,
  and the measured failure was log noise, not breakage. `xdg-config/cosmic:ro` +
  file-watcher fallback covers GNOME.

## D13 — verify.sh gates; behavioural tests, not per-variant

- **Question:** What gates a task, and what is the `Message` test bar?
- **Choice:** `scripts/verify.sh` stages: `cargo fmt --check`, `cargo build
  --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `check-versions.sh`, metadata validation,
  `generate-cargo-sources.py --check`, flatpak-builder offline build, smoke test
  (readiness signal + stay-alive + clean SIGTERM + negative flatpak-spawn case).
  `--locked` in CI only, not the local loop. Test bar: high-value behavioural tests
  + exhaustive `match` (compiler-enforced coverage) + one dispatch test per nested
  sub-enum — not one test per variant.
- **Why:** Per-variant tests duplicate the compiler and tax Phase 2 churn; `--locked`
  locally teaches contributors to bypass `verify.sh`.

## D14 — Version unification is mechanical

- **Question:** Cargo 1.0.2 vs spec/metainfo 1.0.0 drift, plus `build-rpm.sh` /
  `RPM-BUILD.md` outside every table.
- **Choice:** `core/Cargo.toml` version is the single source of truth;
  `scripts/check-versions.sh` (Cargo, spec, metainfo, `build-rpm.sh`,
  `RPM-BUILD.md`) gates `verify.sh`; RPM spec maintained in lockstep (it is the only
  non-Flatpak path and carries `Requires: distrobox`).
- **Why:** The drift class is recurring; only a gate fixes it.

## D15 — Binary spelling: `gosh_distrobox_manager`

- **Question:** Three spellings exist (`gosh_distrobox_manager` in Cargo/desktop,
  `gosh-distrobox-manager` in spec `Name:`).
- **Choice:** Underscore binary everywhere Cargo/desktop/manifest controls;
  hyphenated spec `Name:` stays (RPM convention, `%{name}` ≠ binary).
- **Why:** One spelling across manifest `command:`, `Exec=`, and installed
  `/app/bin/` filename or all four break together and silently.

## D16 — xdg-portal file picker, not rfd

- **Question:** `rfd` vs `xdg-portal`/ashpd for export/import pickers?
- **Choice:** `xdg-portal` (what libcosmic's `file_chooser` already routes through);
  there is no "configure rfd's portal backend" hook. Portal has no
  `directory()`/`file_name()` — prefilled export names are undeliverable, row #88
  re-scoped, not ported.
- **Why:** The portal is reachable from the sandbox with no added grant, on COSMIC
  and GNOME alike — P0 for both channels.

## D17 — Smoke test needs a readiness signal

- **Question:** Stay-alive + clean-SIGTERM passes for a blank window. Enough?
- **Choice:** No — mandatory: (1) alive, (2) readiness line on first `view()`
  completion, (3) clean SIGTERM exit code, (4) negative case (removing
  `--talk-name=org.freedesktop.Flatpak` must fail). Widget-presence hook deferred
  and recorded.
- **Why:** With no distrobox/podman here, the smoke test is the only automated
  signal the shipped binary does anything at all.

## D18 — Orphans and brand-adjacent scope cuts

- **Question:** `disk_usage_page` (unreachable mock), `task_page` (stub), real disk
  backend, terminal emulation, local image management.
- **Choice:** Drop both orphans and terminal emulation; ship `block_io` as the free
  disk-adjacent column; local image management is P1 post-parity (needs new backend
  plumbing — `list_images` is a catalogue, not local state).
- **Why:** Funding `podman system df` + per-container sizing is product spend with no
  parity target (the page was unreachable — nobody used it).

## D19 — 193 rows frozen; tiered verification

- **Question:** 193 rows vs collapsed ~110 for PLAN.md?
- **Choice:** Keep all 193 (the 15 `dead` rows are the deliverable; collapsing is how
  they survive into the port). Estimates come from ~12 page-level work items, not row
  count. Each ticked row records its tier: T1 unit, T2 integration, T3
  running-Flatpak observation. T1+T2 gate every task; T3 gates page tasks + release.
- **Why:** The collapsed list cannot produce the auditable
  exists-148/dead-15/dup-14/missing-13/bug-3 summary.

## D20 — Accepted technical notes (no action, recorded)

- Submodule/pointer SHAs live in `pop-os/libcosmic`'s tree: detectable via sidecar
  `--check`, not preventable. Accepted.
- Poll-only container state (`refresh_interval_secs`); event stream deferred (D8).
- Core/app task-registry duality + `TaskMsg::Expired` round-trip. Accepted.
- 18 MB tarball + Flutter tree stay in git history; S8 deletes working tree only
  (no `filter-repo`). Accepted.
- Orca/AT-SPI never verified on this distro: manual release step. Accepted.
- `cosmic_config_derive` keys are `stringify!` field names (snake_case verbatim);
  legacy gschema kebab-case maps by rename. Resolved by review; recorded.

## D21 — Sibling-plan process lessons applied

- **Question:** The sibling's D27e notes claims "asserted from the neighbourhood of
  the code rather than the code" (wrong executor readings, wrong counts). Apply here?
- **Choice:** Yes: counts verified by grep before freezing (9 icon files, 23
  `AlertDialog`s, 65 PNGs/3.6 MB); `Application::init`/`Task`/`Subscription` facts
  verified at the pinned rev; architecture.md §0.2's runtime-trap mechanism corrected
  (iced enters the runtime around `update()` — rule stands, reason fixed); per-task
  `run_with` + `OnceLock<Arc<Backend>>` kept per precedent; nested `Message`
  sub-enums; core owns the registry.
- **Why:** A plan justified by a false premise loses the argument that follows.

## D22 — T0 absorbs the mechanical lint fixes; "verbatim" means behavior-preserving

- **Question:** The reviewer proved CI red at birth: 9 clippy errors at HEAD, 4 in
  modules slated to "move verbatim" — so T0 cannot close under its own DoD. Widen T0
  or waive the gate?
- **Options:** (a) ship the workflow red / `continue-on-error`; (b) widen T0 to fix
  the 9 lints; (c) waive clippy for T0.
- **Choice:** (b), with a boundary: redundant closures, collapsible `if`, `Default`
  impl, type-complexity aliases, `option_map_unit_fn` get fixed; structural lints
  (`module_inception` on `backends/distrobox/mod.rs`, the `frb_expand` unexpected-cfg)
  get scoped `#[allow]` + comment citing the closing task (T1 restructure / S7 FRB
  deletion) instead of renames smuggled into a lint commit.
- **Why:** A red pipeline on day one is how gates get deleted (process priority, and
  the DoD is non-negotiable). "Move verbatim" (PLAN §"What is actually changing")
  always meant behavior-preserving — lint/format churn does not violate it; the two
  tempting non-mechanical edits (`distrobox.rs:564`, `app_state.rs Default`) must be
  verified semantics-preserving at sign-off.

## D23 — Drop `dbus-config`: D12's keep-rationale is contradicted by libcosmic's code

- **Question:** D12 kept `dbus-config`, arguing the sibling's measured 15-errors/20s
  failure was app-specific (that app edits COSMIC settings; ours only watches its own
  keys). The reviewer cites `libcosmic/src/app/cosmic.rs:119-124` (unconditional
  `settings_daemon` proxy at every `Cosmic::init` when the feature is on) and
  `core.rs:392-404` (`watch_config` early-returns the D-Bus watcher whenever the proxy
  is `Some`, bypassing the file watcher entirely) — the branch depends on the proxy,
  not on which keys the app watches.
- **Options:** (a) keep with "unverified" caveat; (b) drop to match the sibling's
  measured config (file-watcher path, 0 errors).
- **Choice:** (b) — feature list loses `dbus-config`; `watch_config` goes through
  `config_subscription` (file watcher) on COSMIC and GNOME alike.
- **Why:** Standing priority sandbox-correct beats simplicity, and D5 (adopt measured
  answers) beats D12's theorised distinction now that code evidence contradicts it. A
  config layer that silently stops updating on GNOME is the exact failure class G.1-5
  exists to eliminate. Re-add only on a two-build measurement showing a need.
