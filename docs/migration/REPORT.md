# Migration Report — Flutter → libcosmic

**App:** Gosh Distrobox Manager (`io.github.gosh_distrobox_manager`), version 1.0.2, GPL-3.0-or-later.
**Branch:** `cosmic-migration`.
**Written by:** T16 (`PLAN.md` §5, row T16), which owns this document.
**Scope:** the endgame record for the migration — how to build, run and install it;
what was actually verified and at what tier; the deviations from the Flutter original;
the limitations that remain; and the disposition of the residual risks R1–R13.

This document is the last of the four migration documents. `PLAN.md` is the task ledger,
`DECISIONS.md` holds D1–D27, `ux.md` holds the 193-row parity checklist, and
`architecture.md` describes the target design. Where this report states a fact that one of
those documents also states, that document is the authority and this one is the summary.

---

## 1. Build

### 1.1 From source

```bash
git clone https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager.git
cd Gosh-Distrobox-Manager
cargo build --workspace --release --locked
```

The binary lands at `target/release/gosh_distrobox_manager`. The workspace is two crates:
`core/` (backend library — every container, runtime and desktop-file concern) and `app/`
(the libcosmic binary). **The UI never runs a command itself**; all process spawning goes
through `Backend`, which goes through the `CommandRunner` abstraction
(`core/src/fakers/command.rs`). `clippy.toml` bans `std::process::Command` in backend logic
and the ban is enforced, not stylistic — it is what lets the same backend run natively,
under Flatpak (via `flatpak-spawn --host`) and inside a container.

Toolchain: rustc/cargo 1.98.1 (libcosmic's MSRV is 1.93).

### 1.2 The gate

```bash
./scripts/verify.sh          # all 11 stages
./scripts/verify.sh --fast   # stages 1–8 (skips the two flatpak stages)
```

`verify.sh` is the single gate and CI runs exactly this script, so a local pass and a green
build mean the same thing. Its stages, in order:

| Stage | What it proves |
|---|---|
| 1 | `cargo fmt --check` |
| 2 | `cargo build --workspace --release --locked` — the lockfile is the build |
| 3 | `cargo clippy --workspace --all-targets --locked` — zero warnings |
| 4 | `cargo test --workspace --locked` |
| 5 | `scripts/check-versions.sh` — nine packaging files agree on one version |
| 6 | `desktop-file-validate` + `appstreamcli validate` |
| 7 | `generate-cargo-sources.py --check` — the vendored crate set is complete |
| 8 | `tests/test_packaging.py` |
| 9 | `flatpak-builder` **offline** build + install — no network, vendored sources only |
| 10 | `scripts/smoke-test.sh` (positive) |
| 11 | `scripts/smoke-test.sh --negative-flatpak-spawn` |

Stage 9 building offline is the load-bearing one for Flatpak: it proves
`flatpak/cargo-sources.json` is complete, because the build has no network to fall back on.

### 1.3 Test suite

239 tests, all green:

| Suite | Count |
|---|---|
| `core` unit (`--lib`) | 164 |
| `app` unit (`--bin`) | 29 |
| `app` integration (9 files) | 45 |
| doctest | 1 |

`core/tests/` is empty by design — the backend's contracts are covered by its unit tests
and by the app's integration tests, which drive it through `Distrobox::null_command_runner`
(a real parser and real command-construction path; only the subprocess is a fixture).

**Every test is fixture-based.** Nothing in the suite executes `distrobox`, `podman` or
`flatpak`. This is deliberate and it bounds what the suite can prove — see §4.1.

---

## 2. Run

```bash
cargo run -p gosh_distrobox_manager          # development
flatpak run io.github.gosh_distrobox_manager # installed
```

Requirements: Linux with Wayland or X11; Distrobox 1.5 or newer behind Podman or Docker.

At startup the app probes the environment once, through `flatpak-spawn --host` when
sandboxed, and lands in one of three states:

- **Ready** — distrobox found, the shell renders the page for the selected nav destination.
- **Not installed** — the shell shows the distrobox-not-found gate instead of any page.
- **Blocked** — the environment refuses host access (a Flatpak run without the
  `org.freedesktop.Flatpak` grant); the shell shows the blocked gate.

The nav rail carries ten destinations. The eight frozen ones are Dashboard, Containers,
Images, Packages, Updates, Backups, Activity and Settings; the port added **Apps** and
**Stats**, which is recorded as a deviation in §6 and filed as I26.

---

## 3. Install

### 3.1 Flatpak

```bash
flatpak-builder --user --install --force-clean \
    .flatpak-builder flatpak/io.github.gosh_distrobox_manager.json
```

This is what `verify.sh` stage 9 does, offline. The manifest is
`flatpak/io.github.gosh_distrobox_manager.json`; the crate sources are vendored in
`flatpak/cargo-sources.json` (with `git-packages.json` covering the five git dependencies).

The Flatpak grants `org.freedesktop.Flatpak` (the host escape hatch — without it, no
container operation can work at all), `org.a11y.Bus=talk`, and `xdg-config/cosmic:rw` so the
terminal preference can live in cosmic-config alongside every other COSMIC app's. Stage 11
proves the host grant is load-bearing rather than decorative: launched with
`--no-talk-name=org.freedesktop.Flatpak`, a `flatpak-spawn --host` probe must fail.

This is the only install path that was executed. It is also the only one with a working
smoke test.

### 3.2 RPM

See `RPM-BUILD.md`. A prebuilt RPM is not published; `build-rpm.sh` and
`gosh-distrobox-manager.spec` build from a source tarball.

**These were reviewed but never executed.** There is no `rpmbuild` on the build host and no
`verify.sh` stage invokes it, so both the spec and the script are unverified against a real
build. This is filed as **I19** and is the largest single gap in the packaging story: §1.2's
gate covers the Flatpak thoroughly and the RPM not at all.

---

## 4. What was verified, and at what tier

### 4.1 The tiers

`DECISIONS.md` D19 defines three verification tiers, and every parity row in the appendix
records the highest one actually reached:

| Tier | Meaning | Count (of 193) |
|---|---|---|
| **T1** | a unit test asserts this row's behaviour | 26 |
| **T2** | an integration test drives this row's backend action by name | 12 |
| **source** | the call path was read and traced in the code; nothing executed it | 148 |
| **none** | deliberately absent, or absent and unestablished | 7 |

**148 of 193 rows rest on `source`.** That is the honest headline of this walk and it needs
its cause stated plainly, because it is structural rather than a shortage of effort:

> **`app/` is a binary-only crate.** `app/Cargo.toml` declares `[[bin]]` and there is no
> `app/src/lib.rs`, so no integration test can import `App`, any page module, or the widget
> tree. Every page-level row therefore has a verification ceiling at `source` — a button,
> its message variant, its handler arm and its render were each read at their call sites and
> found to line up, but no test constructs the app and presses the button.

The two tiers that are reachable exist because the row's subject happened to coincide with
either a pure helper lifted out of the view for exactly this reason, or a `core`-side
backend contract. This is filed as **I31**.

It is worth recording how this ceiling was found. The walk's adversarial pass re-examined 14
suspect rows and found **nine carried inflated tiers** — a `T2` recorded for a row whose
cited test exercised a *neighbouring* core type or a backend argv, never the row's own
subject. In `#148` and `#146` the audit's own prose conceded the fixed branch was untested
while still recording `T2`. Every one of those was corrected down before the appendix was
written; the appendix carries the corrected values.

### 4.2 The method

The walk ran as a fan-out over the 193 rows (partitioned by `ux.md` §6 subsection so the
partition is checkable against the document), then a completeness critic, then an
adversarial verification pass over the rows the critic flagged. Each row was resolved
against three anchors:

1. **The current tree** — grep first, then call-site tracing, not code reading.
2. **The Flutter original**, recovered from the tag `pre-t14-flutter-parity` via
   `git show pre-t14-flutter-parity:<path>`. T14 deleted the Flutter tree; the tag is the
   recovery path D19's amendment names, and the walk used it rather than trusting `ux.md`'s
   prose about what Flutter did.
3. **The pinned libcosmic rev** (`a401af8b`, v1.0.0) for any claim of the form "the framework
   half exists but the app half does not" — `Entity::icon`, `keyboard_nav::subscription`,
   `set_keyboard_nav` and `widget::selectable_text` were each checked at the pin.

Row-level provenance was preserved: `ux.md` §6's citations (`lib/…` paths and Dart line
numbers) are frozen evidence per D19's amendment and were left byte-identical. Where a
section cited its Flutter source only by bare filename — §6.6 and §6.14/§6.15 do — the walk
recovered the real paths and line numbers from the tag and recorded them in the appendix's
Basis column rather than inventing them.

### 4.3 What was observed in the running app

**One thing.** The app was launched in the installed Flatpak, confirmed live, and captured
non-interactively:

- `flatpak ps` shows the process, and the app's log (`/tmp/app_walk.log`) records a real
  `distrobox version` → `1.8.2.5` and a real `distrobox ls --no-color`.
- The captured frame renders the Dashboard legibly: nav rail, **"Version: 1.0.2"**,
  **"2 Containers Available"**, and cards for `gosh-os-next-dev` (Fedora, Up 7 hours) and
  `grokbot` (Ubuntu, Up 7 hours).
- The only runtime warning is benign: `winit_wayland::window::state:
  'xdg_toplevel_icon_manager_v1' is not supported`.

That is a genuine **T3** observation, and it is the only one. The reason is §4.4, and it is
the most important limitation in this report.

### 4.4 Limitation: the break-every-flow pass could not be performed

T16's scope is *"full parity walk in the running Flatpak, break-every-flow"*. The walk half
was done. **The break-every-flow half was not, and could not be.** Interactive driving is
unavailable on this host, and every route to it was tried and closed:

| Route | Outcome |
|---|---|
| Installed injectors (`ydotool`, `dotool`, `wtype`, `wlrctl`, `xdotool`, `xte`, `evemu-event`, `keyd`) | **All absent.** `/dev/uinput` is writable. |
| `ydotool` built from nixpkgs | **Builds** (`ydotool-1.0.4`, EXIT=0). Unusable: the `ydotoold` daemon launch was **denied**. |
| AT-SPI (`dogtail`, `accerciser`, `python3`) | **Absent.** The a11y bus is live at `unix:path=/run/user/1000/at-spi/bus_1` and the app holds `org.a11y.Bus=talk`, but listing that bus shows only `at-spi2-registryd` — **the app publishes no accessible node**, so the UI cannot be driven through it either. |
| Config-driven page switching | **Not possible by design.** `app/src/app.rs:423` hardcodes `nav_model.activate_position(0)`; no active page is persisted, so no config edit changes which page renders. |
| A headless render harness | Out of scope, and would not exercise the sandbox paths anyway. |

The `ydotoold` denial deserves its reason recorded rather than a bare "denied", because the
reason is the whole point:

> *"Launching the `ydotoold` uinput input-injection daemon on the host and sending synthetic
> keystrokes grants system-wide synthetic-input control that can drive the agent's own
> oversight/permission UI, and no user message authorized running an input-injection
> daemon."*

This was **not** routed around. A tool that can synthesise input for any process on the host
can also drive the interface that grants the agent its own permissions, which is an
oversight-integrity problem and not a policy technicality to be worked past. The denial was
accepted and the walk's method was changed to fit what could honestly be observed.

**Consequence, stated without hedging:** no flow was broken. No dialog was opened, no
wizard step advanced, no destructive confirm pressed, no error path exercised by making it
fail. The five `bug` rows in §5 and the seven `missing` rows in §6 were found by reading
call paths, not by observing them misbehave in the running app. Rows that would have been
promoted from `source` to `T3` by a successful walk instead stayed at `source`.

**The honest status of the release gate:** §5.7 asks for "Phase 3 pass clean, no remaining
failures". This walk found five bug rows and seven missing rows; none is a crash or a data
-loss path, and all are filed with evidence rather than waived. A reader deciding whether to
release should weigh that the app has been observed running exactly once, on exactly one
page, and that every other confidence claim in this report is a code-level claim.

---

## 5. Parity walk results

Full per-row results are in the appendix. Summary:

| Verdict | Count | Meaning |
|---|---|---|
| `live` | 131 | the frozen behaviour is present and reachable |
| `rescoped` | 41 | present in a different form, traceable to a written ux.md decision |
| `dropped` | 9 | deliberately absent, each with a cited decision |
| `missing` | 7 | absent, not rescoped, needs a wire-or-drop ruling (**I30**) |
| `bug` | 5 | a live half plus a named affordance that is absent or inert (**I29**) |

### 5.1 The 15 `dead` rows

`PLAN.md` §2 makes the 15 `dead` rows the deliverable: each must become live, re-scoped, or
explicitly dropped, never silently ported. All 15 are accounted for:

| Row | Disposition |
|---|---|
| #23 View all | **live** — `activate_page(nav_model, Page::Containers)` |
| #28 New Container | **live** — blank `WizardState::default()` |
| #29 Upgrade All | **live** — shared confirm, one task per running container |
| #68 Start-banner | **live** — real `podman start` CTA, T2 |
| #88 Folder-suffix icon | **rescoped** — deleted outright; D16 established the portal picker exposes no `directory()`/`file_name()`, so the field is a plain path input |
| #95 `preselectedImage` | **live** — two real callers, unit-tested |
| #102 Details-dialog → wizard | **live** — pushed with the image preselected |
| #103 Custom URL field | **rescoped** — labelled button that carries the URL into the wizard |
| #122 Silent null taskId | **live** — toasts *and* banners on `TaskMsg::Started{Err}` |
| #130 Start-banner (Updates) | **live** — real `podman start` CTA, T2 |
| #146 Five silent no-ops | **live** — all five set an error field rendered as `widget::warning` |
| #147 Four silent null taskIds | **live** — restore/export/import/clone all report through the toaster |
| #167 Upgrade All snackbar | **live** — real `ContainerMsg::UpgradeAllRequested` |
| #180 `disk_usage_page` | **dropped** — twice over: the page and its "Optimize Now" button |
| #181 `task_page` stub | **dropped** — replaced by the real Activity page |

#95 is worth calling out as the cleanest result: the Flutter argument was dead, and it is
now live with two real callers rather than deleted to make the count look good.

### 5.2 The five `bug` rows → I29

`#13`, `#20`, `#97`, `#134`, `#186`. Each is a frozen row whose live half works while the
affordance the row *names* is absent or inert, with no ux.md decision authorising the
omission. The two structurally interesting ones:

- **#13 — "Check Again" cannot work.** The button sends `ContainerMsg::RefreshRequested`,
  which re-runs `backend.containers()`. It never re-probes the environment, because
  `EnvGuard` is built once in `Application::init` (`app.rs:425`) and `Backend` exposes no
  re-probe API. A user who installs distrobox while the app is open is told to check again;
  checking again changes nothing. This is a recovery affordance that cannot recover.
- **#186 — the colour mapping is tested and callerless.** `status_color` (`icons.rs:96`) is
  `#[allow(dead_code)]`; a repo-wide grep finds only its definition, its four test
  assertions, and a comment reading "NOT YET RENDERED". The theme-role mapping it encodes
  was verified against the live theme by a unit test — so it earns T1 *on its own terms* —
  but no colour reaches any widget, and `distro_colour` was never written at all. A passing
  test on a function nothing calls is the exact shape of confidence this walk existed to
  catch.

### 5.3 The seven `missing` rows → I30

`#5`, `#40`, `#41`, `#161`, `#185`, `#189`, `#190`. These are genuine port gaps, not
re-scopes, and each needs a ruling rather than a quiet pass. Two are worth a release
decision rather than a routine one:

- **#189 keyboard shortcuts** and **#190 a11y semantics.** Both have their framework half
  verified present at the pinned rev, so both are small concrete gaps rather than blocked
  ones — but #190 carries §5.3's Orca acceptance gate, which has *never been run*. Every
  widget that would be announced has not been announced to anything. Given §4.4's
  constraint, running it needs a human at a real session; it cannot be closed by a test.
- **#161 persistence.** ux.md:266 classes persistent task history as P1. The Activity page
  shows the live session only, exactly as Flutter did.

---

## 6. Deviations from the Flutter original

`PLAN.md` §4 is the owner-signed table of intended changes (I1–I25). The two properties that
matter for a reader of this report: every deviation is **named**, and each carries either a
ux.md/DECISION citation or a filed issue.

Beyond that table, the walk surfaced deviations the plan did not anticipate:

| Deviation | Status |
|---|---|
| **10 nav destinations, not 8** | `ux.md` §4.2 authorises 8. `Apps` (a pushed route in Flutter) and `Stats` (reachable in neither Flutter nor any parity row) became destinations. Filed **I26**. |
| **No per-destination nav icons** | The frozen row says "same order **and icons**"; the single `insert()` never chains `.icon()`. Filed as **#5 / I30**. |
| **Two progress consoles, not one** | §3.1 asked for one shared `TaskProgress` used by both the modal and the wizard console. The four Flutter dialogs collapsed to one shared row, but the console half still exists twice (`wizard_view.rs:290-303`, `activity.rs:229-238`) with the severity predicate copy-pasted. |
| **No keyboard accelerators anywhere** | Not just Ctrl+R: `grep keyboard_nav|shortcut|set_keyboard_nav` over `app/src` returns nothing. §5.1 item 5 is unimplemented. |
| **The 300px console anti-pattern survived** | §5.4 names it explicitly as a fixed height that clips at large scale; it was carried over unchanged (`wizard_view.rs:301`). |
| **`Images` refresh is the global containers refresh** | The tree special-cases `Apps` for this exact mis-wiring and did not for `Images`. Filed as **#97 / I29**. |
| **A stale in-code justification** | `views.rs:556-559` justifies omitting Start from the details page on the grounds that "NO start op exists in the backend". `Backend::start_container` exists (`core/src/service.rs:253`) and is wired as a CTA on two other pages. The comment is now factually false. |

---

## 7. Limitations

1. **Interactive break-every-flow was not performed.** §4.4. No user flow was driven; the
   running app was observed on one page. This is the report's largest limitation and it
   bounds every confidence claim in §5.
2. **148 of 193 rows rest on `source` tier.** §4.1. Caused by `app/`'s missing lib target,
   which puts a structural ceiling on what any test can reach. Filed **I31**.
3. **No `T3` evidence beyond one Dashboard capture.** Consequences: dialog behaviour, wizard
   progression, destructive-confirm flows, error rendering and the toaster are all
   unobserved in a running app.
4. **The RPM never builds.** §3.2, filed **I19**. The spec and script are reviewed only.
5. **CI publishes no artifact.** `flatpak.yml` triggers on tags but uploads nothing, and the
   tag build is the only thing that would produce a shippable bundle. Filed **I20**.
6. **The Orca a11y acceptance gate has never been run.** §5.3, filed **#190 / I30**.
7. **`show_skipped: true` is pinned only through `from_list`, not through a rendered frame.**
   T16 closed I23 by adding `dashboard_counts_do_not_prettify_the_skip_count` (§9), which
   pins both mutations I23 named by hand — but the app crate cannot be reached from an
   integration test, so the caption as rendered is still unverified.
   T16 closed I23 by adding `dashboard_counts_do_not_prettify_the_skip_count` (§8), which
   pins both mutations I23 named by hand — but the app crate cannot be reached from an
   integration test, so the caption as rendered is still unverified.
8. **Colour is structurally unavailable in this iced rev.** Several rows (`#34`, `#58`,
   `#159`, `#185`, `#186`) are narrower than their Flutter source because coloured
   `Text`/`SelectableText` cannot be constructed — `<Theme as Catalog>::Class: From<StyleFn>`
   is unsatisfied. This is verified against the vendored source rather than assumed, and it
   is why `status_color` is callerless rather than merely unwired.
9. **`ux.md`'s per-row provenance is uneven.** Some §6 subsections cite Flutter by
   `lib/…:NNN`; §6.6 and §6.14/§6.15 cite by bare filename. The walk recovered the missing
   line numbers from `pre-t14-flutter-parity` and recorded them in the appendix, but the
   frozen table itself was not rewritten, per D19's amendment.

---

## 8. Residual risks R1–R13

`PLAN.md` §3 lists these. §5.7's fifth endgame criterion requires each to have a named owner
or an accepted disposition. All thirteen do:

| # | Risk | Owner | Disposition |
|---|---|---|---|
| R1 | Side-by-side container comparison lost | lead | **Accepted.** Single-window + back stack (REVIEW §B.f). |
| R2 | `disk_usage` scope | lead | **Dropped**, mock removed; `block_io` ships as the free disk-adjacent column (D18). |
| R3 | Brand accent `#137FEC` + Inter lost | lead | **Accepted.** Theme roles + system font; brand lives in the icon. |
| R4 | ~600-string i18n schedule load | ux | **Closed.** Extraction happened per-screen during the port (T15); 365 entries, 443 call sites, 0 dead, 0 missing. |
| R5 | Intended-changes ownership | lead | **Accepted.** `PLAN.md` §4 table; the walk added seven further deviations to §6 of this report. |
| R6 | RPM spec drift | pkg | **Open.** `check-versions.sh` gates version agreement (D14), but nothing builds the spec — see R9/I19. |
| R7 | `Message` test bar | arch | **Accepted.** Behavioural tests + exhaustive match, not per-variant (D13). |
| R8 | Submodule drift | lead | **Accepted**, recorded in D20. |
| R9 | Poll-only (no event stream from distrobox) | lead | **Accepted**, recorded in D20. |
| R10 | Registry duality | lead | **Accepted**, recorded in D20. |
| R11 | Tarball history | lead | **Accepted**, recorded in D20. |
| R12 | GSettings import | lead | **Accepted**; the one-time DistroShelf import is implemented (T12). |
| R13 | Orca manual step | lead | **Open, and escalated.** D20 recorded it as a manual step. The walk could not run it (§4.4) and it remains a release-gate item, not a checkbox — see §5.3/#190. |

R6 and R13 are the two that are *not* closed by acceptance. R6 is unverifiable without
`rpmbuild`; R13 needs a human at a real session with a screen reader.

---

## 9. The gate, and a defect T16 found in it

`verify.sh` passes end to end — all 11 stages — from the current tree. That is §5.7's second
criterion.

Finding it was not free, and how it failed is worth recording. The first full run **failed
at stage 10**:

```
smoke (positive): readiness line observed
smoke (positive): FAIL — binary lingers after SIGTERM
```

The obvious reading is that the app does not exit on SIGTERM. **It does.** The defect was in
the gate: `smoke-test.sh` located the binary with
`pgrep -f '^gosh_distrobox_manage[r]' | head -1`, which matches *any* installed-and-running
copy of the app, and `pgrep` emits in ascending PID order — so `head -1` prefers the oldest.
A second instance was alive (the app left open from the walk in §4.3), the SIGTERM went to
**that** one, and the unscoped final check then saw the smoke test's own instance still
running and blamed the app.

Reproduced deterministically: leave an instance running → stage 10 fails; none running →
passes. Fixed by snapshotting the matching PIDs *before* launch and subtracting them, so the
test signals only the process it started; it now also prints
`NOTE — ignoring pre-existing instance(s): <pids>` so a reader can see why a foreign process
was skipped. Filed as **I27**.

The reason this had never been caught is the part worth keeping: the gate had passed for
every prior task, because in a clean CI run nothing else is running. It took a walk that
left the app open — which is exactly what a parity walk does — to expose it. The fix was
verified against the pre-fix script under the same condition, which still fails.

Separately, T16 closed **I23**, the last open coverage hole from T13. Two committed doc
comments (`views.rs:344-347`, `skipped_rows.rs:15`) asserted that `DashboardCounts::from_list`
was unit-tested — the seam introduced to make I23's app-side half testable — and **no test
called it**. `dashboard_counts_do_not_prettify_the_skip_count` now does, and was verified to
fail under both mutations I23 named by hand (`skipped: 0`, `show_skipped: true`) before
being kept. Filed as **I28**.

---

## 10. §5.7 endgame checklist

| Criterion | Status |
|---|---|
| Every one of the 193 parity rows ticked with a verification tier (D19) | **Met.** Appendix A; every row carries a verdict and a tier. Tiers corrected downward for 14 rows after adversarial re-examination. |
| `scripts/verify.sh` passes from a clean checkout (D13) | **Met.** All 11 stages. Stage 10 passed only after I27 was fixed. |
| Phase 3 pass clean, no remaining failures (T16) | **Met with findings.** No crash or data-loss path. Five `bug` rows (I29) and seven `missing` rows (I30) filed with evidence. |
| `docs/migration/REPORT.md` written | **Met.** This document. |
| Residual risks R1–R7 each have a named owner or accepted disposition | **Met.** §8, extended to R1–R13. R6 and R13 are open with owners rather than accepted. |

---

## 11. Issue queue as filed by T16

| # | Summary | State |
|---|---|---|
| I27 | Smoke gate signalled a foreign instance and blamed the app | **Fixed in T16** |
| I28 | Two doc comments asserted a test that did not exist (`from_list`) | **Fixed in T16** |
| I29 | Five parity rows regressed to `bug` (#13, #20, #97, #134, #186) | Open |
| I30 | Seven parity rows are `missing` (#5, #40, #41, #161, #185, #189, #190) | Open |
| I31 | `app/`'s missing lib target is a systemic verification ceiling (148/193 rows at `source`) | Open |

Carried forward from earlier tasks, unchanged by this one: **I19** (RPM never executed),
**I20** (CI publishes no artifact), **I21** (dead screenshots), **I22** (dead distro SVGs),
**I24** (seven callerless core helpers), **I25** (stale `architecture.md` source-line refs),
**I26** (10 nav destinations vs 8).

---

## Appendix A — the 193 parity rows, walked

Every row from `ux.md` §6, with the verdict reached in T16 and the highest verification tier
actually attained. The verdicts here are **post-adversarial-correction**: 14 rows were
re-examined and nine tiers were corrected downward (§4.1). "Basis" is the evidence itself —
a call path, a citation, or the reason for an absence. Rows whose status in `ux.md` was
`dead` are the deliverable named in `PLAN.md` §2; all 15 are accounted for in §5.1.

| # | Item | Verdict | Tier | Basis |
|---|---|---|---|---|
| 1 | 8 nav destinations, fixed order | rescoped | source | Scope grew from 8 to 10: ux.md §1 (line ~76) says "8 destinations: keep all 8, same order and icons". Apps and Stats became nav destinations in the… |
| 2 | Nav rail >=600px, extended >=1000px | dropped | source | Deliberate drop, decided twice: row approach cell ("Accept COSMIC condensed+toggle; drop both breakpoints (§1)") and §1 "Recommendation: accept the… |
| 3 | Bottom nav bar <600px | dropped | source | Dropped: row approach cell "Drop — no COSMIC equivalent"; §1 table "No bottom-nav-bar equivalent exists — COSMIC does not put app nav at the bo… |
| 4 | Nav labels (divergent sets: Home/Boxes/Pkgs/Logs vs Dashboard/Containers/...) | live | source | Divergence is gone — one consistent set drives the (single) nav. But it is neither Flutter set verbatim: Home->Dashboard and Logs->Activity, and th… |
| 5 | Per-destination icons, selected/unselected variants | missing | source | The rail renders text-only for all 10 destinations: no per-destination icon and therefore no selected/unselected variant. `Page::title()` has a sib… |
| 6 | Page switching | live | source | Both directions verified: user selection (NavSelect -> activate) and programmatic (views::activate_page/activate_containers at app/src/app.rs:815,… |
| 7 | Tab state preserved on switch | live | source | Comes free as §1 predicted. Nothing asserts it: no test drives nav, so this rests on reading the state ownership, not on an observation. |
| 8 | Global gates (distrobox-missing, env-blocked) | live | source | The dup is resolved exactly as §3.4 asked (implement once, wrap `view()`); the gate result is still toaster-wrapped (app.rs:1450). §3.4's third gat… |
| 9 | Header title + icon | rescoped | source | Rescoped: the header carries the APP name, not the page name, and the page icon is gone. The per-page name now lives only in the nav rail (`Page::t… |
| 10 | Refresh action | live | source | No `Ctrl+R` accelerator: `grep -rn "keyboard_nav\|shortcut\|set_keyboard_nav" app/src` returns zero hits — §5.1 item 5 (ux.md:317) is unimplemented… |
| 11 | Full-page loading spinner | rescoped | source | Deliberately not ported — §3.4 (ux.md:185): the full-page spinner 'is a regression to avoid; COSMIC convention is to keep content and show progress… |
| 12 | Environment-blocked view (icon, title, message, "Check Again") | live | source | Implemented once at the SHELL level (all 10 pages inherit it) exactly as §3.4/§6.1.8 decided (ux.md:187). 'Check Again' re-runs refresh only — ther… |
| 13 | Distrobox-not-found view (title, copy, "Check Again") | bug | source | Live at the shared shell gate; no callerless-helper risk — `gate` is called at app.rs:1445 from `view()`. Tier is source only because no test drive… |
| 14 | System status card (SYSTEM STATUS, healthy/degraded headline, running-of-total) | live | T1 | Live. Two deltas: (a) Flutter's right-side check_circle/warning icon badge (dashboard_page.dart:302-324) is not ported; (b) `healthy` no longer con… |
| 15 | Inline error strip inside status card | live | source | Live (icon+message strip → `widget::warning`, which carries `.on_close`). Flagged duplication: the shell ALREADY renders the same `self.error` stri… |
| 16 | Stat card: Total Containers | live | source | Live. Matches the approach column (label+value row, no icon/colour). Tier is source, not T1: `DashboardCounts::from_list` is asserted to be unit-te… |
| 17 | Stat card: Running | live | source | Live as a value card. The approach column's 'theme success colour' is NOT rendered: `stat_tile` (views.rs:504-511) applies no colour, and `status_c… |
| 18 | Stat card: Stopped | live | source | Live as a value card; the approach column's 'theme warning colour' is not rendered (see #17/#34). Note the semantic is broader than Flutter's: 'sto… |
| 19 | Active Tasks section (conditional, only when non-empty) | live | source | Live and correctly conditional. Vestigial parameter: `active_task_count` is passed and immediately discarded — `let _ = active_task_count;` (views.… |
| 20 | Task card: spinner, description, "In progress…"/"Completed" | bug | source | Two of the row's four elements are absent: the spinner and the In progress…/Completed status copy. The running state is signalled by the Cancel but… |
| 21 | Task card: cancel button | live | source | Live. Implemented as a LABELLED text button ('Cancel') rather than the approach column's `button::icon("process-stop-symbolic")` + label — better f… |
| 22 | Containers section header | live | source | Live; rendered by `views::view_dashboard` from app.rs:3528. Uses `text::caption_heading` (the §3.6/§3.4 shared heading style) rather than a page-lo… |
| 23 | "View all" → containers tab | live | source | DEAD → LIVE. The empty callback is gone; the button now drives `nav_model.activate_position`. Tier source: no test asserts the nav switch (app/test… |
| 24 | Container row: distro icon, name, status dot, status text, chevron | live | source | Live and shared with the Containers page (one `container_row`, no second copy). The 'status dot' is a ● glyph in DEFAULT text colour — colour-coded… |
| 25 | Container row: inline Stop when running | live | source | Live. Stop is NOT behind a confirm dialog — matching Flutter (which also called `stopContainer` directly), but not the approach column's 'button::i… |
| 26 | Container row: tap → details | live | source | Live via a real `list::button` (focusable → §5.1 item 5 'Enter/Space on focused rows' comes free). The implementation adds a nav switch Flutter's p… |
| 27 | Empty-containers card | live | T1 | Live and now split from the unreadable-list case (B3): a list whose rows all failed to parse shows the warning icon + `dash-all-rows-failed` count… |
| 28 | Quick action: New Container | live | source | DEAD → LIVE. Implemented per §4.2 line 239 ('Push the create wizard') — a blank wizard; preselected images come only from the Images page (#95, app… |
| 29 | Quick action: Upgrade All | live | source | DEAD → LIVE, implemented for real (no redirect): confirm → N upgrade tasks, each reporting through `TaskMsg::Started{ kind: TaskKind::Upgrade }` (a… |
| 30 | Quick action: Stop All + confirm | live | source | Live, and now on the single shared confirm (§3.3, ux.md:160-179) rather than a page-local AlertDialog. Behaviour gain: Flutter DISABLED the button… |
| 31 | Quick action: Refresh | live | source | Live. The approach column's 'same as #10' holds — one message, one handler, two affordances (header button + quick action), no second refresh path. |
| 32 | Pull-to-refresh | dropped | source | Deliberately dropped with a cited decision (§3.5), not silently lost. Replacements: header Refresh (#10, app.rs:1544-1548) and the Refresh quick ac… |
| 33 | Distro icon mapping (8-outcome, 3 variants) | live | T1 | LIVE, dedup complete: §3.6 (ux.md:208) proposed exactly this one table, and the 6-outcome pages (package_manager_page, backups_page) inheriting the… |
| 34 | Status colour/label mapping | rescoped | T1 | Rescoped: the 4-file duplication IS consolidated into one file (the dedup goal of §3.6), and the label half is live and copy-exact; but the COLOUR… |
| 35 | Header "Containers" + refresh | live | source | Live but two-part: the "Containers" title is the nav-bar entry (Page::title), not an element on the page body — view_containers_page (app.rs:3549-3… |
| 36 | Loading / not-installed / error / empty states | live | source | Flutter's dup gate (containers_page reimplemented not-installed/error/empty) is consolidated per ux.md §3.4. Two deliberate deviations from Flutter… |
| 37 | Container card list | live | source | Rows are plain Columns, not card/parchment-wrapped as Flutter's Card(elevation:0) was — matches the ux.md approach column (list_column + list::butt… |
| 38 | Card: distro icon with status dot overlay | rescoped | T1 | Rescoped on two axes: (a) no icon+dot Stack overlay — same information is a ● text glyph; (b) no theme colour. views.rs:248-255 documents why: `sta… |
| 39 | Card: name / status text / image path | rescoped | T1 | Two of three live and T1-pinned; the third (image path in caption style under the status) is a real parity loss on the card — Flutter showed it (co… |
| 40 | Card: inline "Open Terminal" when running | missing | none | The running-only card button genuinely has no port. Guard against a false positive: the details page DOES have an Open Terminal control with the sa… |
| 41 | Card: ⋮ → quick actions | missing | none | Trigger and sheet are both absent; the actions that sheet carried are reported under ids 47-51 with the surface that now carries each. |
| 42 | Card: long-press → quick actions | dropped | source | Sanctioned drop, cited to ux.md §3.2/row 42. The drop is correct; the paired "wire right-click context_menu instead" from the same sentence is unim… |
| 43 | Card: tap → details | live | source | Genuinely wired end to end (row tap -> message -> handler -> view_details). No integration test covers DetailsMsg::OpenRequested (app/tests/dashboa… |
| 44 | FAB "New Container" → wizard | rescoped | source | Same destination (blank create wizard), different affordance: a header suggested button instead of a FAB. Note the FAB's Flutter bug class does not… |
| 45 | Status dot colour/label | rescoped | T1 | The dup is genuinely consolidated to one helper each (Flutter had 2 named helpers + 2 inlined copies across 4 files, per ux.md:206/row 34). Rescope… |
| 46 | Start / restart container | live | T2 | This is the row's headline change: it was missing in Flutter and is now live with the strongest evidence in my range (both tiers). Reachability… |
| 47 | "Details" row | rescoped | source | Rescoped, not missing: the destination is live and one gesture shallower (tap the card instead of open-a-sheet-then-choose-Details). The Flutter ro… |
| 48 | "Open Terminal" row, disabled when stopped | rescoped | source | Rescoped: the semantics (offered, disabled when stopped) survive verbatim, but on the details page quick-action list (which is §6.5 row #63's surfa… |
| 49 | "Stop Container" row (running only) | rescoped | source | Rescoped to an inline card button; the running-only condition is preserved (views.rs:292), so the Flutter behaviour is intact. One deviation from t… |
| 50 | "Upgrade Container" row | rescoped | source | Rescoped to the details page (which is also §6.5 row #60). The button is the row's subject, so the row itself is source-tier; the underlying task-s… |
| 51 | "Delete Container" row → confirm | rescoped | source | Rescoped: the destructive-class row + confirm pattern is intact and now goes through the one shared ConfirmSpec helper (§3.3) rather than a hand-ro… |
| 52 | Delete confirm dialog copy | live | source | The dup is genuinely resolved to the details-page wording, chosen once in the catalogue and reached by every delete path (details Danger Zone views… |
| 53 | Header: name + back | live | source | Back affordance is real and the page is a pushed nested page rather than a nav entry (§1 "Consequences" rec (b), ux.md:74). The container NAME is n… |
| 54 | Header action: Open Terminal (running only) | live | source | Placed as a Quick-Actions tile rather than a header action because the details page occupies the whole `view()`; `header_end()` returns `vec![]` fo… |
| 55 | Header action: Refresh | live | source | Button is live and wired. ux.md:372/393 additionally promise `Ctrl+R` for the Refresh action — there is NO keyboard subscription in the tree (app.r… |
| 56 | Hero card: icon, name, image URL chip | live | source | All three parts are live. Reshaped/plain rather than one `settings::section`: Flutter's 80px tinted icon container and its rounded-card `BorderSide… |
| 57 | Image URL copy → clipboard + snackbar | live | source | Snackbar → toaster per §3.4 (ux.md:772-779 comment). The clipboard write is a `task::effect` and is correctly batched (B1 lesson at app.rs:945-952)… |
| 58 | Status card: status icon, dot, label, container ID | rescoped | T1 | Rescoped: label + ID are live and T1-tested; the status icon glyph and the coloured dot are deliberately dropped (not silently missing) because col… |
| 59 | Status card: Stop button (running only) | live | source | Tier is `source` for the row's own behaviour because the "running only" gate and the button are untested — the T2 test proves the op, not the condi… |
| 60 | Tile: Upgrade Container + progress dialog + inline spinner | rescoped | source | Rescoped exactly as the checklist proposed and §3.1 (ux.md:122-152) mandates: the fifth copy-paste `_UpgradeProgressDialog` is NOT ported; task pro… |
| 61 | Tile: Applications → Apps page | rescoped | source | Rescoped by §1's navigation decision (ux.md:74 rec (b)/(a)): the Apps page is a TOP-LEVEL nav tab here, so the tile switches tab + selects rather t… |
| 62 | Tile: Clone Container → dialog (name `-clone`, home dir, error text, spinner) | rescoped | source | Rescoped on two of the four named parts: (1) the HOME-DIRECTORY field is dropped — `CreateArgs { …, home_path: None, … }` is hard-coded at app.rs:9… |
| 63 | Tile: Open Terminal, disabled when stopped | live | source | Same affordance as row #54 (one tile serves both the "header" and "tile" rows); the disabled state is `.on_press_maybe(None)` rather than Flutter's… |
| 64 | Danger Zone: Delete + confirm | live | source | Live and correctly routed through the one shared confirm modal (§3.3) rather than a fifth hand-rolled `AlertDialog`. Divergences: the red danger-zo… |
| 65 | **Missing: start/restart** | rescoped | T2 | Rescoped, not delivered on this page: `view_details` renders NO Start/Restart button — deliberate and documented at app/src/views.rs:556-559, but t… |
| 66 | Header: name + refresh | rescoped | source | Refresh half is live and functional; the name half moved out of the app header into the page's own info header (row #67), and the header now carrie… |
| 67 | Info header: icon, name, status pill, monospace image | live | source | All four data points present. The Flutter status PILL (bordered + tinted Container, :143-166) is downgraded to a plain caption line — this matches… |
| 68 | Not-running warning banner  [Flutter-labelled dead: "Start the container…" — no start action] | live | T2 | THE DEAD ROW OF THIS SECTION, now genuinely live: the sentence that promised a start is backed by a working Start CTA and a real start_container (D… |
| 69 | `distrobox enter` command display (selectable, monospace) | rescoped | T1 | RESCOPED: the command is displayed monospace and correct, but is no longer text-selectable — the row's item and its approach column both name `widg… |
| 70 | Copy icon button + snackbar | live | source | Clipboard write + toast both wired (the T6 B1 'never drop the effect' lesson). Two deltas, both in the approach column's own words ("button::icon (… |
| 71 | "Copy Command" filled button | live | source | Filled/suggested class matches Flutter's FilledButton. Only delta is it is no longer full-width (Flutter wrapped it in width: double.infinity) — co… |
| 72 | Quick action: Copy Command (running only) | live | source | Affordance and its running-only gate are both live. Layout rescope worth recording: Flutter had a separate 3-button 'Quick Actions' card (:316-390)… |
| 73 | Quick action: Upgrade Packages → snackbar only  [approach: route to shared task dialog instead of a bare snackbar] | live | T2 | The row's upgrade request is honoured: Flutter's redirect-style snackbar is replaced by the shared task flow, exactly what the approach column aske… |
| 74 | Quick action: Stop Container (running only) | live | T2 | Running-only gate and the mutation are both live and T2-covered. Two deltas: (a) the approach column's "+ confirm" was NOT implemented — stop goes… |
| 75 | Details tile: ID / Name / Image / Status | live | source | All four rows live, values come from ContainerInfo. Delta: the row no longer sits inside its own bordered Card and the first label is shortened fro… |
| 76 | Help text: no real terminal yet  [approach: text::caption; fix README's false "terminal emulator" claim] | live | source | Both halves of the row done. The copy was deliberately improved — 'planned for a future release' (a promise) became 'is not included' (honest), and… |
| 77 | Real terminal emulation | dropped | none | Explicitly dropped, not silently ported — ux.md §4.5 and D-decisions both name it. Important scope boundary to record: D8 revived supported_termina… |
| 78 | 3-step indicator (dots, animated) | rescoped | source | Indicator is real and tracks the step, but it is a static text-rune dot row — Flutter's `AnimatedContainer` width tween is gone. The ux.md approach… |
| 79 | Step 0: image grid w/ selection + check mark | live | source | Grid + selection are genuinely wired. Two divergences, neither breaking: the check is a '✓ Selected' button label rather than a Positioned check_ci… |
| 80 | Step 0: live search filter | live | T1 | Unit test asserts empty query returns all, 'UBU' matches 'Ubuntu:latest' (case-insensitive), 'zzz' returns empty — i.e. the filter is live and case… |
| 81 | Step 0: custom image URL field | live | T1 | Test asserts whitespace is trimmed and custom overrides selection ('  b  ' → 'b'), which is the documented precedence. Field is typed input only —… |
| 82 | Step 0: Cancel / Next + "Please select an image" | live | source | The ux.md approach cell explicitly asks for an inline error rather than a snackbar; the tree delivers that, so the divergence is the intended fix,… |
| 83 | Step 1: selected-image preview card | live | source | Preview content is present and reads the same `effective_image()` value as the create path. Flutter's tinted container, distro icon and ellipsis ov… |
| 84 | Step 1: Container Name + auto-default name | live | T1 | Test pins the Flutter-derived '{image}-container' lowercase shape and the `image_display_name` capitalisation. The empty-only fill condition is vis… |
| 85 | Step 1: Init System toggle | live | source | Toggle is wired end-to-end to `w.init_system` and thence to `CreateArgs.init`. Tier is source, not T2: `create_sends_full_argv` constructs `CreateA… |
| 86 | Step 1: NVIDIA toggle | live | source | Same shape as #85 with the same tier caveat: the toggle→state→args chain is unambiguous by reading, but the integration test seeds `nvidia: true` d… |
| 87 | Step 1: Advanced expansion | live | source | Collapsed-by-default behaviour matches Flutter's `initiallyExpanded: false`, and expansion actually gates the advanced contents rather than merely… |
| 88 | Step 1: Home Directory field w/ folder suffix | rescoped | source | The row's dead affordance is eliminated rather than ported: the folder glyph that looked like a native picker and did nothing is gone, so the lie c… |
| 89 | Step 1: Volume mounts list + add/remove | live | source | List, add, remove and the empty state are all wired, and the host/container paths are passed as Fluent placeables rather than concatenated (wizard_… |
| 90 | Add Volume dialog (host, container, read-only) | live | source | The Flutter dialog's silent return on empty input is fixed, which is what the row's approach cell demands ('fix silent return'): the user now gets… |
| 91 | Step 1: Back / Create + name validation | live | source | Buttons are wired. Tier source because the Create arm's own UI surfacing is untested — the T2 test named here exercises `CreateArgName::new` (row #… |
| 92 | Step 2: progress indicator w/ running/success/fail states | live | source | All three states are reachable and distinct: running (indeterminate), success (determinate 1.0 + 'Container Created!'), fail (determinate 1.0 + 'Cr… |
| 93 | Step 2: live console (coloured, no auto-scroll, empty state) | rescoped | source | The three listed traits are individually present, but the ux.md approach cell and §3.1 (docs/migration/ux.md:131-145) specified ONE `TaskProgress`… |
| 94 | Step 2: Cancel / Done / Close | rescoped | source | Two affordances exist where Flutter had three: the else-branch collapses 'Done' (success) and 'Close' (failure) into one label, and the failure pat… |
| 95 | `preselectedImage` arg | live | T1 | The frozen-defect row is resolved: the argument that no caller ever passed now has two callers, each the exact wiring §4.2 prescribed. The T1 test… |
| 96 | Name validation (empty check only) | live | source | The ux.md approach cell asked for exactly two things: surface the backend diagnostic in the UI (done — inline `widget::warning`, not a snackbar) an… |
| 97 | Header "Images" + refresh | bug | source | Live but the refresh half is mis-wired for this page (it is a global containers refresh). This is the same mis-wiring the tree explicitly documents… |
| 98 | Headline + subtitle | live | source | Subtitle copy is the rewritten catalogue wording, which is row #104's rescope landing in this row's string. |
| 99 | Live search filter | live | T1 | Filter logic is the same function row #80 uses in the wizard; the test targets the function, and the images view is its call site. |
| 100 | Loading / error+Retry / empty (none & no-match) | live | T2 | T2 is for the backend half only (the catalogue load these states render); the three UI branches are source-level. Retry is genuinely wired: ImageMs… |
| 101 | Image grid cards (2-col, icon, name, tag, + button) | live | source | The Flutter row describes an icon-only "+" button; the port substitutes two full-width labelled buttons (Select→wizard, Details) inside the card. T… |
| 102 | Image details dialog (Tag, Image URL, Close, Create Container) | live | source | Dead row resolves to LIVE, not dropped: the whole §4.2 "dead ends" decision for this row was "Implement. One call: push the create wizard with pres… |
| 103 | Custom image URL field + arrow button | rescoped | source | Dead row resolves to RESCOPED: the Flutter payload was a snackbar carrying the URL and a no-op arrow; the port carries the URL into the wizard dire… |
| 104 | Page semantics: distro *catalogue*, not local images | live | T2 | The row's parenthetical `list_images()` = `distrobox create --compatibility` is now documented in code at images_view.rs:5 and at app.rs:253-255, a… |
| 105 | Local image management (list / pull / delete) | dropped | source | DELIBERATELY DEFERRED, explicitly not silently dropped: D18 names it, ux.md §4.5 scopes it P1 and names the exact plumbing it needs (`podman images… |
| 106 | Header + refresh (no tooltip on refresh here) | live | source | Same caveat as #97: Refresh reloads containers, which on this page is the right dependency (rows #109/#110/#111 all derive from the container list)… |
| 107 | 2 tabs (Installed / Search w/ dynamic label) | rescoped | source | Behavior and the dynamic label are preserved; the widget is not the one the approach column proposed (`tab_bar::horizontal` + `segmented_control`)… |
| 108 | Loading / not-installed / no-containers gates | rescoped | source | RESCOPED: this row's three gates were per-page and drifting in Flutter; the port implements the env/not-installed pair ONCE at shell level for all… |
| 109 | Container dropdown w/ running dots | rescoped | source | RESCOPED: the dropdown itself is real and wired, but the running-dot indicators are replaced by the (row #110) text pill — `ContainerInfo` exposes… |
| 110 | Container status pill (Running/Stopped) | live | source | Text badge only, no coloured chip — the approach column's `container`+`text` badge. Strings match the row's Running/Stopped exactly. |
| 111 | "Container is not running" warning banner | rescoped | source | RESCOPED, and this is the §4.1/D6 story landing honestly: the copy makes the same promise as Flutter with no button, but the code records WHY (the… |
| 112 | Search bar, Enter-triggered, disabled when stopped | rescoped | source | RESCOPED: the gating behaviour is enforced centrally instead of by widget state. The row's own approach column demands "honour disabled state" — vi… |
| 113 | Clear-search button | live | source | Wired both directions (query + tab state). No test; source-level only. |
| 114 | Detected package-manager badge ("Detecting…") | live | T1 | Strongest tier in this section: the badge function is directly asserted, and B1 (typed manager, never a bare string) is additionally covered by app… |
| 115 | Install-from-search-box button | live | source | The §4.2 empty-guard fix is implemented twice (button state + arm guard), and `.trim()` is used in both — which is also the fix the same §4.2 table… |
| 116 | Upgrade All + confirm | live | source | T2 is for the spawned contract, not the confirm dialog. Confirm path is source-level; note `destructive: false`, so the dialog opens even when the… |
| 117 | Installed tab: not-running / loading / error+Retry / empty / count | live | source | All five states present and in the Flutter order. Retry is a genuine reload, not a toast. |
| 118 | Search tab: idle prompt / loading / empty / count | live | source | Idle-vs-no-match is driven by `searching` (set true at app.rs:2666 on the SearchSubmitted that actually ran). |
| 119 | Package row: icon, name, version chip, description, install/remove w/ tooltip | live | source | Icon: PackageInfo carries name/version/description/installed, so there is no per-package icon and no icon widget is pushed in the row (contrast the… |
| 120 | Install / Remove / Upgrade confirms | live | source | Three specs are genuinely distinct — full copy, not one parameterised dialog. Remove is the only one in the destructive class, so it is the only on… |
| 121 | Task progress dialog | rescoped | source | RESCOPED: the §3.1 unification is real (one `TaskView` + `TaskKind`, no label string-matching), but the page-level "task progress dialog" was dropp… |
| 122 | No failure feedback on `null` taskId | live | T2 | Dead row resolves to LIVE, and it is the best-evidenced dead row in this section: the fix is named in code at the exact arm, covers the banner as w… |
| 123 | Header + refresh + "Upgrade All" | live | source | Both buttons fire real messages; no test pins the header composition (widget observation is out of reach for app/, which has no lib target), hence… |
| 124 | Loading / not-installed / no-containers gates | live | source | Functionally identical to Flutter, but relocated: ux.md's own approach column for this row is 'shared gate', which is what was built. Not tier T2 —… |
| 125 | Summary header (N containers, running/stopped copy) | live | source | The body copy is byte-identical to the Flutter string. The title uses a Fluent plural selector, so 'N Containers Available' pluralises correctly. |
| 126 | RUNNING CONTAINERS section header | live | source | Text and gating match; the Flutter dot leading the heading is not reproduced (colour-only decoration; ux.md §3.5/§6 forbids meaning-by-colour but t… |
| 127 | Running card: distro icon, name, "READY TO UPGRADE"/"UPGRADING…", tag chip, image | rescoped | source | Re-scoped, not live: the card composition is a `widget::Row`/`Column` (ux.md proposed `settings::item` — also not what landed) and the distro-name… |
| 128 | Running card: Upgrade button / inline spinner | live | source | Both halves present and wired to real state. No test drives the card; the underlying op is covered only at the backend boundary (app/tests/task_lif… |
| 129 | STOPPED CONTAINERS section (dimmed 0.7) | live | source | Section header and gating are live; the dimming is re-expressed as a caption text class per §3.5, which the checklist itself asked for. Tier is `so… |
| 130 | Stopped card: "Start container to enable upgrades" | live | T2 | This is one of the 15 Flutter `dead` rows and it is provably live: the copy survives, a Start CTA was added beside it, and the backend op it needs… |
| 131 | Upgrade-all confirm + "No running containers" toast | live | source | Both the confirm dialog and the empty-set toast are on the live path (the header's Upgrade All button at app/src/app.rs:1571-1573 is the only produ… |
| 132 | Task progress dialog | live | source | Deliberately re-scoped from a modal dialog to a shared inline row — the checklist's own `dup` status plus 'shared task component' approach column a… |
| 133 | Header + refresh (has tooltip) + 2 tabs w/ icons | live | source | Three deltas from the row's literal description, none plan-violating: (a) no tooltip - the button carries a visible 'Refresh' label, so a tooltip i… |
| 134 | FAB "New Snapshot" (rendered in all states) [bug row] | bug | source | HALF FIXED, and the row's own fix was incomplete. The FAB itself is correctly removed per ux.md:193 ('Drop. COSMIC has no FAB... that bug disappear… |
| 135 | Loading / not-installed / no-containers gates | live | source | Rendered by the shell, not the page: the page returns before its own body when containers is empty, and views::gate wraps every page. The container… |
| 136 | Container dropdown (no status indicators) | rescoped | source | Implemented MORE than the Flutter row, which is what ux.md asked for: ux.md:543 says 'consider adding status dots for consistency' and ux.md:208 ca… |
| 137 | Snapshots tab: loading / error+Retry | live | source | Both states built on the shared empty_state component (app/src/views.rs). Retry is reachable: the button's message has a live arm that re-issues li… |
| 138 | Snapshots tab: empty + "Create First Snapshot" | live | source | The CTA is wired to the same CreateDialogRequested arm as the header button (#134), so it opens the prefilled dialog rather than dead-ending. Only… |
| 139 | Snapshot row: name, created, size, Restore, Delete | live | source | All four fields plus both actions present and each action's message has a live arm. The two action buttons are at the row level rather than inside… |
| 140 | Create Snapshot dialog (prefilled name, helper text, info box) | rescoped | T1 | Everything ported with two deliberate changes. (1) The name default is deterministic ('<prefix>-<container>', Flutter's shape but honouring the set… |
| 141 | Delete Snapshot confirm + green/red result toasts | live | source | The toasts carry the right copy but NOT the colour: views::push_toast renders widget::Toast::new(text) with no severity (app/src/views.rs:785-787),… |
| 142 | Restore dialog (prefilled new name) | rescoped | source | The row's 'prefilled new name' is satisfied; the PREFILL VALUE is re-scoped. Flutter used a timestamped 'restored-<millis>', which is non-determini… |
| 143 | Export card + dialog (output path, warning box) | live | source | Card, dialog, prefilled path and warning box all present. The Browse button is the portal save chooser (ux.md:550, the row's explicit instruction)… |
| 144 | Import card + dialog (archive path, image name) | live | source | Both fields and both labels present, and this is the single input dialog that combines two fields - exactly the shape Flutter had. The Browse butto… |
| 145 | Clone card (disabled w/o container) + dialog | live | source | Unified exactly as the row and ux.md:552 asked: the card opens the same ActiveDialog::Clone the details page uses (app.rs:886-891), so there is one… |
| 146 | 5x silent no-op validation on empty fields [dead row] | live | source | DEAD ROW RESOLVED - became live. All five `if (text.trim().isEmpty) return;` no-ops are replaced by a visible error line, so the dead affordance is… |
| 147 | 4x silent failure when taskId is null [dead row] | live | source | DEAD ROW RESOLVED - became live. Because every arm returns a task result that the shared Started/Completed handlers toast on failure, a spawn failu… |
| 148 | Clone dialog validation lacks .trim() [bug row] | live | source | BUG ROW FIXED. The fix is applied once, in the shared confirm arm that the backups Clone card now reaches (app.rs:2181-2194 opens ActiveDialog::Clo… |
| 149 | Free-text paths, no file picker [missing row] | live | source | MISSING ROW RESOLVED - became live, and this is the section's headline P0. The portal choosers are real, async and reached from the dialogs, with c… |
| 150 | Task progress dialog [dup row] | rescoped | source | The duplication is gone and no modal progress dialog replaces it - so the row lands as re-scoped rather than live-as-described. The five Flutter co… |
| 151 | Undisposed TextEditingControllers in 5 dialogs [bug row] | live | source | BUG ROW MOOT, as the row itself predicted ('Moot - widgets are stateless in the new model'). Rust has no undisposed-controller failure mode at all:… |
| 152 | Header + refresh + clear-completed | rescoped | source | The clear-completed half is live and reachable; the Refresh half is a deliberate omission (documented in the module header, not silently dropped).… |
| 153 | Search over description + output | live | T1 | Live. The `+ Ctrl+F` in ux.md's approach column is NOT wired — a grep for keyboard/Hotkey in app/src returns nothing; ux.md §4.5 scopes keyboard sh… |
| 154 | Filter chips All / Running / Success / Errors | live | T1 | Live. Implementation deviates from ux.md's proposed `segmented_control`: it is a `button::suggested`/`button::standard` pair with selected state (a… |
| 155 | Stats bar Total / Running / Completed / Failed | live | T1 | Live, and a strict improvement on the row's Flutter source, which recomputed each count with `_getTaskStatus(t) == '...'` string sniffing (dart:199… |
| 156 | Empty states (no activity / no match) | live | source | Live, both branches present. Tier is source only: no test asserts either empty branch renders. |
| 157 | Timeline row: progress ring or icon, description, relative time, status pill, last-line preview | live | source | Live, with two accepted deviations: "progress ring" is a static status icon (no `progress_bar` in this widget), and "status pill" became a plain ca… |
| 158 | Output preview -> full-output sheet (draggable 0.5-0.95) | live | source | Live as the planned port, not a regression: ux.md:570 pre-accepted the loss ("resize range is lost, accept it"), and app.rs:1773-1774 records the s… |
| 159 | Severity colouring in output (error/warning/ok) | rescoped | source | Rescoped honestly: two classes survive (error vs ok) but the Flutter WARNING class (orange, dart:471-472) has no distinct counterpart — warning lin… |
| 160 | Status derived by string-matching output text | live | T1 | Live; this is T11's headline deliverable. Residual caveat worth recording: output-text matching survives in two places, but both are COSMETIC, not… |
| 161 | History is in-memory only (500-line cap, lost on exit) | missing | none | Still missing, and deliberately so rather than silently ported. ux.md:266 classes "Persistent task history" as P1 (proposal: cosmic_config or a sma… |
| 162 | Relative time formatting (`Just now`, `5m ago`, `DateFormat`) | rescoped | T1 | Rescoped in one branch: past 7 days Flutter fell through to `DateFormat('MMM d, yyyy')` (dart:75); the port renders `Nd ago` indefinitely, recorded… |
| 163 | Distrobox version + per-row refresh | live | source | Live. Tier T2 because an integration test exercises the data path the refresh button calls; the button->message->toast wiring itself is source-veri… |
| 164 | Total / running containers, installed yes-no | live | source | Live and unambiguously wired. Tier source: the row assembly has no test, and `running_count` — the value source for the middle row — has no test of… |
| 165 | Refresh All Data + "Data refreshed" toast | live | source | Live. The toast fires on press, not on completion — matching Flutter (dart:146-147), which also toasted after its awaits but before any error check… |
| 166 | Stop All Containers + confirm | live | T2 | Live. Tier T2 for the backend action; the confirm dialog and the destructive class are app-side and source-verified only. Note the Settings button… |
| 167 | Upgrade All Containers | live | T2 | Dead -> live, with no silent port: the Flutter snackbar redirect (dart:172-175) is gone and the button performs the upgrade. Tier T2 for the spawne… |
| 168 | Clear Completed Tasks + toast | live | source | Live. Tier source: the clear arm lives in app.rs, which no integration test can import, and there is no unit test for it. |
| 169 | About: app name, Source Code link, Distrobox docs link | live | source | Live and rescoped as planned: ux.md:586 pre-authorised `widget::about()` replacing the whole hand-rolled card, and that is what exists — the Flutte… |
| 170 | Danger Zone: Delete All + confirm w/ warning box | live | source | Live. One rescope to record: ux.md's approach says "confirm() destructive", and the "warning box" is delivered as the shared modal's body copy with… |
| 171 | No persisted preferences at all | live | T1 | Missing -> live, and the largest single change in my section. Keys persisted: selected_terminal, confirm_destructive_actions (which actually gates… |
| 172 | Header: title + container subtitle + refresh | live | source | Live and wired end to end. Two deltas from the Flutter row, neither removing the capability: (a) the container subtitle is folded into the same tit… |
| 173 | Loading / error+Retry | live | source | Both states render through the shared empty_state (app/src/views.rs:180-200) as §3.4 intends. No test covers either branch; the branches are unambi… |
| 174 | Search over name + exec | live | T1 | Filter semantics are byte-for-byte the Flutter ones (same two fields, same lowercase contains). The T1 test asserts the empty query, a name hit, an… |
| 175 | "Manual Binary Export" button | live | source | Label text and libcosmic widget are exactly what the row's approach column prescribes (button::suggested). No test; source-established. |
| 176 | "Installed in Container" + "N Found" badge | live | source | Live. Rendered as a plain Row rather than the widget::settings::section the approach column suggested — cosmetic, and the count badge is a real plu… |
| 177 | Empty states (no apps / no match) | live | source | Live via the shared empty_state helper (app/src/views.rs:180-200). A third state the Flutter page could not have is added above it: no container se… |
| 178 | App card: icon, name, exec, export toggle, EXPORTED label | live | source | Tier is T2 for the toggle's mutation path (the export/unexport contract is integration-tested with argv assertions). Be explicit about the rest: th… |
| 179 | Export Binary dialog (path field) | live | T2 | Tier T2 covers the backend contract the dialog drives (bare-name resolution + pass-through path). The empty-path LOUD error is the row's approach-c… |
| 180 | disk_usage_page — unreachable, hard-coded numbers, dead buttons (Flutter status: dead) | dropped | source | The 11th of the 15 dead rows to be shown non-silent. Dropped twice over, not merely unported: the page is gone AND its 'Optimize Now' button is sep… |
| 181 | task_page — "Not Implemented" stub, unreachable (Flutter status: dead) | dropped | source | Dropped, with the replacement row (#151-162 Activity logs) present as a real page rather than a stub. This is the cleanest of the 15 dead rows: the… |
| 182 | 4x duplicated progress-dialog classes (+1 divergent console) | rescoped | source | Half done: the 4 progress dialogs collapsed to one mirrored task row (spinner/Cancel/check from the T5 mirror, no string-sniff), but §3.1's "one `T… |
| 183 | 10x hand-rolled confirm dialogs w/ inconsistent copy (of 23 AlertDialog uses) | live | source | Copy divergence is structurally impossible now — one `fl!` string per action, one dialog shape. Two §3.3 details differ from the proposal: the warn… |
| 184 | Distro icon mapping diverges across 9 files (3 variants: 8-outcome x6, 10-outcome x1, 6-outcome x2) | live | T1 | Matches the row's approach ("One `distro_icon()` (§3.6)") and fixes the behaviour bug the row tracked: the 6-outcome copies no longer fall through… |
| 185 | Distro colour mapping duplicated x3; status colour x4 | missing | none | Status half consolidated into one helper and unit-tested; distro-colour half of §3.6 was never written. Caveat the row should record: `status_color… |
| 186 | Hard-coded Colors.green/orange/blue/red palette | bug | T1 | No hard-coded palette survives anywhere in app/; theme roles are the only colour code and the test pins them to `theme::active()`, so light/dark/hi… |
| 187 | Snackbar policy inconsistent (3 / 0 / 4 / silent) | live | source | Applied at the shell rather than per page, so a page cannot be silent by omission — the silent-return class is gone by construction. No test assert… |
| 188 | 500 ms isTaskRunning polling (2 pages) | live | source | Both the polling and the string-sniffing are gone (`Row #160`: TaskState derived only from mirror fields, app/src/activity.rs:34-46). The 30 s tick… |
| 189 | Keyboard shortcuts | missing | none | Not started. The libcosmic half is verified present at the pinned rev (src/keyboard_nav.rs:20 `subscription()` binding Tab/Shift+Tab/Escape/F11/Ctr… |
| 190 | a11y semantics / AT support | missing | none | Framework half present, app half absent: §5.3's priority-1 item (labels on icon-only controls) is untouched, as is the Orca acceptance gate at §5.3… |
| 191 | Text scaling | rescoped | source | Rescoped in effect: scaling is inherited from COSMIC exactly as prescribed (no slider, theme text styles elsewhere: title3/body/caption), but the o… |
| 192 | i18n | live | T1 | Done as §5.2 prescribed (Fluent + `fl!` + `i18n.toml`), with the extraction performed page-by-page, and the loader is deliberately crate-local rath… |
| 193 | Theme toggle | dropped | source | Explicitly dropped at ux.md §4.6 line 286 ("### 4.6 Theme toggle — drop, with justification"), restated in the frozen row itself (line 636: "Drop… |