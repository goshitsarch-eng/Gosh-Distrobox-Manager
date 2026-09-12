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

## D24 — T0 sign-off rulings (lead, 2026-09-11, commit `a1c2556`)

T0 closed with all four gates green on the commit (`fmt --check`, `build --locked`,
`clippy --all-targets -D warnings`, `test --locked`: 70 passed). Flatpak gate recorded
N/A (no `flatpak/` surface until T2; D13 gate begins at T2/T4). Parity rows N/A (no UI
touched). `rust/data/`, `flatpak.yml`, spec byte-identical; `Cargo.lock` unmodified.
Three agent disputes ruled on, all evidence-verified by lead:

- **UX-1 rail erratum — UX agent sustained, REVIEW corrected.** `home_screen.dart:65,72-74`
  sets `extended: maxWidth >= 1000` at the same threshold as `labelType: none`; per the
  Flutter contract `labelType` governs only the *unextended* rail, so labels are visible
  at every width. REVIEW §C UX-1's "hides labels" premise is wrong; ux.md §1's premise
  stands. No plan change (COSMIC `nav_model` labels satisfy parity trivially). REVIEW.md
  itself is a frozen record and is not rewritten — this entry is the erratum.
- **Pointer count — both agents right, different frames.** REVIEW A8's "three" =
  libcosmic rev + the two unpinned git deps (`dbus-settings-bindings`,
  `freedesktop-icons`); the pkg agent's "five" adds the two submodules (`iced`,
  `cosmic-icons`). T2's sidecar covers all five git sources (PLAN §1 already says
  five; packaging.md §1.3 now lists all five).
- **`xdg-config/cosmic:rw`, not `:ro` — pkg agent sustained, D12 amended.** The `:ro`
  in D12/PLAN-§3 was a bad sibling copy: the sibling only *watches* COSMIC keys, while
  we *persist* our own settings (the GSettings replacement) — `:ro` would fail every
  write silently in-sandbox. packaging.md §1.4 carries the `:rw` rationale; T2's
  manifest uses `:rw`.
- **PNG figure — REVIEW's "~5 MB" refers to no text in any doc** (verified by grep);
  the audited figure is 65 files / 3.57 MiB, recorded in ux.md §6. Not reproduced.
- **Process deviation (accepted):** T0 landed as one commit, not fmt/lint/CI separable
  commits — fmt and lint fixes touch the same files and cannot be separated post-hoc.
  Reviewability was achieved instead by hunk-by-hunk lead review (all 9 lint fixes
  verified semantics-preserving; the two sensitive spots, `AppState::Default` →
  `new()` delegation and the `ResponseFn`/`ResponseMap` aliases, are pure).

## D26 — T2 sign-off rulings (lead, 2026-09-11)

T2 closes with the Rust gates green (unchanged tree: `fmt --check` exit 0; no Rust
source touched) and the packaging gates verified by lead-measured evidence, not by
agent report. One agent dispute ruled on; the reviewer's remaining items are
dispositioned as T3/T4 obligations or doc follow-ups, not T2 blocks.

- **Reviewer O1 (blocking) — sustained in premise, overruled in remedy.** The premise
  is correct and independently confirmed: `Cargo.lock` at T2's commit holds zero git
  sources, so the committed `cargo-sources.json` (363 registry-only entries,
  `--check` exit 0) exercises no git path, and no `[[bin]]` exists yet — a green
  `--check` at T2 cannot certify §1.3's git machinery. But the fix the reviewer
  proposed (re-scope T2 to fixture tests, or fold T2 into T3) is moot: the owner
  already did the work both options were trying to force. Measured lead evidence:
  (a) the committed sidecar is byte-identical to the output of `--refresh-git`
  against a scratch lockfile resolving libcosmic at the pinned rev with PLAN §1's
  feature list — 11 remotes / 51 packages, `git-manifests/` 51-for-51 with no
  missing/orphan dirs, qualifier kinds `{rev, tag, (none)}`; (b) the generator is
  16 functions, same set as the sibling's HEAD, 38-line diff confined to path
  defaults plus the pointer-vs-source doc header; (c) a surrogate offline
  `flatpak-builder` build (identical manifest modulo source paths, identical
  finish-args/build-options/`cargo-sources.json` git content) finished
  `Compiling gosh_distrobox_manager v1.0.2` → `Finished release in 1m 35s`,
  EXIT=0, with both submodule checkouts logged
  (`iced @ ffe1f1d`, `cosmic-icons @ 343c007f`). The sidecar is therefore validated
  for the exact lockfile state T3 will produce; what remains is *regeneration*,
  not *re-proof*. packaging.md §2.5.1's sequencing note records this honestly:
  "`--check` is green today, but the artifact it guards is not yet the right one."
- **Reviewer O2 (git-pin staleness) — moot, no ruling needed.** The committed
  manifest uses local `file`/`dir` sources, not the `{"type": "git", "commit":
  "<TAG_COMMIT_SHA>"}` placeholder from the pre-T1 draft. The Flathub switch is a
  release-checklist item (packaging.md §1.2 notes + §4.2), not a per-task staleness
  source. `verify.sh` builds the working tree at every task by construction.
- **Reviewer O3 ("five git sources") — erratum issued, owner already corrected.**
  D24's "T2's sidecar covers all five git sources" conflated documented *pointers*
  (libcosmic rev + 2 submodules + 2 unpinned deps) with lockfile git *sources*.
  Measured: **11** distinct `source = "git+…"` strings (libcosmic rev; the two
  unpinned deps; 8 transitive pins inside the iced subgraph: cosmic-protocols rev,
  atomicwrites, winit/cosmic-0.14 tag, softbuffer/cosmic-4.0 tag,
  window_clipboard/sctk-0.20 tag, smithay-clipboard/sctk-0.20 tag, cryoglyph rev,
  accesskit/cosmic-0.14 tag). packaging.md §1.3 now carries the measured 11-row
  table; the generator itself was always count-agnostic and needs no change. The
  "five" survives only as the count of human-facing pointers in PLAN §1.
- **T3 obligation (recorded, not optional):** the moment `app/` lands, its owner
  runs plain `python3 flatpak/generate-cargo-sources.py` (offline, against the
  committed sidecar — no `--refresh-git` needed if the lockfile git set matches
  the 11 predicted) and commits the regenerated `cargo-sources.json`. If the
  lockfile git set differs (iced pointer moved, unpinned deps resolved elsewhere),
  the owner runs `--refresh-git` (network) and commits sidecar + manifests +
  sources together. Either way `--check` must exit 0 before T3 closes. PLAN T3 row
  amended accordingly.
- **T4 obligations (recorded):** (a) wire job 2 (`packaging-metadata`) per §2.5.1 —
  `--check` first, then desktop/metainfo validation, then `check-versions.sh`;
  (b) `.gitignore`: add `repo/` (the §2.1 `--repo=repo` output; `.flatpak-builder/`
  is self-ignored by the tool's own `.gitignore`, `build/` already covered);
  (c) decide the generator-test runner — the sibling ports `pytest tests/ -q`
  (`tests/fixtures/git-deps/` + `test_packaging.py` from its HEAD) but D13 lists
  no pytest stage; T4 either adds the stage or folds the assertions into
  `verify.sh` as plain python, and records the choice in D13's stage list;
  (d) `flatpak` job (tags + dispatch): `--user --install` so the smoke test's
  `flatpak run` has an installed app (stage 6 as sketched with `--repo=repo`
  alone never installs); (e) canonical manifest stays the JSON — T4's D17
  negative case derives the variant via `json.load` → drop
  `--talk-name=org.freedesktop.Flatpak` → re-dump.
- **Doc follow-ups (not gates):** packaging.md §4.1/§4.2 still say `rust/Cargo.toml`
  and `rust/data/` (stale post-T1; T14 owns the version-unification rewrite);
  §6 Q1 ("BaseApp worth it? — decide before writing the manifest") is answered by
  the committed manifest (`base: com.system76.Cosmic.BaseApp`) but the question
  text still reads open — T14 closes it with one line noting the sibling's
  no-base-app alternative was measured and declined (icon theme via BaseApp,
  §1.1). The flat `.desktop`/`.metainfo.xml` files do not exist until §1.5 lands
  (owned by the page tasks via T14 at latest); until then the manifest's three
  `core/data/…` install lines fail — sequencing documented in §1.2 notes + §2.5.
- **Process note:** the owner's build-watch loop (`until ! pgrep -f
  flatpak-builder`) self-matched its own cmdline and would have waited out the
  600 s timeout; the surrogate build had already finished EXIT=0. No harm done —
  the log, not the loop, is the gate evidence — but future wait loops must match
  a PID file or a distinctive build-dir pattern, never a bare process-name
  substring.

- **Question:** The reviewer proved T1's `clippy.toml` (`std::process::Command::new`
  only) is a placebo against this tree: `std::process::Command` appears in `core/src`
  only in comments; the live spawn is `async_process::Command::new`
  (`core/src/fakers/command.rs:126`). REVIEW ARCH-Q10's "already violated once" story
  (`checked_run_command` re-established in `host_exec.rs`) is also false here —
  `checked_run_command` never existed in this repo's history, and
  `map_flatpak_spawn_host` only rewrites program/args without spawning.
- **Choice:** `disallowed-methods` bans **both** `std::process::Command::new` (the
  AGENTS.md rule as stated — the right regression net for a future hand-written
  spawn) and `async_process::Command::new` (the actually-live path), with the single
  `#[allow]` at the sanctioned indirection (`core/src/fakers/command.rs:124-133`)
  and a demonstrated fire-test (allow removed → error observed) before T1 closes.
- **Why:** A guard that cannot fire is worse than no guard — it certifies the exact
  failure mode (runner bypass → every Flatpak user silently broken) as "enforced".
  Same D21 failure class as the §0.2 mechanism: an answer ported from a neighboring
  codebase's history without checking this tree.
- **Spec amendment:** architecture.md §1.3 S2's "Green: no source change" now reads
  "no behavioral change" — dropping `crate-type` to the rlib default newly compiles
  the one doctest (`desktop_file.rs`, previously dead under staticlib/cdylib), so the
  added `use` line is a declared forced edit and sign-off must show Doc-tests
  1 passed alongside the 70 unit tests.

## D27 — T13 rulings: the B3 container type, and two fallback triggers that are not one

- **Question:** B3's spec says `ContainerList { containers: BTreeMap<String,
  ContainerInfo>, skipped: Vec<ParseIssue> }`, and the B3 critique called the
  `Deref<Target = BTreeMap>` that implies not just awkward but unsound — a map
  `Deref` provides no slice access, so the ~17 consumer sites it listed would need
  rewriting. B7's spec says the seven inline podman→docker blocks "route through one
  place" and leaves Q11 (trait vs helper) open.
- **Choice (B3):** `containers` is a **`Vec<ContainerInfo>`** with
  `Deref<Target = Vec<ContainerInfo>>`; `TolerantList<T>` carries the other four.
- **Why:** The critique and the plan were both partly wrong. The `BTreeMap` was
  never observable: `Distrobox::list` already returned one, but `service.rs`
  already did `map.into_values().collect()` at the `Backend` boundary, so the map
  was erased before any consumer saw it. Under a `Vec` the `Deref` leaves the slice
  consumers and every `self.containers.*` field/method read unchanged; under the map,
  the critique's breakage list is real. The one consumer edit is the `for` loop in
  `view_containers_page`, which needs `.iter()` because `Deref` does **not** provide
  `IntoIterator for &ContainerList` — so "zero churn" is one line, not none. The `Vec`
  also preserves the name-sorting the `BTreeMap` happened to provide (explicit
  `sort_by`), so no ordering regression rides along.
- **Choice (B7):** the helper branch of Q11 — `Distrobox::runtime_output(cmd,
  Fallback)` over `container_runtime::retarget`, with `ContainerRuntime` left at
  three methods.
- **Why:** all six sites are output commands over argv `Distrobox` already
  builds, so the trait would re-encode the same argv in two more places; and
  `ContainerRuntime` is `#[async_trait(?Send)]`, so growing it would force a `Send`
  conversion for operations that never needed one.
- **Correction — B7's "seven" was wrong.** Counted in `HEAD`, exactly **six**
  functions carried a docker fallback: `start`, `get_container_id`, `create_snapshot`,
  `list_snapshots`, `delete_snapshot`, `get_container_stats`. `export_container` and
  `import_container` are podman-literal and **never had a docker branch** — they are
  *streaming* (`cmd_spawn` → `Child`), so `runtime_output`'s `String` cannot serve
  them at all, and they are untouched for the same reason they were untouched before.
  All six convert; the "two stayed literal" note is about sites the spec mistakenly
  counted among the seven, not about sites this refactor declined.
- **`retarget` is not `map_docker_to_podman`.** That helper
  (`podman.rs:17`) is `if command.program == "docker" { program = "podman" }` — a
  one-way `docker → podman` map on the *program field only*, installed by
  `Podman::new` (`podman.rs:87`) so a `Docker` backend's commands reach the podman
  runner. It cannot produce a docker retry: it points the other way, which is exactly
  the bug `runtime_output` documents (building the retry through a
  `Podman`-constructed runner would rewrite it straight back to podman). It also
  **cannot** touch arguments, so it would not rename a container named `docker` —
  that was an earlier draft of this note and it was wrong. `retarget` chooses the
  target program and leaves the argv byte-identical, which is the property the
  `list_snapshots` test pins.
- **Spec amendment — the two triggers are not interchangeable.** B7 says "one place",
  which reads as one policy; the originals had two. Five sites retried on *error*;
  `get_container_id` retried only on *empty output* and **propagated** a podman error
  (its podman branch ended in `?`, so docker was never consulted). Collapsing them
  would answer a broken podman install with a misleading "container not found", or
  hand back a same-named container from a different runtime's store. `Fallback::{
  OnError, OnEmpty }` names both, and a test pins each.
- **The one behaviour change, declared:** `create_snapshot` originally trimmed only
  its podman branch and returned the docker retry's output raw. The unified helper
  trims both. A stray newline in a returned image ID is not worth a second code path
  to reproduce; noted at the call site rather than buried.
- **Also:** `show_skipped_lines` and the Dashboard caption it gates get **no parity
  row**. D19 freezes rows 1–193 as the Flutter-parity set, and this capability has no
  Flutter counterpart to reach parity with — the Flutter app had no tolerant parser, so
  there was nothing to report and no row to tick. It is recorded instead as **I13** in
  PLAN.md §4 (migration-introduced changes) with a note in ux.md §6.13 pointing there.
  An earlier draft of this decision minted ux.md ids 172–173 for it; those ids are owned
  by §6.14 (Apps page), so minting them broke the frozen 1–193 sequence. Blank lines are
  not parse issues: a trailing newline must not read as a skipped row.

### D27 rulings from the T13 advocate pass

- **Correction — the header skip was positional, and that predated T13.** `list()` did
  `text.lines().skip(1)`, which removes whatever line happens to be first. It is
  verifiable — `distrobox-list` prints a *fixed literal* header (1.8.2.5, line 231:
  `printf "%-12s | %-20s | %-18s | %-30s\n" "ID" "NAME" "STATUS" "IMAGE"`) and every row
  after it is `id|name|status|image` — so the skip now tests for those four literals by
  identity. Two consequences the positional form had, both gone: a headerless response
  silently lost its first real container (no log, no count, no UI trace — the exact
  silent-loss class B3 exists to remove), and a leading non-header line shadowed the
  real header. Because it came in with T1's `git mv rust core` rather than with T13's
  spec, it is recorded as **I14**, not as a fix to B3.
- **The predicate is deliberately four-literals-and-nothing-else.** No real container row
  can satisfy it (an id is a container hash, never the literal `ID`), so it is safe at
  any position; a looser prefix test would eat a real row whose id merely *starts* with
  `ID`, which is the same silent-loss failure in the other direction. Pinned by a test
  that mutates the predicate into a prefix match and fails.
- **Correction — the `%i` rationale was wrong, and so was the re-split rationale.**
  Two drafts of my own, both retracted in place. (1) `%i` was documented as dropped
  because "this function sees only the `Exec` string, never the entry's `Icon`" — false:
  `launch_app` holds `app.entry.icon`, so the pair *can* be formed. It is dropped because
  it must not be: `distrobox-export` rewrites `Icon=` to a host-side absolute path under
  `/run/host` (1.8.2.5, lines 582-590) that the containerised program cannot open, so
  `--icon <host path>` is worse than no `--icon`. (2) `split_exec`'s doc claimed
  `distrobox-enter` "had to re-split" the fused string. It does not: the `--` branch ends
  in `exec "$@"` with no `eval` and no re-split, so the old one-string form asked for a
  program literally named the whole fused string and **failed to launch**. Right
  conclusion, wrong mechanism, in both cases.
- **`launch_app` now refuses an `Exec` that leaves no command.** `%u`-only or empty
  `Exec` used to drain to a bare `enter --name <box> --` — an interactive shell in the
  container where the user asked to launch an app. It is a typed `Err` naming the entry
  instead (**I15**): a silent wrong action is worse than a visible refusal, and the
  signature can report it.
- **`is_clean_empty()` now has a production caller, and the review's framing of its
  defect was half right.** The method's doc claimed the UI must not render "every row
  failed" as "no containers yet", while the Containers page tested `.is_empty()` — the
  doc was aspirational. Worse, the *other* two container-gated pages (Backups, Packages)
  had the same defect and the review had not listed them. All three now branch on
  `is_clean_empty()` and share one helper (`views::container_list_copy`), which picks the count
  and the singular/plural wording; only the Containers page offers a `Refresh`, since
  retrying is what its unreadable list might answer to. Recorded as **I16**.
- **`api.rs` dropping `skipped` is deliberate, not an oversight.** That module is the
  FRB shim and is an **S7 deletion target in T14**; it erases `ContainerList` to
  `Vec<ContainerInfo>` for the same reason `service.rs` does, and B3's `skipped` is
  specified against the `Backend` boundary. Investing log plumbing in a file scheduled
  for deletion would be work discarded within the same phase.
- **Test-quality rulings — and one reviewer premise that was wrong.** Three app/core
  tests could not fail and were rewritten to fail on a real mutation. `entry_round_trips_core_config`
  round-tripped `AppConfig::default()`, where the field is `false`, so the round trip
  proved nothing; it now starts from non-default values on every field. `entry_default_matches_core_default`
  compared `PrefsEntry::default()` to the very expression that *defines* it
  (`Self::from(&AppConfig::default())`) — a tautology; it now asserts literals plus
  parity against the core default. The "peers stay Vec" half of the B3 boundary test was
  dead code behind an `if let Ok` whose fixture never registered the peer's commands, and
  `let _: &Vec<T> = &apps` is a deref coercion that a wrapper type also satisfies — it now
  names the type at the binding and lets a fixture failure fail the test. **But** the
  reviewer's claim that *dropping* a field from `From<&AppConfig> for PrefsEntry` would
  leave tests green is wrong: both impls are struct literals, so a dropped field is
  `E0063` and does not compile. The real hazard is a *wrong value* — hardcoding
  `show_skipped_lines: false` on the write-back, or swapping `PrefsEntry` to a derived
  `Default` — and both are now caught by test. Every new test in this pass was
  mutation-checked (six mutations, six failures) rather than assumed to bite.
- **`%i` drops as a *formable but forbidden* pair, not a capacity limit.** Correction to
  an earlier draft of this bullet: `ExportableApp::entry.icon` **is** in scope at
  `launch_app`, so `--icon <name>` could be emitted with no signature change at all. It is
  dropped because emitting it would be wrong — `distrobox-export` rewrites `Icon=` to a
  host-side path under `/run/host` (1.8.2.5, lines 582-590) that the containerised program
  cannot open. A future caller with a genuinely container-usable icon should still add its
  own expansion rather than relax this list, since the list is keyed by code, not by
  context.
- **`list_installed_packages` stays intolerant of an unknown package manager — that is a
  prerequisite failure, not a row failure.** The review read the hard error as
  inconsistency with its peers; it is the same rule they follow. `detect_package_manager`
  returning `Unknown` means the detect script's `else` branch fired — no apt/dnf/pacman/
  apk/zypper/xbps/emerge in the container — so *no* rows can be produced, and
  `Ok(vec![])` would report "0 packages installed" for a container that may have
  hundreds. Every peer draws the line identically: a missing podman is an `Err` too.
  Only row-level refusal is tolerated (a tab-less line becomes a `ParseIssue`, which it
  already did). Recorded in the fn's own doc so the next reader does not "fix" it.
- **Correction — the stale-citation finding splits in two, and my first ruling of it
  was wrong on the facts.** The direction matters, and the earlier bullet conflated them.

  *Into the docs: inherited, still T14's.* `architecture.md` carries **161** source-line
  references — **83** written `file:line` and **78** bare `:NNN` continuing a nearby
  filename (e.g. row B1's `distrobox.rs:1215`, `:1316`, `:1348`). Across the six
  migration docs the `file:line` form totals **151**. Of these, **10** name the
  pre-rename `rust/src/` path that T1's `git mv rust core` killed — `rust/src/api.rs` ×3,
  `rust/src/backends/distrobox/distrobox.rs` ×3, `rust/src/app_state.rs` ×2,
  `rust/src/backends/host_exec.rs` ×1, `rust/src/backends/supported_terminals.rs` ×1.
  (The earlier draft's "79" is nearest the **78** bare refs but matches no measurement I
  can reconstruct; the figures above are counted, one grep each, and every one of the 161
  is stale-prone the same way, so all of it goes to T14 together.) T13 added none of them
  — `git diff -U0` over the doc shows the task introduced **zero** new source-line
  references — so they stay filed with T14, which already owns the docs rewrite.

  *Out of the Rust sources: caused by this diff, fixed here.* Five sites in `core/` and
  `app/` cited architecture.md **by line number**, and T13's own architecture.md edit
  (the B-row table plus the §2.3 struct) invalidated four of them, each shifted by a
  different one of my hunks: row **B4** 1206→1223, row **B7** 1209→1226, the
  `show_skipped_lines` key 1010→1015, and the `"12 containers, 3 rows skipped"`
  comment 425→430. (The `containers:` field cited alongside it moved 561→566.)
  All five now cite **section names** instead — `§6.4, row B4` / `row B3` / `row B7`,
  `§2.3`, `§5.3` — so nothing in the Rust tree pins a doc line number any more and the
  next doc edit cannot re-break them. (The claim in the earlier draft that
  `architecture.md` "uses `§NNNN` as line-number notation throughout" is simply false:
  it uses `§N.N` **section** numbers exclusively, and a scan for `§` followed by three or
  more digits matches **nothing** in any migration doc. The line-numbered references were
  always the *reverse* direction, doc-ward from the Rust files, which is precisely why
  this task could invalidate them and why they were this task's to fix.)

  *Two more defects the audit surfaced while fixing the above, both fixed.* (1)
  `distrobox.rs:1708` cited **§6.9** for the B7 helper. No such section exists, and none
  ever did — `git log -S` finds no heading by that name in the file's whole history, and
  architecture.md's §6 runs 6.1–6.4 only. It now cites §6.2 (which names both
  `runtime_output` and `retarget` by hand) plus §6.4 row B7. (2) Eight references in
  `app/` — `§3.5`, `§3.6`, `§4.5` — were *unqualified* while resolving to **ux.md**, not
  architecture.md; architecture.md happens to define §3.1–§3.4 and §4.1–§4.3, so a reader
  who resolved them against the wrong file found either nothing (§3.5, §4.5) or the wrong
  section. All eight now read `ux.md §3.5` etc. A re-audit resolving every `§` reference
  in the Rust tree against the doc named on its own line returns clean.
  (3) Two more inconsistent forms, both swept: `§6.4-B3` (3×, in `service.rs`,
  `message.rs`, `app.rs` — an anchor form the doc never uses) is now `§6.4, row B3`, and
  the bare `arch §` shorthand (2×, against **26** spellings of `architecture.md §`) is
  spelled out, since "arch" is also a role label throughout PLAN.md and the `arch/` crate
  name is one keystroke away. Rows B3/B4/B7 are cited identically in all five places now.
- **`Exec` needs the spec's *value* escapes applied before tokenizing, and the
  CR-not-whitespace exception is the subtle half.** `Exec` carries two escape layers in a
  fixed order: the spec's general *string* escapes (`\s`→space, `\n`→LF, `\t`→TAB,
  `\r`→CR, `\\`→`\`), applied to the value as read, and then the `Exec` key's own
  quoting/tokenizing rules. Getting the order wrong is not cosmetic — the old code
  tokenized first and passed `--dir\s"my dir"` through as the single argument
  `--dirsmy dir` (the spaces never became separators), so the app's own default export
  pattern did not launch. `split_exec` now runs the value pass **first** (pass A), which
  is also why `parse_desktop_file` deliberately leaves `DesktopEntry.exec` **raw**: it
  decodes only `Name` and `Icon`, the two values nothing re-tokenizes, because decoding
  `Exec` as well would apply the rule twice (`--name=a\\sb` → four arguments once,
  five twice — neither throws, so only a test can tell them apart; the asymmetry is
  pinned by a test that asserts the two forms *differ*).
- **Within that, CR is decoded but is NOT a separator — the one place the two layers
  disagree, and it is GLib's disagreement, not ours.** The first implementation added
  `\r` to the tokenizer's whitespace arm by reasoning from the escape table ("`\r` is in
  it, so a decoded CR must be whitespace"). That reasoning was wrong, and a differential
  test against the reference implementation caught it. GLib's `g_shell_parse_argv` splits
  on space, TAB and LF only; a CR stays **inside** the token, so
  `Exec=/bin/echo a\rb` is argv `["/bin/echo", "a\rb"]` — one element carrying a carriage
  return — while `\t` in the same position yields three elements. Separators are now
  space/TAB/LF, pinned by `split_exec_decoded_cr_stays_inside_the_argument`, which fails
  in *both* directions: add `\r` back to the separator arm and it fails; drop `\r` from
  the value table and it fails too.
- **How that was established, and how far the evidence reaches.** Not by reading the spec
  alone — which does not settle it — but by **differential testing**: 49 `Exec` values fed
  through GLib (`GKeyFile.load_from_file` for the value layer, then
  `g_shell_parse_argv` for the tokenizer, exactly the two-stage split GLib itself
  performs) against this crate's `split_exec`, comparing argv element-by-element with a
  single shared escaping so empty arguments, spaces, tabs and newlines are all visible.
  Result after the CR fix: **39 agree, 0 differ**; the 10 remaining cases are ones GLib
  *refuses to load at all* (the undefined escapes `\q`/`\u`/`\x0b`, an unterminated quote,
  a trailing backslash) — the deliberate divergence recorded at `unescape_value`, where we
  stay permissive because a malformed `Name` from a container must not cost the user the
  app. KDE Frameworks 6.29 was independently checked (source + live probes, including a
  compiled `KService`/`DesktopExecParser` run): it unescapes at *value* parse time too
  (`printableToString`, kconfig `src/core/kconfigini.cpp`), it **also keeps a CR inside
  the argument**, and it lands on the same argv for `--dir\s"my dir"` → `["--dir",
  "my dir"]` and `C:\\path` → `["C:path"]`. So both major implementations agree with the
  spec's *ordering*, and agree with **each other** on CR.
- **Two corrections the primary sources forced, both narrower than the draft that preceded
  them.** (1) The claim above was first written as "the two implementations agree with the
  spec's ordering" *and* were left to imply agreement on the whole separator set. They do
  not: **KDE's `KShell::splitArgs` splits on a literal space only** (`kshell_unix.cpp`
  compares `c == QLatin1Char(' ')` at the leading-skip, tilde-path and token-terminator
  sites — not `QChar::isSpace()`, which was my working hypothesis and is wrong), so a
  decoded TAB or LF separates here and in GLib but stays inside the argument in KDE. And
  the spec itself never says "whitespace" anywhere — it says **"Arguments are separated by
  a space"**, singular — so TAB and LF are *GLib's* addition, not a spec requirement. This
  crate follows GLib anyway (rationale at `split_exec`): the values come out of host
  `.desktop` files that the desktop's own GLib-based parser tokenizes this way, and the
  spec's reserved-character list names tab and newline, which only needs doing if those
  characters separate arguments. (2) **The spec defines no single-quote rule at all** —
  it specifies double-quote enclosing and lists `'` only among reserved characters, and
  says nothing about quoting with it. Single-quote support is therefore a deliberate
  extension matching both implementations' behaviour, not spec conformance, and is now
  labelled that way in the code rather than filed under "the spec's quoting rules".
  The harness that produced the parity numbers was temporary and is not part of the
  commit — the *findings* are, as the tests and the doc comments at `unescape_value` and
  the tokenizer's whitespace arm.
- **Line continuation: the rule is *context-dependent*, and the differential run is what
  showed it.** `\` + LF is a continuation *outside* quotes — both characters vanish and
  the token is not broken, so `a` `\` LF `b` is one argument `ab`. Inside double quotes
  the backslash is consumed and the **newline is kept**: `"a` `\` LF `b"` is one argument
  holding a real LF, byte-identical to `"a` LF `b"`. I first generalised the unquoted rule
  to quoted contexts and dropped both characters; the differential run went to `diff=1` on
  exactly that case, and a direct probe confirmed GLib collapses both quoted forms to the
  same element. The two arms now differ deliberately, and the quoted one is asserted
  *against its own unbackslashed twin* so a future "simplification" that unifies them
  fails. Reached through pass A, which is what makes it live: file text `a\\\nb` decodes to
  `a` `\` LF `b`, and GLib cancels the pair to launch `["ab"]`.
- **Empty quoted arguments are *kept*, and this was checked at the launch layer, not just
  the tokenizer.** A reviewer reported as blocking that GLib *drops* an empty quoted argv
  element, pinning our own `--dir "" --verbose` → `["--dir", "", "--verbose"]` as a
  phantom slot that shifts every later positional argument by one. GLib's tokenizer keeps
  it (`g_shell_parse_argv` → `['--dir', '', '--verbose']`), and because the claim was about
  what a program *receives* I checked the launch path too rather than accepting either
  result: `GDesktopAppInfo.launch()` against a recording script that frames `"$@"` with a
  count and per-argument lengths returns `['--dir', '', '--verbose']` and, for an `Exec`
  of only `""`, `['']`. The reported divergence does not exist. (My own first two attempts
  to measure it *appeared* to confirm the finding twice — first a `printf` whose leading
  format string was parsed as an option, then a `"$@"` capture that silently dropped the
  first element and, later, a shell-quoted `[a<LF>b]` whose newline-containing line my
  parser collapsed to the empty string. Each was a broken instrument, not evidence; the
  length-prefixed frame is what settled it. The lesson is the one this file keeps
  re-learning: a differential result is only as good as the capture pipeline.)
- **The header predicate is version-robust; the doc claim that preceded it was not.** A
  reviewer noted `is_distrobox_header` and its comment were locked to the 4-column header.
  Confirmed against upstream across four release tags: 1.5.0.2 prints the six-column form
  (`printf "%-12s | %-20s | %-18s | %-16s | %-5s | %-30s\n" "ID" "NAME" "STATUS" "MEM"
  "CPU%" "IMAGE"`, lines 192-193) and the 4-column form arrives at 1.6.0.1 and is
  unchanged through 1.8.2.5. On a 1.5.x host the real header failed the predicate, was
  parsed as a data row, and was counted as a *skipped container* — copy asserting a
  malformed row that does not exist. The predicate now keys on field 0 with an arity floor
  (field 0 is `"ID"` and there are at least 4 fields), which accepts both layouts and still
  rejects a container *named* `ID…`, and the doc cites both versions instead of one.
- **I16 extends past the three pages it named — the Dashboard and Updates are the two it
  reached last, and they are the two a user actually lands on.** Reviewers found the
  Dashboard's status card and containers preview still branching on bare `is_empty()`, and
  the Updates page rendering its own create-prompt from a `&[ContainerInfo]` signature
  that had no access to `skipped` at all. All three now route through pure helpers that
  **take no `show_skipped_lines` flag** (`dashboard_status_body`, `dashboard_preview_copy`,
  `all_rows_failed_copy`) — deliberately, because the gate belongs to the caption, not to
  the *lie*: the first attempt threaded the preference through and a mutation that gated
  the copy behind it compiled and passed every test, so the API was reshaped until the
  mutation failed. `view_updates` now takes `&ContainerList` and surfaces a partial-skip
  caption ungated. The Dashboard is also the one page where the contradicting pair could
  co-occur in a single card (caption "1 row skipped" two lines above "No containers
  configured"), which is what made the earlier partial fix look complete.
- **`api.rs` keeps its inherited gap, deferred to T14 on purpose.** The FRB shim logs the
  aggregate skip count for `get_containers` but drops it unlogged for its four peers
  (`list_installed_packages`, apps, exported binaries, snapshots). The finding is accurate
  and the fix is four copies of a line that already exists — but that module is the FRB
  shim and is an **S7 deletion target in T14**, which rewrites the file wholesale; adding
  logging to code scheduled for deletion buys nothing and would have to be re-verified
  against a new surface anyway. Recorded here rather than fixed, so the decision is visible
  instead of looking like an oversight.
