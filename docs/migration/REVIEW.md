# Phase 1 devil's-advocate review

**Reviewer:** devil's advocate (independent pass)
**Date:** 2026-09-11
**Branch:** `cosmic-migration`
**Scope:** `docs/migration/ux.md` (660 lines), `docs/migration/architecture.md` (1280
lines), `docs/migration/packaging.md` (973 lines), against `docs/migration/DECISIONS.md`
and the source tree.

**Phase 1 constraint honoured: no source file was modified by this review. This file is
the only artifact created.**

Tie-break priority applied throughout (per `DECISIONS.md`):
parity with the shipped Flutter app → Flatpak sandbox correctness → accessibility →
simplicity/maintainability → COSMIC conventions.

---

## 0. Verdict, in one screen

**GO for Phase 2 — with 7 blocking pre-conditions (§G.1) and 4 mandated corrections
(§G.2).** No plan is rejected; no section is rejected outright.

| Doc | Verdict |
|---|---|
| `ux.md` | **Approve with changes** — §1 (responsive/breakpoints) needs one correction; §4.1 and §4.4 need sequencing changes; §6 checklist granularity endorsed (§D) |
| `architecture.md` | **Approve with changes** — §0.2 is factually wrong about its own hazard mechanism; §1.3 feature list contradicts `packaging.md` §1.3; §6.1 is missing 599 lines of files it claims to cover; Q6 resolved by this review |
| `packaging.md` | **Approve with changes** — §1.4 is missing the a11y bus grant (P0 impact); §1.3 misses two unpinned git dependencies; §3's "configure rfd" has no hook; one correction (C2) is right but incomplete |

**The single most important structural finding:** a **sibling project in the same
organisation has already completed this exact migration exercise** —
`/home/gosh/Documents/GitHub/Gosh-Yubico-Authenticator-for-Linux` — with a Phase-1 review +
`PLAN.md` + measured decisions (D26/D27/D27a/D27b/D31/D32), a reusable
`flatpak/generate-cargo-sources.py` (`--check` + `git-manifests/` sidecars), and a working
libcosmic Flatpak manifest. **None of the three documents mentions it.** It answers
`packaging.md` Q5 and Q12 and `architecture.md` Q9 *on the record, from measurement*, and
it supplies the tooling that `packaging.md` §1.3 and §4.2 describe building from scratch.
Adopting it converts the highest-risk unknowns in this plan into solved problems.

---

## A. Verification of the stated corrections

Each correction was checked against source. **All eight are substantively correct. Two are
materially incomplete and one is correct for the wrong reason.** No correction is false.

### UX corrections

**A1 — "`nav_model`/`on_nav_select`, not the `nav_bar` widget" — CORRECT.**
`libcosmic/src/app/mod.rs:382` (`fn nav_model`), `:417` (`fn on_nav_select`), `:442`
(`fn nav_bar`, whose default builds `widget::nav_bar` with `max_width(280)`). Both readings
are true of libcosmic, but the *trait* is the hook the app implements, so the correction is
the right one for a plan. Verified.

**A2 — "`ContainerStats` has no disk field" — CORRECT, but the doc omits what it does
have.** `rust/src/backends/distrobox/distrobox.rs:237-244`:
`cpu_percent: f64`, `memory_usage: String`, `memory_limit: String`, `memory_percent: f64`,
`network_io: String`, `block_io: String`. There is no disk field — true. But `block_io` **is**
a labelled block-device I/O figure, and it is the closest thing to disk data that already
exists. Any "Disk" column in the port can ship **today** from `block_io` without new
backend work. This matters for UX Q6 (see §C) and it is a free parity win the doc leaves on
the table.

**A3 — "`list_images` is a distro catalogue, not local images" — CORRECT.**
`distrobox.rs:1069-1082` builds `distrobox create --compatibility` (plus `--list-images` in
one branch) — it enumerates the distro images distrobox *can* create from, not images
present locally. Verified verbatim.

**A4 — "`fakers/` is the `CommandRunner` layer, not mock data" — CORRECT.**
`rust/src/fakers/` holds the `CommandRunner` indirection; `NullCommandRunner` is the test
double. `rust/src/backends/flatpak.rs` carries `map_flatpak_spawn_host` plus its 4 tests.
This is the Flatpak escape hatch, not test fixtures. Verified.

**A5 — "start-container does not exist anywhere" — CORRECT.**
No start/launch entry point exists in `rust/src/api.rs` (exactly **35** `pub fn`s, verified
by count) nor in `lib/`. The three dead-end banners are real:
`lib/screens/container_terminal_page.dart:237`, `lib/screens/updates_page.dart:470`, and the
third in the container card quick-actions. Verified.

**A6 — the recon brief's "8 files contain `_getDistroIcon`" — the doc corrected the count
to 8; the real number is 9.** `_getDistroIcon` appears in **9** files. Downstream, the
status-helper claim is also imprecise: ux.md §3.6 says 4 copies, but there are 2 *named*
helpers and the logic is duplicated in 4 files under different names. The doc's own
"8-file icon/colour copy" heading (§3.6) therefore undercounts by one file. Cosmetic, but it
is the kind of number that ends up in a task estimate.

### Packaging corrections

**A7 — C1, "`--filesystem=home` is unneeded" — CORRECT.**
The only `std::fs::write`/`read_to_string` in the sandboxed process are
`rust/src/backends/supported_terminals.rs:255` and `:270`, both for
`dirs::data_dir().join("distroshelf-terminals.json")` (`:112-115`). The desktop-file
installer script is `include_str!`-embedded and executed **on the host** through the runner,
so it is not a sandboxed file access. No sandboxed code opens `~/.local/share/applications`.
C1 holds.

**A8 — C2, "iced is a git submodule" — CORRECT in substance, INCOMPLETE in the two places it
matters.**

- The framing "`disable-submodules` defaults to false" is the wrong model: `disable-submodules` /
  `--no-recurse-submodules` are **hardcoded** in flatpak-builder's git source handling — there
  is no manifest-level toggle to get wrong. The practical consequence the doc draws (submodules
  *are* checked out, so the plan is safe) is right; the mechanism stated is not.
- **`packaging.md` §1.3 misses two further unpinned git dependencies.** Beyond `iced`
  (via `pop-os/libcosmic`), libcosmic's Linux target block also pulls:
  - `cosmic-settings-daemon` → `pop-os/dbus-settings-bindings`
  - `cosmic-freedesktop-icons` → `freedesktop-icons`

  Both are **inside the Linux target block**, i.e. always compiled, never feature-gated away.
  The doc's reproducibility argument (Q3: "our reproducibility depends on a SHA recorded in
  *pop-os/libcosmic's* tree") is therefore understated by a factor of three — there are three
  such pointers, not one. The mitigation the doc proposes (record the expected SHA as
  documentation) must cover all three, and the sidecar mechanism in §C/PKG-12 covers them by
  construction.

---

## B. Cross-doc consistency

| # | Topic | Status | Finding |
|---|---|---|---|
| a | libcosmic pin rev + features | **CONTRADICTION** | Rev `a401af8b1c54a8abd393b8c5b7c8809402f83850` is consistent in all three documents — good. **Features are not.** `architecture.md:238` declares `features = ["tokio", "wayland", "x11"]` with **defaults on**, while `packaging.md §1.3` requires `default-features = false` plus an explicit list. These produce **different dependency graphs**, which means the flag list that is validated by the offline Flatpak build (§5.3) is not the one the architecture task compiles against. **Must be resolved to one explicit `default-features = false` + list, written once and referenced by both docs.** |
| b | Config story | **CONSISTENT in outcome, UNSEQUENCED** | All three agree on `cosmic-config`. But `distroshelf-terminals.json` is unresolved: `architecture.md §5.3` persists `custom_terminals` through cosmic-config and `packaging.md §1.4.2` keeps a `--filesystem` grant for the JSON file. If the JSON file survives, the grant and its silent-failure mode survive with it. **Answer: kill the file, keep the grant out (see PKG-6/ARCH-Q5).** |
| c | start-container priority | **CONSISTENT** | `ux.md §4.1` calls it P1-blocking; `architecture.md §7` does not sequence it early enough to satisfy that. Both call it blocking; neither puts it in the first build task. See UX-5. |
| d | Orphan pages | **CONSISTENT** | `task_page` dropped by both; `disk_usage_page` tied to new backend work by both. No conflict. |
| e | File picker (rfd) under sandbox | **CONTRADICTION with reality** | `ux.md §4.3` and `architecture.md` say `rfd`; `packaging.md §3` says "configure rfd to use its portal backend". **There is no such configuration hook.** libcosmic declares its own `rfd` dependency as `default-features = false, features = ["xdg-portal"]`, and libcosmic's `src/dialog/file_chooser/` offers `xdg_portal` (ashpd) and `rfd` as mutually-selectable backends, with `xdg-portal` the canonical Linux choice. The sibling project's manifest uses `"xdg-portal"`. See UX-15. |
| f | Single vs multi-window | **CONSISTENT, with an a11y gap** | All three land on single-window + in-app page stack; `multi-window` sits in default features unused. The gap nobody recorded: today's pushed routes can be dismissed with a system back gesture / Escape, and `widget::nav_bar`-based navigation offers no equivalent. Nested pages **must** carry an explicit back affordance (`.on_press`) or keyboard users lose the exit. `keyboard_nav::Action::Escape` exists in `libcosmic/src/core.rs` but is not a substitute for an in-view back control. |

---

## C. Consolidated ANSWERED list — all 45 open questions

Legend: **[A]** answered with evidence · **[R]** residual risk (belongs in `PLAN.md`).
Counts: **38 answered · 7 residual**.

### UX open questions (19)

**UX-1. Drop the 1000px auto-expanded rail? — [A] Yes; the question is stated backwards.**
`lib/screens/home_screen.dart` sets `NavigationRailLabelType.none` at `>= 1000px`, i.e. the
Flutter app **hides** labels on wide windows — it does not guarantee them. There is no
"labels always visible on desktop" parity to preserve, so the premise of the question is
false and the proposed cost (overriding `Application::nav_bar()` and reimplementing the
chrome) is a cost with no benefit. Accept COSMIC condensed + user toggle.
*Evidence:* `home_screen.dart` breakpoints; `libcosmic/src/app/mod.rs:442`;
`libcosmic/src/core.rs` (`is_condensed`).

**UX-2. Does anyone want the bottom nav bar? — [A] No; drop it outright.**
It renders only `< 600px`. The app ships as an RPM/Flatpak desktop application
(`gosh-distrobox-manager.spec`, metainfo) with no mobile target after the Flutter tree is
removed, and `ux.md §4.5` already classifies the mobile-shaped surfaces as out of scope.
Remove it rather than porting or replacing it, and set a window minimum width so the
condensed state is the floor.

**UX-3. Nested pages vs multi-window (losing side-by-side comparison) — [R].**
*Product decision.* Technical answer: single window + back stack is correct for the port
(`multi-window` would multiply the `Message` enum work that ARCH-Q3 is already large, and the
Flutter app is single-window). The regression is real but unmeasured. **Residual because it
needs a product call, not more evidence.**

**UX-4. Rename the "Images" page? — [A] Keep the label; fix the page's own copy.**
Tie-break puts parity first: nav labels are user muscle memory and no capability changes
either way. The dishonesty A3 identifies is in the page's *content*, not its nav title.
Add a clarifying subtitle ("images available to create from") inside the page. Low stakes;
if it is ever renamed, that is a deliberate product change, not a migration side effect.

**UX-5. Does start-container block the whole migration, or just three pages? — [A] It blocks
release; it must not block page work, but it must precede the three pages' copy freeze.**
Put `start` in the backend task group (it is one more `ContainerRuntime` entry alongside the
seven inline fallbacks). Gate only the three banners. Shipping the banners' promise without
the capability is the worse evil because `ux.md §4.2` classifies those rows as `dead` —
affordances that look functional and are not — which is the exact defect class this migration
exists to remove. **Correct sequencing:** add `start` to `architecture.md §7`'s backend rows,
then un-gate the three banners.

**UX-6. Is `disk_usage` worth new backend work? — [R].**
*Scope decision.* Evidence is decisive on the Flutter side: the page is unreachable (not
imported by `home_screen.dart`, which imports exactly 8 pages) and every number is hardcoded
(`'45.2 GB Total'`, `'30 GB Images'`, `'15.2 GB Data'`, `'Containers (8)'`, `value: 0.66`).
Recommendation: **drop the page**, and if any disk figure is wanted, ship UX-A2's `block_io`
column (free, already in `ContainerStats`). Residual because funding `podman system df` +
per-container sizing is a product spend, not a review call.

**UX-7. Are the dead quick actions a UI bug or an unfinished feature? — [A] Fix all of them;
the "why was it never done" question is moot post-port.**
The Flutter UI is a MAUI/Navigator app whose pages access state through
`AppStateProvider`/`InheritedWidget`; pushing a route, opening a dialog, or reaching a
provider from a card in a different subtree is genuinely awkward, which is why several of
these are one-liners that stayed undone. In libcosmic the equivalent is a `Message` dispatched
directly from the widget — the structural reason disappears with the rewrite. Fix all
(`#23`, `#28`, `#29`, `#88`, `#95`, `#102`, `#103`, `#167`).

**UX-8. `widget::about()` replacing the About card — [A] Adopt it.**
Tie-break: a11y + i18n outrank cosmetic parity. `libcosmic/src/widget/about.rs` (feature
`about`) supplies the standard COSMIC layout, **localised strings**, and the
developers/translators/license sections — which is also the mechanism that gives translators
credit once i18n lands. Hand-rolling the three-row card re-introduces untranslated strings.
Record as an intended change (UX-18).

**UX-9. `ContextDrawer` as a replacement for the resizable bottom sheet — [A] Accept the
drawer.**
`Application::context_drawer()` is a fixed-width panel; the Flutter sheet's 0.5–0.95 drag is
not reproducible and is not worth a custom widget. The app already has a full **Tasks** page,
which is the correct destination for a user who needs to read a lot of output. Set the
drawer's width sensibly and add "open in Tasks" as the escalation path.

**UX-10. Fix the 5-branch/8-branch distro-icon divergence? — [A] Fix it; it is a defect, not
a behaviour change.**
A9 counted `_getDistroIcon` in **9** files with 2 named helpers and the logic duplicated in 4
files under different names. Divergent icon sets for the same distro across pages is a bug by
definition; a single shared helper is the fix. It does not need product sign-off — it is
already covered by the "intended changes" table (UX-18). Note the corrected file count when
estimating.

**UX-11. Trust a real `TaskState` enum to replace string-matching? — [A] Yes, and it is free.**
`frb_generated.rs` and the FRB codegen are deleted by `DECISIONS.md` D3 / `architecture.md`
§1.3, which means `models/task.rs` and the Dart-side task model are rewritten regardless. The
change therefore costs nothing extra. It also fixes two confirmed defects: the Dart-side
`lower.contains('error') || lower.contains('failed')` sniffing
(`lib/providers/app_state.dart:29`) and the "push each 1024-byte read as one line" behaviour
in `stream_reader_to_task_output` (`rust/src/api.rs`). Do it as part of the strip, not as a
separate plan.
*Evidence:* `app_state.dart:17` (`_maxOutputLines = 500`) duplicating `MAX_TASK_OUTPUT_LINES =
500` in `api.rs`; `COMPLETED_TASK_TTL = 600s`; `finish_task` retain semantics;
`cancel_task` (`api.rs:429`) `task.handle.abort()`.

**UX-12. Does replacing `Colors.*` with `cosmic_theme` roles change the app's identity? — [R].**
*Product/brand decision.* Technical answer: adopt theme roles. COSMIC users expect the system
accent, contrast and reduced-transparency preferences to be honoured; hardcoding
`#137FEC` defeats the `style()` hook and the user's accessibility settings. **Also flag
Inter:** libcosmic ships the COSMIC system font; keeping Inter means bundling and licensing a
font and losing the user's font-size/text-scaling preference — a `ux.md §5.4` regression.
Recommendation: theme roles + system font for UI chrome; keep the brand in the app's own
icon/logo, which is where brand actually lives. Residual because losing the accent is a brand
call the team must make explicitly.

**UX-13. Persisted preferences: does anything need them? — [A] Yes, and cosmic-config already
carries the requirement.**
The premise "nothing is persisted at all" is wrong: the app persists
`distroshelf-terminals.json` via `supported_terminals.rs:255/270`, and the legacy gschema
carried `selected-terminal`, `window-width`, `window-height`, `distrobox-executable`. The
migration **must** have a config layer to avoid regressing those. Persisted nav destination
and last-used image are genuinely new and are P1 nice-to-haves, not parity.
*Evidence:* `rust/data/io.github.gosh_distrobox_manager.gschema.xml` (4 keys);
`supported_terminals.rs:255,270`; `architecture.md §5.3`.

**UX-14. Is extracting ~600 strings realistic, or will it double the port's time? — [R].**
*Capacity/owner decision.* Process answer: extract **per screen, as each screen is ported** —
never as a separate pass. `ux.md §5.2`'s own plan already implies this, and the sibling
project's `i18n.toml` + `fl!`/`flc!` setup demonstrates the mechanics. Doing it per-screen
adds a small constant factor; doing it afterwards means re-touching every screen, which is
the expensive path. Residual because the *number* ~600 is unverified and the absorption is a
scheduling call.

**UX-15. Does `rfd` work in the Flatpak build? — [A] Answered, and it changes the plan:
use `xdg-portal` (ashpd), not `rfd`.**
- libcosmic's `src/dialog/file_chooser/` implements `xdg_portal` (ashpd) and `rfd` as two
  mutually-selectable backends.
- libcosmic's own `rfd` dependency is declared `default-features = false, features =
  ["xdg-portal"]` — i.e. even the `rfd` path is already routed through the portal. There is
  **no "configure rfd to use its portal backend" step**, so `packaging.md §3` describes a
  configuration that does not exist.
- The portal is the session-bus service `org.freedesktop.portal.Desktop`, reachable from a
  Flatpak sandbox without an added `finish-args` grant. The P0 file picker is therefore P0
  for **both** native and Flatpak installs — the doc's worry in UX-15 is resolved in the
  affirmative.
- **Parity loss to record:** the portal backend exposes no `directory()` / `file_name()`
  setters, so `default_export_dir` and prefilled filenames (the `#88` "folder-icon file
  picker" row) are **undeliverable** through the portal. That row must be re-scoped, not
  ported.
*Evidence:* `libcosmic/src/dialog/file_chooser/`; sibling manifest
`com.goshapps.YubicoAuthenticator.yml` (`"xdg-portal"`).

**UX-16. Has anyone verified Orca against an `a11y`-feature libcosmic app on this distro? —
[A] No, and the sibling project measured the blocker — which is a missing manifest grant,
not an unproven feature.**
The sibling project measured a11y to be **inert under Flatpak without
`--talk-name=org.a11y.Bus`** (justified against `accesskit_unix/src/context.rs:163-183`) and
added the grant. `packaging.md §1.4` does not have it while `ux.md §5.3` rates a11y P0 — so
as written, the plan ships an inert a11y path on its only shipping channel. **Add
`--talk-name=org.a11y.Bus` to §1.4.1**, keep a11y P0, and add "verify with Orca on the live
session" to `packaging.md §5.4`'s cannot-verify list as a manual release step. The feature is
proven; only the local verification is missing.

**UX-17. Which strings must NOT be localised? — [A] Rule: user data never enters a
translatable string by concatenation; it becomes a Fluent placeable.**
Verbatim and never translated: container names, image references/URLs, command output, and
the `distrobox enter` command line. The Flutter code violates this in exactly the places
A-numbers identify — `'Install "$packageName" in "…"?'`, `'Delete "$name"?'`, `'Upgrading
${containerName}'`. Port form: `fl!("install-package", package = package_name, container =
name)` with `{ $package }` / `{ $container }` placeables. **Decide the rule before
extraction** — this is the cheapest i18n mistake to prevent and the most expensive to
retrofit (it changes the message catalogue, so translations churn).

**UX-18. Who owns the "intended changes" list? — [R].**
*Process/ownership decision.* Recommendation: a single **"Intended changes vs Flutter"**
table in `PLAN.md`, one row per change, each with a sign-off column. The changes to enumerate:
distro icons unified across 2 pages (UX-10), the 8 dead quick actions now live (UX-7), real
failure feedback replacing silent no-ops (`#122`, `#147`), disabled buttons instead of silent
no-ops, `widget::about()` (UX-8), theme roles replacing `Colors.*` (UX-12), `disk_usage`
dropped or reduced to `block_io` (UX-6/A2), the file-picker `directory()` loss (UX-15), and
`distrobox_source` dropped (ARCH-Q8). Residual because it needs a named owner, not an answer.

**UX-19. 193 rows or the collapsed ~110? — [A] 193. See §D.**

### Architecture open questions (14)

**ARCH-Q1. Executor and the `tokio::spawn` contract — [A] Keep `cosmic::executor::Default` and
require `cosmic::task::future`. The stated mechanism is wrong; the conclusion is right.**
`architecture.md §0.2` claims `update()` does not run inside a tokio runtime and that a
`tokio::spawn` from `update()` panics with "no reactor running". **It does not.**
`iced/winit/src/lib.rs:2030`: `let task = runtime.enter(|| program.update(message));` — and
likewise `:2054` for `program.subscription()`, with `:125`/`:149` covering instance creation.
**iced already enters the runtime around `update()`.** The hazard is real only for code
running *outside* that window — a bare `std::thread`, or a `smol`-based test body — which is
why the rule is still the right rule. Correct the rationale, and drop the claim that "a
compiler cannot catch" it: the compiler catches nothing, but the runtime does not punish it
either. Consequences: the doc's rejection of `cosmic::executor::multi::Executor` stands (it
does not help `update()`, which is already inside the runtime), and the `std::thread::spawn`
alternative is rejected because it would break the `Task`/action plumbing that gives
cancellation. Do **not** expose `spawn_task` in both forms — one form, one contract.
*Evidence:* `iced/winit/src/lib.rs:125,149,2030,2054`; `libcosmic/src/executor/single.rs`
(multi-thread runtime, `worker_threads(1)`); `iced/futures/src/subscription.rs:182-205`.

**ARCH-Q2. Per-task `Subscription::run_with` vs one aggregate subscription — [A] Per-task.**
`run_with<D, S>(data: D, builder: fn(&D) -> S)` with `D: Hash` — `TaskId` is the natural `D`,
and the `OnceLock<Arc<Backend>>` satisfies the fn-pointer builder, mirroring libcosmic's own
precedent. The aggregate alternative loses per-task cancellation granularity, which
`cancel_task` (`api.rs:429`, `task.handle.abort()`) and `COMPLETED_TASK_TTL = 600s` depend on.
Head-of-line blocking across 8 concurrent streams is the aggregate design's real cost. Note
`Runner.data = (data, builder)`, so a `const` selector with interior mutability is a viable
second design; the `OnceLock` is preferred as the documented precedent.
*Residual note:* the global is still a global — record it as accepted in `PLAN.md`.

**ARCH-Q3. `Message` granularity — [A] Nested sub-enums.**
iced has no enum convention to violate; libcosmic's own example is flat only because it is
trivial. The nested form keeps `update()`'s top-level match readable and lets a page own its
variants, at the cost of threading every page through the top level. With ~70 variants across
12 pages the flat form produces one unnavigable match. Accept the threading.

**ARCH-Q4. Where does authoritative task state live? — [A] Core owns the registry; the app
holds a `TaskView`.**
This preserves the surviving semantics from the current code — TTL
(`COMPLETED_TASK_TTL = 600s`), the replay buffer, `finish_task`'s `None => true` retain rule,
and `cancel_task`'s abort all live in core today (`api.rs`), and `STATE: LazyLock<AppState>`
is the existing ownership model. Moving it to the app would give up `is_task_running` and
`cancel_task` for a view the UI dropped, and would make TTL a UI concern. The
`TaskMsg::Expired` round-trip is the price; accept it.
*Residual note:* dual bookkeeping is real; record the invariant list in `PLAN.md`.

**ARCH-Q5. `cosmic-config` vs `dirs` + `serde` — [A] `cosmic-config`, and the deciding evidence
is not convenience — it deletes a manifest permission.**
The `dirs` + `serde` option's only real cost is that it leaves
`distroshelf-terminals.json` in `dirs::data_dir()`, which is precisely the fragile grant and
"highest-probability silent failure in the whole design" that `packaging.md §5.2` warns
about. Folding terminals into cosmic-config removes a `finish-args` entry, a file-path
mismatch failure mode, and a second source of truth. That is the strongest single
simplification in the plan, decided on sandbox-correctness, which outranks simplicity.
*Evidence:* `supported_terminals.rs:112-115,255,270`; `packaging.md §1.4.2, §5.2`;
`packaging.md` Q5/Q12 and ARCH-Q7 all collapse into this one answer.

**ARCH-Q6. `cosmic_config_derive` key naming — [A] RESOLVED by this review: keys are the Rust
field names verbatim, snake_case, no renaming.**
`cosmic-config-derive/src/lib.rs` emits
`ConfigSet::set(&tx, stringify!(#field_name), &self.#field_name)?` and the matching
`ConfigGet::get::<#field_type>(config, stringify!(#field_name))`. `stringify!` takes the
identifier as written, so a field `custom_terminals` is the key `custom_terminals`.
**`architecture.md §5.3`'s key table is correct as written.** The doc's own
"must be verified before implementing row 7" gate is now satisfied — this is the one open
question the doc flagged as unresolved that is fully closed.
*Residual note:* the legacy gschema keys are **kebab-case** (`selected-terminal`,
`window-width`, `window-height`, `distrobox-executable`) while cosmic-config keys are
snake_case, so the migration mapping is a rename, not a copy. Put that in the table.

**ARCH-Q7. `selected_terminal` migration semantics — [A] Match on `program`; on multiple
matches prefer the exact `full_command_id()`, else first-and-log. Do not prompt.**
`full_command_id()` = `program + extra_args` (`supported_terminals.rs:27`). Legacy value is a
bare program name. Resolution order: (1) exact `program + extra_args` match — impossible for a
bare program name, so this is the *forward* case for a re-migration; (2) the unique entry
whose `program` matches; (3) first match, logged. No UI prompt: the Flutter app never
prompted, and the tie-break puts parity first. The crate's own doc-comment about multiple
Flatpak terminals sharing `program == "flatpak"` is the case where (3) fires; log it so it is
diagnosable, and note that with cosmic-config the ambiguity exists only on first import.
*Evidence:* `supported_terminals.rs:27`; `ux.md`/`architecture.md §5.3`.

**ARCH-Q8. What does `distrobox_source = "bundled"` mean? — [A] The semantics are now known
from released upstream code; **drop the key** for this fork.**
At `v1.4.4` (upstream DistroShelf GTK4), the key is *implemented*: 
`src/dialogs/preferences_dialog.rs:148-163` reads `gio::Settings::new("com.ranfdev.DistroShelf")`
and flips `distrobox-executable` between `"host"` and `"bundled"`, and
`src/widgets/welcome_view.rs:201-202,369-370` does the same. Upstream ships distrobox *inside*
its Flatpak (`com.ranfdev.DistroShelf.json`), so `"bundled"` means "the distrobox we ship".
**This fork does not bundle distrobox** — `gosh-distrobox-manager.spec` has
`Requires: distrobox`, and `packaging.md` bundles nothing. So `"bundled"` is unimplementable
here. Drop the key, record it in the intended-changes table (a setting that no shipped build
of this fork ever read disappears — no user-visible loss).
*Evidence:* `v1.4.4:src/dialogs/preferences_dialog.rs:148-163`;
`v1.4.4:src/widgets/welcome_view.rs:201-202,369-370`; `.spec:13`.

**ARCH-Q9. Enable libcosmic's `single-instance` feature? — [A] No, defer — and note the
contradiction it creates.**
`packaging.md §1.5` **deletes** `rust/data/io.github.gosh_distrobox_manager.service.in` and
the D-Bus service entry, while `single-instance` requires exactly that service for
`dbus_activation`. Enabling it means restoring the file the plan removes. Defer to Phase 2+.
The harm from two instances is bounded: cosmic-config writes atomically per key file
(last-writer-wins, no corruption), and two task registries are wasteful, not dangerous. If
adopted later, restore the service file and add the grant. See also PKG-Q5's resolution,
which *removes* the daemon dependency — consistent with deferring this.
*Evidence:* `rust/data/io.github.gosh_distrobox_manager.service.in` exists at HEAD;
`packaging.md §1.5`; `architecture.md §5.4`.

**ARCH-Q10. Mechanical enforcement of the CommandRunner rule — [A] Yes; enforce it with
clippy.**
`clippy.toml` → `disallowed-methods = ["std::process::Command::new"]`, scoped to `core/`, with
an `#[allow]` at the sanctioned indirection (`map_flatpak_spawn_host` /
`checked_run_command`). The rule has **already been violated once**: `checked_run_command`
existed in the Flutter-era Rust and had to be re-established in upstream
`rust/src/backends/host_exec.rs`, which makes B6 a cherry-pick rather than new work. A rule
with a documented violation and no enforcement is a rule that will be violated again, and the
failure mode — every Flatpak user silently broken — is the worst in the plan.
*Evidence:* `AGENTS.md` ("NEVER use `std::process::Command` directly in backend logic");
`rust/src/backends/flatpak.rs` (`map_flatpak_spawn_host` + 4 tests).

**ARCH-Q11. Extend `ContainerRuntime` or add one helper? — [A] Add
`run_runtime_cmd(&self, args: &[&str])`; and change `#[async_trait(?Send)]` to
`#[async_trait]` unconditionally.**
The trait-extension version grows the surface for no parity gain; the podman/docker
divergences it would document (`podman events` has no docker equivalent) belong to ARCH-Q12,
which is deferred. The `?Send` point is the load-bearing one and the doc has it right:
`container_runtime.rs:13` is `#[async_trait(?Send)]`, and a `?Send` trait object cannot be
shared across the multi-threaded executor or moved into a `OnceLock<Arc<Backend>>` used by a
`run_with` builder. Since `CommandRunner` is already `Arc`-shared and `Send + Sync`, and
`tokio::process::Child` is `Send`, the relaxation buys nothing here — it is an artefact of the
FRB/smol era. Make it `Send` unconditionally, before the trait is used from spawned work, not
after.
*Evidence:* `container_runtime.rs:13`; `architecture.md §6.2`; ARCH-Q1's executor answer.

**ARCH-Q12. Should `PodmanEventStream` drive container state? — [A] Not in Phase 2 — this is
the plan's most seductive untested path.**
`podman.rs:66-122` does implement a `Stream` over `podman events` filtered to distrobox
containers, and it would make B5's start/stop transitions feel instant. But
`packaging.md §5.4` states plainly that **no real container operation is verifiable in this
environment at any level above argv assertion** — there is no podman, no docker, no distrobox.
Wiring a live event stream would create exactly the class this review is meant to catch: a
long-lived, unverifiable, silently-failing code path whose failure mode is "the UI just stops
updating". Keep the `refresh_interval_secs` poll for Phase 2 and add "event-stream with
watchdog/reconnect" to the post-parity backlog with its own test requirement.
*Evidence:* `podman.rs:66-122`; `packaging.md §5.4`; B5.

**ARCH-Q13. `Status::Other(String)` display — [A] Keep the catch-all; map it for display;
do not model `Paused`/`Restarting` yet.**
`distrobox.rs:108` defines `Up | Created | Exited | Other(String)`. The transitional states
`Other` catches are only reachable via the live event stream (ARCH-Q12), so modelling them
explicitly now would add unreachable enum variants — the same defect class as the Flutter
dead ends. Render through a `Status` → display/label/icon mapping so the free-text case is
localised and icon-bearing rather than raw, and revisit with ARCH-Q12.

**ARCH-Q14. Does the app need `--recurse-submodules` in CI and packaging? — [A] The Flatpak
path is already safe; the RPM path and any new CI checkout are the gaps.**
- **Flatpak: safe.** `cargo-sources.json` fully enumerates libcosmic's graph — including the
  in-tree workspace members `cosmic-config`/`cosmic-theme` and the git deps
  `cosmic-freedesktop-icons`/`cosmic-settings-daemon` — and flatpak-builder opens the
  libcosmic git checkout as a mirror and populates submodules itself. Verified against the
  sibling project's working manifest, which vendors `libcosmic-1.0.0/cosmic-icons/`.
- **Cheap hedge:** add `disable-submodules: false` explicitly so the assumption is visible,
  even though it is the hardcoded default (see A8).
- **RPM: gap.** `gosh-distrobox-manager.spec` builds from a tarball; `%prep` has no
  `git submodule update --init --recursive`, and the committed 18 MB tarball (see §E) does not
  carry populated submodules. Add the submodule init or vendor the tree into the tarball.
- **CI: gap.** `.github/workflows/flatpak.yml` uses `actions/checkout@v3` with no
  `submodules:` key, and any new Rust CI stage needs `submodules: recursive`.
*Evidence:* `flatpak.yml`; `architecture.md §0.1`; sibling `flatpak/git-manifests/` (49
entries incl. `iced_futures-0.14.0`).

### Packaging open questions (12)

**PKG-1. Is `com.system76.Cosmic.BaseApp` worth the dependency? — [A] Yes; keep it.**
libcosmic resolves icons at runtime from the system theme
(`libcosmic/src/icon_theme.rs` → `pub const COSMIC: &str = "Cosmic"`). "Ship the handful of
icons we need" degrades every icon *not* in the handful — including all app-provided and
MIME-type icons and any icon a distro adds — and produces a subtly wrong COSMIC look that is
hard to diagnose. Structural question, answered before manifest authoring: keep the base.
The sibling project's manifest is the working precedent for the whole module layout.

**PKG-2. `libcosmic` as a git source in `cargo-sources.json`, or vendored? — [A] Git source.
This is already solved — port the sibling's generator.**
`/home/gosh/Documents/GitHub/Gosh-Yubico-Authenticator-for-Linux/flatpak/generate-cargo-sources.py`
(23,357 bytes) generates the file with `--check` support and committed `git-packages.json`
(currently `{}`) + `git-manifests/` sidecars. The git route keeps the repo small; vendoring
via `cargo-vendor-filterer` would commit a multi-megabyte tree for no additional guarantee,
since a successful **offline** Flatpak build already proves vendoring completeness
(`packaging.md §5.3` is right about this). Adopt the script rather than rebuilding it.

**PKG-3. The `iced` submodule SHA is recorded in someone else's tree — [A] Add the sidecar;
accept the residue. See also A8.**
The `git-manifests/` sidecar pattern pins exactly what the lockfile's git sources must be, and
`--check` detects staleness **without network access** (the script's own docstring: "A sidecar
that does not describe exactly the lockfile's git sources is a hard error, so --check catches
staleness without needing the network"). That converts silent drift into a hard, offline,
CI-detectable failure — which is the strongest available mitigation. **Extend the sidecar to
cover all three pointers, not just `iced`** (A8: add `cosmic-settings-daemon` /
`pop-os/dbus-settings-bindings` and `cosmic-freedesktop-icons`). Do not vendor iced.
*Residual:* a deliberate force-push of the pointer remains detectable-but-not-preventable;
record as accepted.

**PKG-4. Does the app need `--share=network`? — [A] No, confirmed against the Phase 2 feature
set as documented.**
All container operations run on the host through `flatpak-spawn --host`; there is no update
check, no release-note fetch, no in-sandbox icon pack fetch (icons come from the base app),
and no crash reporting in the documented feature set. Icons and fonts are covered by PKG-1 and
UX-12. Keep the omission, and make the CI check in `packaging.md §5.2` ("new direct `std::fs`
usage") a companion rule for "new in-sandbox network usage".

**PKG-5. Do we adopt libcosmic's `dbus-config`, and what happens on GNOME? — [A] NO — measured,
not theorised. This is the strongest cross-project answer in this review.**
The sibling project enabled `dbus-config`, measured **15 ERROR log lines per 20-second
window** with `com.system76.CosmicSettingsDaemon` absent (the GNOME case), and dropped the
feature (its `D27b`, commit `898ba1a`, `PLAN.md` T9a "Drop `dbus-config` (D27b) … **Done**").
Result: **15 → 0 errors**, and live theme flips still arrive, because cosmic-config's
`watch_config` falls back to a file watcher and `--filesystem=xdg-config/cosmic:ro` delivers
the changes. Therefore:
- Use plain cosmic-config file persistence. Do **not** enable `dbus-config`.
- Do **not** add `--talk-name=com.system76.CosmicSettingsDaemon`.
- Add `--filesystem=xdg-config/cosmic:ro` (the sibling manifest has exactly this).
- GNOME degradation: no live accent change — acceptable, and strictly better than 15 errors a
  window.
This directly answers §3's "main GNOME-specific risk" and PKG-12's sibling concern; the
"fallback when the daemon is absent" is *the mechanism that already works*.
*Evidence:* sibling `DECISIONS.md` D27/D27b, `PLAN.md` T9a, manifest
`- --filesystem=xdg-config/cosmic:ro`, explicit omission of
`com.system76.CosmicSettingsDaemon`.

**PKG-6. Is `--filesystem=home` genuinely avoidable? — [A] Yes, C1 holds, and the allowlist is
now enumerable.** See A7. The only sandboxed filesystem touch is
`distroshelf-terminals.json`; the export `.desktop` files are written by the host script and
never opened in-sandbox. **But note the second-order consequence:** if ARCH-Q5 is taken
(terminals move into cosmic-config), that last touch disappears too, and the `std::fs` CI grep
recommended in `§5.2` can start from an **empty** allowlist — the strongest possible position.
*Evidence:* `supported_terminals.rs:112-115,255,270`; `ux.md §1.4.2`.

**PKG-7. Should the RPM spec be maintained at all? — [R].**
*Distribution/product decision.* The drift is worse than `architecture.md`/`packaging.md` §4.1
suggest, and it is now quantified:
- `.spec`: `Version: 1.0.0`, `Summary: A Flutter application for managing Distrobox
  containers`, `BuildRequires: gtk3-devel`, `Requires: gtk3`, `%changelog` frozen at
  `Fri Jan 24 2025`.
- `rust/Cargo.toml`: `version = "1.0.2"`.
- `rust/data/io.github.gosh_distrobox_manager.metainfo.xml.in`: newest release `1.0.0`,
  `date="2026-01-23"`.
- Three different versions, and the spec's `Summary` and GTK3 dependencies are all wrong for a
  libcosmic app.

Keep it (parity: it is the app's only non-Flatpak path and `Requires: distrobox` is
load-bearing host tooling metadata) **only if** §4.2's lockstep check becomes mandatory. If it
cannot be maintained, retire it explicitly rather than shipping a spec that produces a broken
package. Residual because it is a distribution-scope call.

**PKG-8. What is the actual `Message` enum stability requirement? — [R].**
*Process decision.* One test per variant is the wrong bar and will become a tax:
`architecture.md §2.3` is 12 nested sub-enums, and Phase 2 will churn them. An exhaustive
`match` with **no `_` arm** already gives compiler-enforced "no unhandled variant" coverage —
which is what §2.2 actually wants — so per-variant tests duplicate the compiler. Recommendation:
a smaller set of high-value behavioural tests + the exhaustiveness requirement + one test per
*nested sub-enum's* dispatch. Agree the exact bar in `PLAN.md`.
*Evidence:* `packaging.md §2.2`; `architecture.md §2.3`.

**PKG-9. Is deleting the GSettings schema safe for existing users? — [A] Yes for this fork's
own installs; the migration path is from upstream's schema id, and it is a ~20-line import.**
Traced through history:
- The `io.github.gosh_distrobox_manager.gschema.xml` in this repo was **added by the Flutter
  rewrite commit `844e55e`** (`git log --follow` shows `844e55e` as its creator on this
  branch). No code reads it — verified: no `gio::Settings`/`Settings::new` reference anywhere
  in `rust/` or `lib/`. **It is a born-dead artifact: created and orphaned in the same
  commit.** Deleting it is safe; nothing this fork ever shipped wrote to it.
- **But released GTK4 versions did read GSettings** — under the inherited id
  `com.ranfdev.DistroShelf`. At `v1.4.4`:
  `src/dialogs/preferences_dialog.rs:148,152,159-163`,
  `src/models/root_store.rs:70,111,125,130,174-177,359-361`,
  `src/widgets/welcome_view.rs:201-202,369-370`, `src/widgets/window.rs:122,217` — reading and
  **writing** `distrobox-executable`, plus the window geometry pair.
- All upstream tags (`v0.1.0` … `v1.4.4`) are DistroShelf tags; this fork has no tags of its
  own.
So: **a one-time import of `com.ranfdev.DistroShelf`'s `distrobox-executable`,
`window-width`, `window-height` (and `selected-terminal`) is worth ~20 lines** if any user
arrives from a GTK4 build. Whether that population exists is a product question, but the
import is cheap enough that the honest answer is "write it, log when it fires, and delete it
after one release". This also supplies ARCH-Q7's real migration source and ARCH-Q8's real
semantics.
*Evidence:* `git log --follow rust/data/io.github.gosh_distrobox_manager.gschema.xml`;
`git grep gio::Settings v1.4.4`; `.spec`; `git tag`.

**PKG-10. Does the smoke test prove enough? — [A] No; add a readiness signal, and this is
mandatory rather than nice-to-have.**
`packaging.md §5.4` establishes that **every meaningful behaviour is unverifiable in this
environment**. That makes the smoke test the *only* automated signal that the shipped binary
does anything at all, and a stay-alive + clean-SIGTERM check passes for an app that renders a
blank window. Minimum bar: (1) process alive, (2) a readiness line emitted when the first
`view()` completes / `Application::init` returns (stderr, easy to grep), (3) clean SIGTERM exit
code, (4) `--talk-name=org.freedesktop.Flatpak` removal must make it fail (already planned,
§5.3). A debug/IPC hook asserting a known widget exists is the gold bar; defer that and record
it. The §2.4 note 3 readiness signal is therefore promoted from "note" to requirement.
*Evidence:* `packaging.md §2.4`, §5.3, §5.4.

**PKG-11. `--locked` in the developer loop or CI only? — [A] CI only.**
`--locked` is correct where the lockfile is the contract (CI, release) and wrong in an active
migration where the lockfile legitimately changes several times a day. A local `verify.sh` that
fails on every dependency bump teaches contributors to bypass `verify.sh` — which costs the
project every *other* check the script performs. Split the script: fast stages unlocked
locally, full stages `--locked` in CI (matching §2.5's per-push/tag split).

**PKG-12. Who owns the `cargo-sources.json` regeneration? — [A] Answered by an existing
artifact: the sibling's `generate-cargo-sources.py --check`, owned jointly by CI and the
release checklist.**
Port `flatpak/generate-cargo-sources.py` plus the committed `git-packages.json` and
`git-manifests/` sidecars. Ownership: **CI runs `--check` on every push** (it is offline-capable
by design, so it is cheap and belongs in the fast stage), and the release checklist
(`§4.2`) runs the regeneration as an explicit step. That gives the generated artifact a
mechanical owner instead of a human intention. This also covers PKG-3's three git pointers by
construction.
*Evidence:* sibling `flatpak/generate-cargo-sources.py` docstring; `flatpak/git-manifests/`
(49 entries incl. `cosmic-config-1.0.0`, `iced_futures-0.14.0`, `accesskit_unix-0.18.0`);
`flatpak/git-packages.json` = `{}`.

---

## D. Parity checklist granularity: 193 rows

**Recommendation: keep all 193 rows.** The collapsed ~110 is the wrong instrument.

Justification, in priority order:

1. **The 15 `dead` rows are the deliverable, and collapsing hides them.** `ux.md:622`
   enumerates them precisely (`#23`, `#28`, `#29`, `#68`, `#88`, `#95`, `#102`, `#103`, `#122`,
   `#130`, `#146`, `#147`, `#167`, `#180`, `#181`). A collapsed checklist merges "New Container
   does nothing" into "Dashboard quick actions" — and the whole point of this migration is that
   affordances which look functional and are not are the defect class being removed (UX-5).
   **Collapsing is how those 15 survive into the port.**
2. **The 14 `dup` rows are the simplification budget.** They are the evidence for `ux.md §3.6`'s
   deduplication (5 progress dialogs → 1, and the A9/A6 count corrections). Collapse them and the
   dedup work loses its justification at review time.
3. **193 is not a schedule multiplier; it is a checklist.** The estimate should come from the
   ~12 page-level work items, with the 193 rows as the *acceptance* criteria. The row count and
   the effort estimate are different artifacts and the question conflates them.
4. **The status counts are the review artifact.** `exists 148 · dead 15 · dup 14 · missing 13 ·
   bug/fragile 3` is a defensible, auditable summary of the Flutter app's real state. A 110-row
   list cannot produce that table.
5. **Mechanical cost of the larger list is near zero** — it is a markdown table, reviewed once
   per page during the port, and it is the thing that lets a reviewer say "page 7 is not done"
   without re-reading Dart.

One caveat to record: the numbers in the checklist must be **corrected before it becomes the
contract**. This review found three that are not: `_getDistroIcon` is in **9** files not 8 (A6);
status helpers are **2** named helpers with logic duplicated in 4 files, not 4 named helpers;
and `AlertDialog` appears **23** times across `lib/`, not 11. Fix the counts, then freeze.

---

## E. What is missing from all three documents

Ordered by consequence. The user's suggested candidates are all confirmed, plus significant
additions.

**E1 — The sibling project. (Highest consequence.)** `Gosh-Yubico-Authenticator-for-Linux` has
already completed this exact exercise and produced a Phase-1 review, `PLAN.md`, measured
decisions (D26/D27/D27a/D27b/D31/D32), a working libcosmic Flatpak manifest, and a reusable
`generate-cargo-sources.py` + `git-manifests/`. It answers PKG-5, PKG-12 and ARCH-Q9 *on the
record from measurement* and supplies the tooling §1.3 and §4.2 describe building. **Not one of
the three documents mentions it.** This is the single largest avoidable cost in the plan.

**E2 — The a11y bus grant is missing from `packaging.md §1.4`.** `--talk-name=org.a11y.Bus` is
required for AT-SPI to leave the sandbox; without it, `accesskit_unix` is inert and `ux.md §5.3`'s
P0 a11y claim is false on the only shipping channel. Measured in the sibling project. See UX-16.

**E3 — The binary-name contradiction.** `rust/Cargo.toml:2` → `name = "gosh_distrobox_manager"`;
`rust/data/io.github.gosh_distrobox_manager.desktop.in:3` → `Exec=gosh_distrobox_manager`;
`.spec:1` → `Name: gosh-distrobox-manager`. Three spellings of one binary. If
`architecture.md §1.2`'s crate naming (which discusses `core`/`app` crate names) changes the
binary name, the manifest `command:`, the `.desktop Exec=`, the `.service.in`, and the spec's
`_bindir` all break together and silently. **Pick one and write it into all four places.**

**E4 — The two missing unpinned git dependencies.** `cosmic-settings-daemon`
(`pop-os/dbus-settings-bindings`) and `cosmic-freedesktop-icons` (`freedesktop-icons`), both in
libcosmic's Linux target block and therefore always compiled. `packaging.md §1.3` tracks only
`iced`. See A8/PKG-3.

**E5 — The stale 18 MB committed tarball.** `gosh-distrobox-manager-1.0.0-linux-x64.tar.gz` is
**tracked in git** (verified) at 18,084,265 bytes (17.2 MiB), containing a Flutter Linux bundle
that no longer corresponds to anything buildable. It is not mentioned by any document, it is
part of the repo the migration inherits, and it will be deleted by the Flutter-tree removal in
D3/S8 — **but git will keep it in history forever** unless history is rewritten, so the cost
must be accepted explicitly rather than discovered. Decide: delete in S8 and accept, or
`git filter-repo` at a scheduled point.

**E6 — `.github/prompts/release.prompt.md` points at a file that does not exist.** It instructs
"Update the version number in the root `meson.build`". `HEAD:meson.build` is **ABSENT**
(verified: it exists at `v1.4.4` and was deleted with the Flutter rewrite). This is the same
class as `.github/workflows/flatpak.yml`, which triggers on tags only, uses
`actions/checkout@v3`, builds a **root manifest that does not exist on this branch**, and runs
`ninja test` / `meson dist` — i.e. it is dead. `ux.md`/`packaging.md` do not mention either file.
Both must be rewritten or deleted in the same task that fixes CI.

**E7 — The `.github/agents/*` and `.github/ISSUE_TEMPLATE/bug_report.md` are unowned.** Seven
agent definitions plus a bug-report template describe the Flutter app (including
`gnome-doc-librarian/librarian.py`). No document says who updates them or whether they are
in scope. `AGENTS.md` — the file agents actually read — still describes the Flutter UI +
FRB architecture and will actively mislead Phase 2 implementers from the first task. **Add
"update `AGENTS.md`" to the D3/S8 strip task.**

**E8 — No Rust CI stage exists.** `packaging.md §2.5` plans CI, but the repo currently has
exactly one workflow (`flatpak.yml`), and it does not build Rust. There is no `fmt`, no
`clippy -D warnings`, no `cargo test`, and no `generate-cargo-sources.py --check`. Given that
`architecture.md`'s entire safety story is "every strip step compiles, tests, and
clippy-cleans" (§1.3), the enforcement mechanism for that story does not yet exist. Create it
before the first strip step, not alongside it.

**E9 — `<screenshots>` is absent from the metainfo, and the screenshots already exist in the
repo.** `rust/data/io.github.gosh_distrobox_manager.metainfo.xml.in` has no `<screenshots>`
element at all (Flathub requires it for submission), while `rust/data/screenshots/` holds
3 PNGs (569 KB) and `images/` holds 13 (2.3 MB) — 65 PNGs / 3.6 MB repo-wide. Also wrong in
the same file: `<developer id="io.github">`, three URLs pointing at the wrong org, and
`<translation type="gettext">` naming a catalogue that does not exist (**`po/` was deleted with
the Flutter rewrite**; `git ls-tree HEAD` has no `.po`/`.pot` files). `packaging.md §1.5`
covers the desktop/metainfo/icon *fixes* but not the missing screenshots or the gettext
claim. Note `packaging.md §5.1`'s "~5 MB of PNGs" estimate is high: the real total is **3.6 MB
across 65 files**, and the metainfo-relevant set is 569 KB.

**E10 — `dbus-config` on GNOME is treated as an open risk when it is a solved problem.** Both
`packaging.md §3` and Q5 frame it as the main GNOME failure risk; the sibling project measured
it (15 errors/20 s → 0 after dropping the feature) and shipped the answer. See PKG-5. Listed
here because it is missing from all three docs *as a known answer*.

**E11 — `update-caches.sh` and `fix-icon-cache.sh` are unmentioned.** Both exist at HEAD
(`fix-icon-cache.sh`, `update-caches.sh`) and are part of the packaging surface. The documents
plan icon handling (§1.5) without accounting for the existing scripts, which will otherwise
linger as dead tooling or silently disagree with the new plan. Also unmentioned:
`update-desktop-database` handling.

**E12 — The cross-platform trees are unaddressed.** `android/`, `ios/`, `linux/`, `macos/`,
`web/`, `windows/`, plus `pubspec.yaml`, `analysis_options.yaml`, `flutter_rust_bridge.yaml`,
and `test/` all exist at HEAD. `ux.md §4.5` reasons about mobile-shaped *UI* surfaces but no
document says these directories are deleted. D3 says "delete the Flutter tree last" without
enumerating it, and the `linux/` runner in particular will confuse anyone looking for "the
Linux build". **Enumerate the deletion set in S8.**

**E13 — The 8.9k-line Flutter tree is the parity reference and it is being deleted.** Nothing in
the plan says where the frozen reference goes. Once `lib/` is gone, `ux.md §6`'s 193 rows are the
*only* record of what the Flutter app did — which is another argument for keeping all 193
(§D) and for tagging the pre-deletion commit explicitly before S8 runs.

**E14 — `RPM-BUILD.md` and `build-rpm.sh` are not in the version-unification table.**
`packaging.md §4.1` enumerates the drift across the spec, `Cargo.toml`, and the metainfo, but
the repo also carries `RPM-BUILD.md` and `build-rpm.sh` at HEAD, both of which encode the
version and the build procedure. Any version check (§4.2) that does not cover them will pass
while `build-rpm.sh` produces a mislabelled package.

---

## F. Per-doc verdict, section by section

### `ux.md` — **Approve with changes**

| Section | Verdict | Note |
|---|---|---|
| §0 Verification method | Approve | — |
| §1 Navigation structure | **Approve with changes** | UX-1: the ≥1000px premise is backwards (Flutter sets `NavigationRailLabelType.none`, i.e. hides labels). The conclusion (accept COSMIC condensed + toggle) happens to be right; the stated reason is wrong, and the reason is what a future reader will rely on. Fix the sentence or the next person re-litigates it. |
| §2 Per-screen mapping | Approve | Mapping is sound. Add UX-A2's `block_io` note. |
| §3 Dialog/pattern unification | Approve | 5 progress dialogs → 1 and 3.3's confirm helper are the right consolidations. Add the corrected duplicate counts (A6). |
| §3.6 Icon/colour dedup | Approve with changes | "8-file" is **9** files; status helpers are 2 named + 4 unnamed copies. |
| §4.1 Start container | Approve with changes | Right call (P1-blocking), wrong sequence — must land in the backend task group (UX-5). |
| §4.2 Dead ends | Approve | This is the doc's strongest section; it is the evidence for §D. |
| §4.3 File picker P0 | **Approve with changes** | `rfd` → `xdg-portal`; `directory()`/`file_name()` are undeliverable (UX-15). |
| §4.4 Orphan pages | Approve | Consistent with `architecture.md`. |
| §4.5 Missing features | Approve | Taxonomy is the right instrument for UX-18. |
| §4.6 Theme toggle | Approve | Dropping it is correct; COSMIC owns it. |
| §5.3 Screen reader / AT | **Approve with changes** | P0 claim is false on Flatpak until `--talk-name=org.a11y.Bus` is added to the manifest (UX-16/E2). |
| §5.1/5.2/5.4 Keyboard, i18n, text scaling | Approve | §5.4 is the reason UX-12's Inter question matters. |
| §6 Parity checklist | **Approve** | Keep 193 (§D); correct three counts before freezing. |
| §7 Open questions | Approve | See §C for all 19. |

### `architecture.md` — **Approve with changes**

| Section | Verdict | Note |
|---|---|---|
| §0.1 API facts | Approve | All verified at the pinned rev. |
| §0.2 The runtime trap | **Reject the mechanism** | `iced/winit/src/lib.rs:2030` wraps `program.update(message)` in `runtime.enter()`. No "no reactor running" panic from `update()`. The *rule* (`cosmic::task::future`) is right; the *reason* and the "compiler cannot catch" framing are wrong (ARCH-Q1). This is the most important correction in the review — a plan justified by a false premise will be argued with, and the argument will be lost. |
| §0.3 Backend bugs B1–B9 | Approve | Confirmed against source. |
| §0.4 Not a bug | Approve | — |
| §1.3 Strip plan S1–S8 | **Approve with changes** | Add UX-A9's lint catch (`disallowed-methods`) and, to S8, "delete the enumerated Flutter trees + the 18 MB tarball + update `AGENTS.md`" (E5/E7/E12/E13). `[lib] crate-type` handling at `:210-211` is correct. |
| §2.1/2.2 Coverage tables | Approve | 35 `api.rs` fns and all `AppStateProvider` members verified by count. |
| §2.3 Message enum | Approve | See PKG-8 for the test bar. |
| §2.4 Env guard | Approve | Correct: `checked_run_command` already exists upstream — this is a cherry-pick (ARCH-Q10). |
| §3.4 Subscription design | Approve | See ARCH-Q2. |
| §5 Config | **Approve with changes** | §5.1 is correct and strengthened by PKG-5/PKG-6. §5.2 should name `--filesystem=xdg-config/cosmic:ro` for live watch. §5.3's keys are **correct** — ARCH-Q6 is resolved (derive uses `stringify!`); add the kebab→snake rename note for the legacy import. §5.4's `.service.in` deletion collides with ARCH-Q9. |
| §6.1 T1 verbatim | **Approve with changes** | The T1 table **drops 599 lines** it should account for: `container_runtime.rs` (54), `docker.rs` (95), `podman.rs` (138), `supported_terminals.rs` (312). §0.4 and §6.2 place these among the clean modules, and `packaging.md §2.2` claims "zero changes to backends/". Those three statements cannot all be true. Reconcile — and note `supported_terminals.rs` is the one that must change under ARCH-Q5. |
| §6.2 T2 adapt | Approve | ARCH-Q11's `?Send` correction applies here. |
| §6.3/6.4 T3 + fixes | Approve | — |
| §7 Ordering | **Approve with changes** | Add `start` to a backend row before the three banner pages (UX-5), and pull the CI creation forward (E8). |
| §8 Open questions | Approve | See §C for all 14; Q6 closed by this review. |

### `packaging.md` — **Approve with changes**

| Section | Verdict | Note |
|---|---|---|
| §0 Verified environment | Approve | The constraint analysis is the doc's foundation and it is sound. |
| §0.2 Corrections C1/C2 | Approve with changes | C1 correct as stated. C2's conclusion is right; its mechanism ("`disable-submodules` defaults to false") is wrong, and it misses two git deps (A8). |
| §1.1 Runtime/SDK/base | Approve | PKG-1 answered: keep the base. |
| §1.2 Module layout | Approve | — |
| §1.3 Vendoring strategy | **Approve with changes** | Adopt the sibling's `generate-cargo-sources.py` instead of building one (PKG-2); extend the sidecar to all three git pointers (A8/PKG-3). This is the doc's hardest section and the answer already exists in the org. |
| §1.4 finish-args | **Approve with changes** | **Add `--talk-name=org.a11y.Bus`** (UX-16/E2). **Add `--filesystem=xdg-config/cosmic:ro`** (PKG-5). **Keep `--filesystem=distroshelf-terminals.json` only if ARCH-Q5 is rejected** — if terminals move into cosmic-config, the grant and the whole of §1.4.2's fragility disappear (PKG-6). Do **not** add `--talk-name=com.system76.CosmicSettingsDaemon`. |
| §1.5 Desktop/metainfo/icon fixes | Approve with changes | Add the binary-name decision (E3), `<screenshots>` + the bogus `<translation type="gettext">` (E9), and the fate of `update-caches.sh`/`fix-icon-cache.sh`/`update-desktop-database` (E11). |
| §1.6 GSchema removal | **Approve, strengthened** | The schema was born dead in `844e55e`; deletion is safe. Add the one-time `com.ranfdev.DistroShelf` import (PKG-9). |
| §2.1 verify.sh | Approve with changes | Split `--locked` (PKG-11); add the `disallowed-methods` lint (ARCH-Q10) and the `std::fs` grep (§5.2) as stages. |
| §2.2 Unit test plan | Approve with changes | "zero changes to backends/" contradicts `architecture.md §6.1`'s missing 599 lines and ARCH-Q5. |
| §2.3 Integration test plan | Approve | `NullCommandRunner` is the right harness. |
| §2.4 Smoke test | **Approve with changes** | Promote the readiness signal to a requirement (PKG-10). |
| §2.5 CI | **Approve with changes** | Must include a Rust stage (E8), `generate-cargo-sources.py --check` (PKG-12), and `submodules: recursive` (ARCH-Q14). |
| §3 GNOME verification | **Approve with changes** | The "configure rfd" step does not exist (UX-15); the "main GNOME-specific risk" (dbus-config) is already answered by measurement (PKG-5). |
| §4 Version unification | Approve with changes | Extend the table to `RPM-BUILD.md`/`build-rpm.sh` (E14) and note the spec is at 1.0.0 while `Cargo.toml` is at 1.0.2 (PKG-7). |
| §5 Risks | Approve | §5.2 and §5.4 are the best-written parts of the doc — §5.4 in particular is why PKG-10 and ARCH-Q12 answer the way they do. Only correction: §5.1's "~5 MB of PNGs" → 3.6 MB across 65 files (E9). |
| §6 Open questions | Approve | See §C for all 12. |

---

## G. Residual risk list for `PLAN.md`

### G.1 Blocking pre-conditions (must be settled before Phase 2 starts)

1. **One explicit libcosmic feature list.** Resolve the `architecture.md:238` vs
   `packaging.md §1.3` contradiction to a single `default-features = false` + explicit list,
   written in one place and referenced by both docs (B.a).
2. **The a11y bus grant.** Add `--talk-name=org.a11y.Bus` to `packaging.md §1.4.1`, or drop the
   P0 a11y rating. Shipping the current plan means an inert a11y path on the only shipping
   channel (UX-16/E2).
3. **The file picker decision.** Switch to `xdg-portal`/ashpd, delete the "configure rfd" step,
   and re-scope checklist row `#88` — `directory()`/`file_name()` are undeliverable (UX-15).
4. **The binary name.** One spelling across `Cargo.toml`, the `.desktop.in`, the `.service.in`,
   the manifest `command:`, and the `.spec` (E3).
5. **The `distroshelf-terminals.json` decision.** Accept ARCH-Q5 (terminals into cosmic-config)
   or keep the grant and own its silent-failure mode. Do not ship both half-decided (B.b).
6. **The `.github` cleanup.** `flatpak.yml`, `release.prompt.md`, and `AGENTS.md` all describe
   the Flutter app; `release.prompt.md` references a `meson.build` that does not exist. Fix in
   the same task that creates the Rust CI stage (E6/E7/E8).
7. **The `start` sequencing.** `start` goes in a backend row in `architecture.md §7`, before
   the three banner pages' copy freezes (UX-5).

### G.2 Mandated corrections (doc edits, no decision needed)

1. `architecture.md §0.2` — the runtime-trap mechanism (ARCH-Q1). Keep the rule, fix the reason.
2. `architecture.md §6.1` — account for the 599 missing lines; reconcile with §0.4/§6.2 and
   `packaging.md §2.2`'s "zero changes to backends/".
3. `packaging.md §1.3` — add `cosmic-settings-daemon` and `cosmic-freedesktop-icons` (A8).
4. Checklist counts — `_getDistroIcon` is 9 files; status helpers are 2 named + 4 unnamed;
   `AlertDialog` is 23 occurrences; PNG total is 3.6 MB. Fix before freezing the 193 rows (A6/E9).

### G.3 Residual risks (product/scope/process — record with an owner)

| # | Risk | Needs |
|---|---|---|
| R1 | Multi-container side-by-side comparison is lost (UX-3) | Product |
| R2 | `disk_usage` backends with no parity target (UX-6) | Product/scope |
| R3 | Brand accent `#137FEC` + Inter font lost to theme roles + system font (UX-12) | Brand |
| R4 | ~600-string i18n extraction absorbs schedule (UX-14) | Owner/capacity |
| R5 | No named owner for the intended-changes table (UX-18) | Process |
| R6 | RPM spec drift; three versions in-tree, spec still describes a GTK3 Flutter app (PKG-7) | Distribution |
| R7 | `Message` test bar undecided; per-variant tests will become a tax (PKG-8) | Process |
| R8 | The three git submodule/pointer SHAs live in `pop-os/libcosmic`'s tree; drift is detectable, not preventable (PKG-3) | Accepted |
| R9 | `PodmanEventStream` deferred; the app remains poll-only, so B5's transitions stay laggy (ARCH-Q12) | Accepted |
| R10 | Core/app task-registry duality and the `TaskMsg::Expired` round-trip (ARCH-Q4) | Accepted |
| R11 | The 18 MB stale tarball and the whole Flutter tree stay in git history forever (E5) | Accepted or `filter-repo` |
| R12 | The `com.ranfdev.DistroShelf` GSettings import: write it, log when it fires, remove after one release (PKG-9) | Product + task |
| R13 | Orca/AT-SPI has never been verified on this distro; manual release step required (UX-16) | Manual step |

### G.4 New tasks this review adds to `PLAN.md`

1. Port `flatpak/generate-cargo-sources.py` + `git-manifests/` + `git-packages.json` from the
   sibling project; wire `--check` into CI (PKG-2/PKG-3/PKG-12).
2. Add `clippy.toml` `disallowed-methods = ["std::process::Command::new"]` for `core/`, with an
   `#[allow]` at the runner indirection (ARCH-Q10).
3. Create the Rust CI stage: `fmt`, `clippy -D warnings`, `test`, `--locked`,
   `submodules: recursive`, `generate-cargo-sources.py --check` (E8/ARCH-Q14/PKG-11).
4. Add the readiness signal to `scripts/smoke-test.sh` (PKG-10).
5. Write the intended-changes table into `PLAN.md` and name its owner (UX-18).
6. Enumerate the S8 deletion set (Flutter trees, tarball, `po/`, `flutter_rust_bridge.yaml`,
   `analysis_options.yaml`, `pubspec.yaml`, `test/`) and tag the pre-deletion commit
   (E5/E12/E13).
7. Add the `dbus-config`-absent GNOME verification to `packaging.md §5.4`'s manual list, and the
   Orca run alongside it (PKG-5/UX-16).
8. Add `<screenshots>` to the metainfo and remove the `<translation type="gettext">` claim, or
   restore a `po/` catalogue (E9).

---

## H. GO / NO-GO for Phase 2

### **GO — with 7 blocking pre-conditions (§G.1), 4 mandated corrections (§G.2), and 13 recorded residual risks (§G.3).**

Reasoning:

- **All eight stated corrections to the recon briefs are substantively correct** (A1–A8). The
  three documents are honest about their own uncertainty — `architecture.md` Q6 explicitly says
  it did not resolve a key question, and this review resolved it (keys are the `stringify!` field
  names, so §5.3's table is right). A plan that flags its own unverified premises is a plan worth
  executing.
- **The architecture is sound.** The `CommandRunner`/`flatpak-spawn --host` indirection, the
  pinned-rev dependency strategy, the `spawn_task`/`TaskId`/`TaskRegistry` design, and the
  single-window page stack are all correct readings of the constraints, and the API facts in
  §0.1 verify at the pinned rev.
- **The packaging plan is unusually well-grounded.** §5.3/§5.4's split of what can and cannot be
  verified locally is the most valuable thing in the three documents, and it is what makes the
  residual risk list short and honest.
- **No correction is wrong**, so nothing needs to be un-decided. Every issue found is either a
  fix-forward doc edit or a downstream decision that can be made in parallel with the first strip
  steps.
- **The strongest reason to GO is E1.** The org has already solved the two hardest problems in
  this plan (offline git vendoring for libcosmic and the GNOME/`dbus-config` behaviour) with
  measurement and reusable tooling. Adopting it removes more risk from Phase 2 than any other
  single action available.

**Do not start Phase 2 page work before G.1 items 1–4 are closed** (one feature list, the a11y
grant, the file-picker switch, the binary name). Items 5–7 can be closed during the first strip
steps. The G.3 residuals do not block GO; they block *release*, and each needs an owner named in
`PLAN.md` before the checklist is frozen.
