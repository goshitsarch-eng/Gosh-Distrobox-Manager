# UX Migration Plan — Flutter → libcosmic

Phase 1 deliverable. **No source changes.** This document is the UX half of the migration; it feeds `PLAN.md`.

- **Parity source of truth:** the Flutter UI in `lib/` (8,942 lines). `AGENTS.md` confirms the GTK4/Rust UI is legacy and does not ship.
- **Target:** libcosmic `1.0.0`, pinned rev `a401af8b1c54a8abd393b8c5b7c8809402f83850` (2026-09-10, edition 2024, `rust-version` 1.93, lib target renamed `cosmic`). Not on crates.io — git dependency only. Never depend on crates.io `cosmic` (squat) or tag `v0.12` (stale pre-1.0).

## 0. Verification method

Every libcosmic API name below was read from the actual source at the pinned rev (shallow clone to `/tmp/libcosmic-ref`, `HEAD` confirmed = pinned SHA), not from memory. Where the recon brief and the source disagreed, the source wins and the correction is called out.

**Corrections to the recon brief:**

1. **"Suggested cargo features (UNVERIFIED)"** — verified: `winit`, `tokio`, `wayland`, `x11`, `a11y`, `dbus-config`, `multi-window` are all already in `default = [...]` in `Cargo.toml`. But we do **not** build against those defaults: libcosmic is taken with `default-features = false` plus an explicit list written once in PLAN §1, so every feature we need must be *named*, and the defaults above buy us nothing. That list is `winit`, `tokio`, `a11y`, `wayland`, `x11`, `multi-window`, `about`, `xdg-portal`. Two absences are deliberate and load-bearing: `dbus-config` is **not** enabled (D23 — it routes `watch_config` through a `CosmicSettingsDaemon` proxy that does not exist on GNOME), and `rfd` is **not** enabled (D16 — the file picker is `xdg-portal`/`ashpd`, and `rfd`'s GTK backend would drag GTK into the sandbox).
2. **`nav_bar` is not the app-chrome nav widget.** `widget::nav_bar()` renders a `segmented_button::VerticalSegmentedButton` and its doc comment says "Navigation side panel for switching between views"; it is what `Application::nav_bar()` returns and what panel applets use. It is still the right *widget*, but the app-level integration is `Application::nav_model()` + `on_nav_select()`, and libcosmic builds the chrome and the responsive behaviour for you.
3. **`container_stats` is real but is not disk usage.** `Distrobox::get_container_stats` shells out to `podman stats --no-stream` (falling back to `docker stats`) and returns `cpu_percent`, `memory_usage`, `memory_limit`, `memory_percent`, `network_io`, `block_io`. There is **no disk-usage field** — see §4.4.
4. **`images_page` is not a local-image list.** `Distrobox::list_images` runs `distrobox create --compatibility`, i.e. it returns distrobox's curated *distro catalogue*, not local container images. This matters for §4.5.
5. **The `fakers/` module is not mock data.** `rust/src/fakers/` is the `CommandRunner` abstraction layer (real/native, Flatpak-wrapped, host-exec-wrapped). `api.rs` calls `CommandRunner::new_real()`. Shipped data is real.
6. **Start-container does not exist anywhere in the stack.** No `start`/`launch` in `rust/src/api.rs`; `Distrobox` has `enter_cmd`, `run_in_container`, `run_in_container_streaming`, `launch_app` (host app launch, not container start). The container **is** started implicitly as a side effect of `enter`, but there is no API that starts a container and returns. This is a backend gap, not just a UI gap (§4.1).

---

## 1. Navigation structure

### Proposal

Adopt the **stock COSMIC application shell** rather than reimplementing `home_screen.dart`'s `LayoutBuilder`.

```
Application impl
├── nav_model()      -> Option<&nav_bar::Model>     // the 8 destinations
├── on_nav_select()  -> Task<Message>                // nav_model.activate(id) + per-page load
├── header_start()   -> Vec<Element>                 // page-scoped back/close affordance
├── header_center()  -> Vec<Element>                 // page title (or use window title)
├── header_end()     -> Vec<Element>                 // per-page actions (Refresh, Upgrade All)
├── context_drawer() -> Option<ContextDrawer>        // see §3.2 — replaces bottom sheets
└── dialog()         -> Option<Element>              // see §3.1/§3.3 — single active modal
```

**Why not raw `nav_bar()` + hand-rolled `Row`:** libcosmic already does everything `home_screen.dart` does, and more, correctly:

| Flutter (`home_screen.dart`) | libcosmic equivalent (verified) |
|---|---|
| `NavigationRail` (≥600px) | `widget::nav_bar` — built by `Application::nav_bar()`, wraps `segmented_button::vertical` |
| `NavigationRail(extended: width>=1000)` | Built-in: `Core::is_condensed()` + `Core::nav_bar_toggle_condensed()`; the user toggles, not a hard width breakpoint |
| `NavigationBar` bottom nav (<600px) | `Core::nav_bar_active()` collapses the nav; `widget::nav_bar_toggle()` in the header. **No bottom-nav-bar equivalent exists** — COSMIC does not put app nav at the bottom |
| `NavigationRailLabelType.all` vs `.none` | `segmented_button` label rendering + condensed mode |
| `IconData` per destination | `nav_model.insert().icon(icon).text(...).data(PageId)`, `Entity::icon`/`Entity::text` (both verified on `segmented_button::model::Entity`) |
| `_selectedIndex` + `setState` | `nav_model.activate(id)`; `nav_model.activate_position(0)` for the boot default |
| `VerticalDivider` + `Expanded(child: _pages[i])` | Built by libcosmic around `Application::view()` |
| Tabs rebuild on switch, losing state | Per-page state lives on the `App` struct and survives; only one page's `view()` runs per frame |

### Responsive behaviour — the one real decision

Flutter hard-codes **two** breakpoints (600 → rail, 1000 → extended rail). libcosmic gives one **condensed** state driven by available width plus a user-controlled toggle. There is no way to reproduce a 1000px "always extended" rule without fighting the framework.

**Recommendation: accept the COSMIC behaviour** (condensed nav + toggle) and drop both breakpoints. Rationale: the 1000px auto-expand rule is a Flutter-ism that produces a nav rail the user cannot collapse; COSMIC's model is a user preference persisted by the framework. Reproducing it would mean overriding `Application::nav_bar()` and reimplementing the chrome, which forfeits the responsive and a11y work libcosmic already does. Flagged in §7 as an open question in case product insists on the auto-expand.

**8 destinations:** keep all 8, same order and icons (Dashboard, Containers, Images, PackageManager, Updates, Backups, ActivityLogs, Settings). Note the bottom-nav labels already differ from the rail labels ("Home"/"Boxes"/"Pkgs"/"Logs"); pick one set — recommend the rail labels.

**Nav context menu:** `nav_context_menu()` gives right-click-on-nav-item for free (libcosmic's `menu::nav_context`). Not required for parity; free if wanted.

### Consequences to plan for

- The nav is a **single-window** shell. `container_details_page`, `container_terminal_page`, `apps_page`, and `create_container_page` are pushed routes in Flutter. In libcosmic these become either (a) extra top-level nav entries — wrong, they are contextual; (b) a page stack on the `App` struct rendered inside `view()`; or (c) real OS windows via the `multi-window` feature (`view_window(id)`). **Recommendation: (b) for details/terminal/apps/create** — matches user expectation of a back affordance and keeps one window; reserve (c) for nothing initially. This is the single largest structural change from the Flutter app and deserves its own PLAN.md line item.
- Header actions become per-page `header_end()` vectors, so "Refresh" on every page is no longer a copy-paste per screen.

---

## 2. Per-screen mapping

Notation: all paths under `cosmic::widget::` unless noted. `Application`/`Core` are `cosmic::app::{Application, Core}`.

| # | Flutter screen | libcosmic target | Concrete widgets / APIs (verified) |
|---|---|---|---|
| 1 | `home_screen.dart` shell | `Application` impl | `nav_model()`, `on_nav_select()`, `header_start/center/end()`, `Core::is_condensed()`, `Core::nav_bar_toggle()`; `nav_bar::Model::insert().icon().text().data()`, `activate()` |
| 2 | `dashboard_page.dart` | Page 0 view | `widget::header_bar()` for the title row; `widget::settings::section().title()` for "SYSTEM STATUS"/"CONTAINERS"/"QUICK ACTIONS"; `cards::Cards` or `widget::flex_row` for the 3 stat tiles; `settings::item::builder("…").control(widget::text(…))` for label/value rows; `widget::warning()` for the inline error strip; `widget::progress_bar::indeterminate_circular()` for task spinners; `widget::list::list_column()` for container rows |
| 3 | `containers_page.dart` | Page 1 view | `widget::list::list_column()` + `list::button()` per container; `widget::layer_container` / `cards::cards` for cards; FAB → `header_end()` primary button (COSMIC has **no FAB**; see §3.5) |
| 4 | `widgets/container_card.dart` | Shared `container_row(app, container) -> Element` helper | `list::button(content).on_press()`, `widget::icon::from_name()` for distro icons, `widget::text::body/caption`, `widget::button::icon` for terminal/overflow; overflow menu → `widget::context_menu` or `widget::popover` (see §3.2) |
| 5 | `container_details_page.dart` | Detail page (nested) | `header_bar` + back; `settings::section()` for "Container Status"/"Quick Actions"/"Danger Zone"; `settings::item::builder().title().description().control()` for the action tiles (this is exactly the COSMIC row-with-title-and-subtitle widget); `widget::button::destructive` for the Danger Zone |
| 6 | `container_terminal_page.dart` | Detail page (nested) | `widget::container` + `widget::text` monospace for the `distrobox enter` command; `widget::selectable_text` (there is a real `selectable_text` widget, a11y-gated); `widget::button::standard` + `widget::icon` for Copy; `widget::warning()` for the not-running banner; `settings::item` rows for ID/Name/Image/Status |
| 7 | `create_container_page.dart` | Nested page, 3 phases | `widget::segmented_control` or a step row for the 3-dot indicator (**note:** there is no libcosmic stepper widget — build from `widget::row` + `widget::container`); image step → `widget::grid` + `widget::text_input::search_input()`; config step → `settings::item::builder().toggler()/checkbox()`; `widget::text_input()`; advanced → `widget::settings::section`; volumes → `list_column` + `widget::dialog` (§3.1); progress step → `widget::progress_bar::determinate_circular/linear` + `widget::scrollable` console |
| 8 | `images_page.dart` | Page 2 view | `text_input::search_input()`; `widget::grid`; empty/error via `widget::text::title3` + `widget::button` (no stock empty-state widget — see §3.4); details → `widget::dialog` |
| 9 | `package_manager_page.dart` | Page 3 view | `widget::tab_bar::horizontal` + `widget::segmented_control` for the 2 tabs; container picker → `widget::dropdown` (real widget: `dropdown::dropdown`, `popup_dropdown`); PM badge → `widget::container`+`text`; package rows → `list_column`; confirms → `widget::dialog` |
| 10 | `updates_page.dart` | Page 4 view | Two `settings::section()`s (RUNNING / STOPPED); per-container card → `settings::item` + `widget::button::suggested`; `widget::progress_bar` inline |
| 11 | `backups_page.dart` | Page 5 view | `tab_bar::horizontal`; snapshot rows → `list_column` + `list::button`; Export/Import/Clone cards → `settings::item::builder().title().description().control(button)`; all five input dialogs → `widget::dialog` + `text_input`; paths → portal file chooser via `cosmic::dialog::file_chooser` (feature `xdg-portal`) |
| 12 | `activity_logs_page.dart` | Page 6 view | `text_input::search_input()`; filter chips → `widget::segmented_control` (single-select) or `widget::radio`; stats bar → `widget::row` of `text`; timeline → `list_column`, with `widget::progress_bar::indeterminate_circular` for in-flight items |
| 13 | `settings_page.dart` | Page 7 view | **`widget::settings::section()` + `settings::item::builder()`** — this is the canonical COSMIC settings layout and the best-matching widget surface in the whole library; `item::builder(..).toggler()`, `.checkbox()`, `.control(button)`; `widget::about()` (feature `about`) replaces the hand-rolled About card, giving artists/developers/links sections for free |
| 14 | `apps_page.dart` | Nested page | `search_input`; `widget::grid` + `widget::toggler()` (export switch) per app; manual binary export → `widget::dialog` + `text_input` |
| 15 | `disk_usage_page.dart` | **Orphan — do not port as-is** | §4.4 |
| 16 | `task_page.dart` | **Orphan — drop** | §4.4 |
| 17 | Task progress console (×5 copies) | **One shared dialog** | `widget::dialog()` + `widget::scrollable`; §3.1 |
| 18 | Task output bottom sheet | `context_drawer` | `Application::context_drawer()`; §3.2 |

### Widget-name gotchas to hand PLAN.md

- `widget::card` is a **style module only** (`card/mod.rs` is 4 lines: `pub mod style;`). The card *widget* is `widget::cards::cards` / `widget::cards::Cards`. Do not write `widget::card()`.
- `widget::flex_row` is `widget::flex_row::flex_row(children)` (a const fn in the module, `FlexRow::new`); `pub use` is only of the `FlexRow` type. Call it as `widget::flex_row::flex_row` or via the type.
- `widget::settings` exposes `section`, `item`, `flex_item`, `builder`, `view_column` — check `widget::settings::{section, item}` paths.
- `widget::list` re-exports `ListColumn`, `ListButton`, `button`, `list_column` (from `list/list_column.rs`).
- `progress_bar` exports: `indeterminate_circular()`, `indeterminate_linear()`, `determinate_circular(f32)`, `determinate_linear(f32)`, plus the `linear`/`circular` modules.
- `toaster` is the COSMIC toast system (`Toasts::new(on_close)`, `Toast::new(msg).action(..).duration(..)`). This is the replacement for snackbars (§3.3).
- `widget::warning(msg)` + `.on_close(msg)` renders the inline warning banner — direct replacement for the orange/red-tinted `Container` banners.
- `widget::dialog()` gives `.title() .body() .icon() .control() .primary_action() .secondary_action() .tertiary_action() .width() .height()`. Primary/secondary/tertiary map cleanly onto Delete/Cancel, Stop All/Cancel, etc.
- `Application::dialog()` returns a **single** `Option<Element>` — the app owns one modal slot. See §3.1.

---

## 3. Dialog and pattern unification

### 3.1 The five progress dialogs → one

Counted in source: `_UpgradeProgressDialog` (identical class) is defined **four times** — `container_details_page.dart:499`, `updates_page.dart:485`, `package_manager_page.dart` (`_TaskProgressDialog`), `backups_page.dart` (`_TaskProgressDialog`) — plus a fifth, *different* inline console in `create_container_page.dart` (step 2). They disagree with each other:

| | details / updates / pkg / backups | create step 2 |
|---|---|---|
| Auto-scroll | yes (`animateTo(maxScrollExtent)`) | **no** |
| Empty state | **none** (pkg) / `'Starting upgrade...'` (updates) | `'Waiting for output...'` |
| Line colouring | none (all green) | red on `Error`/`error`, primary on `>`/`->` |
| Dismiss | `barrierDismissible: false` | n/a (a page step) |

**Proposal: one `TaskProgress` component** used by both the modal and the create-wizard console, with the union of the good behaviours (auto-scroll **and** empty state **and** severity colouring):

```
widget::dialog()
  .title(task.description)
  .control(
      widget::scrollable(console_lines)
        .id(console_id)              // scrollable::scroll_to for auto-scroll
        .height(Length::Fixed(300.0))
  )
  .secondary_action(widget::button::standard("Cancel").on_press(CancelTask(id)))
  .primary_action(widget::button::suggested("Done").on_press(CloseTask(id)))
```

Auto-scroll: libcosmic/iced has no `addPostFrameCallback`. Use `iced::widget::scrollable::scroll_to(id, offset)` emitted from `update()` **only when the output length changed** — otherwise a per-frame scroll task will fight user scrolling. This is the one behavioural detail worth prototyping early.

Task state must move off the Flutter-ism of *string-matching output text*. In Flutter, `TaskInfo` sets `failed` on any line containing "error"/"failed", and the UI *additionally* re-derives status by sniffing the last line for "completed"/"failed" (`create_container_page.dart`), and `isTaskRunning` is **polled every 500 ms** in `package_manager_page.dart` and `backups_page.dart`. Proposal: the Rust task model (`rust/src/models/task.rs`) should carry a real `TaskState` enum, pushed to the UI through a `Subscription`, eliminating both the string-sniffing and the polling.

### 3.2 Bottom sheets → `Application::context_drawer()`

Flutter uses `showModalBottomSheet` in exactly two places: `container_card.dart` (quick-actions menu: Details / Open Terminal / Stop Container / Upgrade Container / Delete Container) and `activity_logs_page.dart` (full task output, `DraggableScrollableSheet` at 0.7 initial, 0.5–0.95 range).

libcosmic's `ContextDrawer` is the direct equivalent — `Application::context_drawer()` returns it, and `Core::set_show_context(bool)` drives it. It has `.title() .actions() .header() .footer()`.

- **Container quick actions** → context drawer with a `settings::section` of `list::button` rows, triggered by the card's overflow button. Delete row uses the destructive button class.
- **Task output** → context drawer whose content is the shared console component from §3.1. Note `ContextDrawer` has no built-in drag-resize; it takes a width from `Core::context_width(has_nav)`. Accept that, or use `widget::layer_container` inside `popover` if free-floating resize is truly required (not recommended).

**Alternative for the container card menu:** `widget::context_menu` (right-click/overflow popup) or `widget::popover`. Recommend: **context drawer for the ⋮ button** (discoverable, matches COSMIC patterns in Files/Store), and additionally wire `context_menu` to right-click since it is nearly free. Long-press as a trigger should be dropped — it is a touch idiom with no meaning on desktop.

### 3.3 Confirm dialogs → one pattern, one helper

There are **10** confirm/destructive dialogs across the app — every one a hand-rolled `AlertDialog` (`AlertDialog` appears **23** times across `lib/`; the other 13 are input dialogs and the 4 progress dialogs) — each pairing `TextButton("Cancel")` + `FilledButton(red/orange, verb)`. Their copy is inconsistent even for the same action (`container_card.dart` says "…cannot be undone." while `container_details_page.dart` says "…cannot be undone and all container data will be lost.").

**Proposal:** one helper that enforces COSMIC conventions.

```
confirm(state, ConfirmSpec { title, body, confirm_label, destructive: bool, action })
  -> widget::dialog()
       .title(spec.title)
       .body(spec.body)
       .icon(warning_icon_if_destructive)
       .secondary_action(widget::button::standard("Cancel"))
       .primary_action(if destructive { widget::button::destructive(label) }
                       else        { widget::button::suggested(label) })
```

COSMIC convention notes: destructive actions use the destructive button class (libcosmic's theme has this), the dialog carries a warning icon for danger, and the *body* carries the consequence text — the title never contains the verb-only instruction plus a paragraph. This also fixes the copy divergence by making the strings shared.

**Unify the snackbar/feedback policy** (§3.4) — Flutter's is inconsistent: `package_manager_page.dart` has **zero** snackbars (failures are silent: `installPackage`/`removePackage`/`upgradeContainer` returning `null` shows nothing), `create_container_page.dart` has 3, `backups_page.dart` has 4 pairs but restore/export/import/clone produce **no** snackbar on `null` taskId (silent failure). Every mutation in the new app should report through the toaster.

### 3.4 Empty / error / loading states

libcosmic ships **no** generic empty-state or error-boundary widget (verified: grepping `placeholder` in `src/` returns only `text_input` placeholder colours). Two options: build one shared `empty_state(icon, title, body, action?)` helper, or rely on `widget::warning` for errors and plain centred text for empties. **Recommendation: build the one helper** — the Flutter app repeats this pattern ~20 times (`Icon(size:64) → gap → title → gap → body → optional button`) and the duplication is the reason the copies drifted (e.g. `updates_page` has a "Distrobox Not Found" view with a Refresh button, `containers_page` has one without, `package_manager_page` has one with no button at all).

**Loading:** standardize on centred `widget::progress_bar::indeterminate_circular()`. Flutter currently shows a *full-page* spinner whenever `appState.isLoading` is true, including on refresh while containers are already displayed — that is a regression to avoid; COSMIC convention is to keep content and show progress inline.

**The three global gates** (distrobox-missing, environment-blocked, load error) are currently reimplemented per page with drifting copy. Implement **once** at the shell level (wrap `view()`), so every page inherits it. `apps_page`, `images_page`, `activity_logs_page`, and `settings_page` currently have no distrobox-missing gate at all.

### 3.5 Pattern replacements that have no libcosmic counterpart

| Flutter pattern | Decision |
|---|---|
| `FloatingActionButton.extended` (`containers_page`, `backups_page`) | **Drop.** COSMIC has no FAB. Use a `suggested` button in `header_end()` and/or an action row at the top of the page. Note `backups_page`'s FAB is rendered even in the loading / not-installed / no-containers states where it can only fail with a snackbar — that bug disappears with the move. |
| `RefreshIndicator` (pull-to-refresh, dashboard only) | **Drop.** Touch idiom; conflicts with `widget::scrollable`. Keep the header Refresh button. |
| `Switch` / `SwitchListTile` | `widget::toggler()` — `settings::item::builder(..).toggler(value, msg)` |
| `FilterChip` row | `widget::segmented_control` (single-select) or `widget::radio` |
| `TabBar` + `TabBarView` | `widget::tab_bar::horizontal` + `widget::segmented_control` |
| `IconButton.filledTonal` | `widget::button::icon(icon)` with the appropriate class |
| `Tooltip` | `widget::tooltip(content, tooltip, Position)` |
| `Opacity(opacity: 0.7)` for stopped containers | Use theme text/colour classes rather than a blanket opacity (opacity hurts contrast and is invisible to a11y) |
| `Colors.green/orange/blue` hard-coded status colours | Theme colours from `cosmic_theme` (success/warning/destructive) so light/dark/high-contrast all work |
| `Divider()` / `VerticalDivider()` | `widget::divider::horizontal::{default,light,heavy}()` / `divider::vertical::{…}()` |

### 3.6 Kill the duplication that the 9-file icon/colour copy created

`_getDistroIcon` is duplicated in **9 files** with **three divergent variants** (`grep -rl _getDistroIcon lib/` → 9 files, 19 occurrences). Six files — `create_container_page`, `dashboard_page`, `container_card`, `container_details_page`, `container_terminal_page`, `updates_page` — carry the 8-outcome version (ubuntu/fedora/arch/debian/alpine/centos+rocky/opensuse+suse, else generic); `images_page` carries a 10-outcome **superset** (adds gentoo, void); `package_manager_page` and `backups_page` carry only the 6-outcome version (ubuntu/fedora/arch/debian/alpine, else generic) — missing centos/rocky/opensuse/suse, so those fall through to the generic icon. `_getDistroColor` is duplicated in 3 files (`create_container_page`, `images_page`, `updates_page`), each with the same 7 conditions. Status colour/label is **2 named helpers** (`_getStatusColor`/`_getStatusText` in `dashboard_page`, `container_card`) plus **2 files that re-inline the same switch** as the `statusColor`/`statusText` getters (`container_details_page`, `container_terminal_page`) — the logic is duplicated across 4 files, but there are only 2 named helpers.

**Proposal:** one `theme/icons.rs` (or `distro.rs`) in the new app with `distro_icon(image) -> Icon`, `distro_colour(image) -> Color`, `status_colour(Status) -> Color`, `status_label(Status) -> String`. `package_manager_page`/`backups_page` inheriting the full table is a *behaviour fix*, not just a refactor — call it out in the parity notes as an intended change.

Distro icons: COSMIC uses named icon-theme lookups (`widget::icon::from_name("distro-icon")`), which is strictly better than mapping to `Icons.*` — but the names must exist in the icon theme. Verify availability at plan time; fall back to a bundled set + `widget::icon::icon(Handle)` if the theme lacks them.

---

## 4. Decisions needed

### 4.1 Start container — implement first, it gates three screens

**Status:** missing from the entire stack (no `api.rs` function, no `Distrobox` method).

Three shipped screens tell the user to start a container and give them no way to do it:

- `container_terminal_page.dart:237` — "Container is not running. Start the container to access the terminal."
- `updates_page.dart:470` — "Start container to enable upgrades" (stopped section, dimmed)
- `package_manager_page.dart` — "Container is not running. Start it to manage packages." + a whole "Container Not Running" tab state, with Install/Upgrade/search disabled

Plus the container-card bottom sheet, the details page, and the dashboard only ever offer *Stop*.

**Recommendation: P1 (new capability — no Flutter equivalent to preserve) but Blocking severity.** The caller's brief labels this "P1-must", which is the tension: it is not required for *literal* parity (Flutter has no start either, so a 1:1 port could ship without it and these three screens stay as broken as they are today), but it is required for the port to be *usable*, and the screens' existing copy already promises it. **Sequencing: implement `start_container` in `rust/src/api.rs` + `Distrobox` before the affected pages are ported**, otherwise the migration ships three dead-end screens and the copy debt grows.

Design: `start_container(name) -> TaskId` (streaming, like upgrade) since `distrobox create`-based start can be slow; surface as Start/Restart on the container card, details page, and — importantly — as the CTA inside those three warning banners, replacing the sentence with a button.

### 4.2 The dead ends

| Dead end | Where | Decision |
|---|---|---|
| `preselectedImage` constructor arg, **never passed by any caller** | `create_container_page.dart` | **Wire it.** The plumbing already exists; the wizard already handles it in `initState`. This is free parity. |
| "Create Container" in the image-details dialog → snackbar "Navigate to Create Container with $name" + `SnackBarAction('Go')` whose `onPressed` is `// TODO: Navigate…` | `images_page.dart:370-388` | **Implement.** One call: push the create wizard with `preselectedImage: imageUrl`. |
| Custom-image URL field → snackbar `'Use "…" in Create Container'`, arrow button does nothing else | `images_page.dart:190-209` | **Implement** with `preselectedImage`. The field is already the wizard's `_customImageController` equivalent. |
| Quick action "New Container" — `onPressed: () { // Navigate to create container }` | `dashboard_page.dart:587` | **Implement.** Push the create wizard. |
| Quick action "View all" — `onPressed: () { // This would switch to containers tab in real app }` | `dashboard_page.dart:156-158` | **Implement.** It becomes trivial: `nav_model.activate(containers_id)`. Better in libcosmic than Flutter. |
| Quick action "Upgrade All" → snackbar 'Navigate to Updates page to upgrade' | `dashboard_page.dart:596-603` | **Implement for real.** Call the upgrade-all path (confirm → for each running container) directly. A redirect snackbar is strictly worse than just doing it. |
| Settings "Upgrade All Containers" → snackbar 'Go to Updates page to upgrade containers' | `settings_page.dart:170-179` | **Implement for real**, same as above. |
| Folder icon as `suffixIcon` on Home Directory — a bare `Icon`, no `onPressed`, looks like a native file picker | `create_container_page.dart` | **Implement** with the portal (`cosmic::dialog::file_chooser::open`, feature `xdg-portal`). This is the only "file picker" affordance in the app and it is a lie. |
| "Stop All" / "Delete All" / per-container actions with `null` taskId → **no feedback at all** | `package_manager_page`, `backups_page` | **Implement.** Route every mutation result to the toaster. |
| Silent validation `return` (dialog stays open, no message): Add Volume with empty paths; Create Snapshot / Restore / Export / Import with empty fields | `create_container_page`, `backups_page` | **Fix.** Use `widget::text_input`'s error display or disable the primary action until valid. A button that does nothing is the worst possible feedback. |
| Install button uses the raw search-box text, **no empty guard** — produces a confirm dialog reading `Install "" in "ubuntu"?` | `package_manager_page` | **Fix.** Disable when empty. |
| `Clone` validation uses `text.isEmpty` **without `.trim()`**, unlike its four siblings | `backups_page` | **Fix** to `.trim()` for consistency. |

### 4.3 Native file picker — P0 for Backups

`backups_page`'s Export and Import take free-typed filesystem paths (prefilled `/tmp/<name>-export.tar`). Typing a path is a real usability failure and a correctness risk (no existence/overwrite check). libcosmic has this solved: `cosmic::dialog::file_chooser::{open, save}` (`src/dialog/file_chooser/open.rs`, `save.rs`) behind the **`xdg-portal`** cargo feature (not in default — must be enabled). The Home Directory field also benefits.

**Recommendation: P0.** Without it the Backups screen's two headline features require the user to know and type absolute paths.

### 4.4 The two orphans

**`disk_usage_page.dart` — do not port as a mock; build it for real or not at all.**
The page is unreachable (no import, no route) and every number is hard-coded: `'45.2 GB Total'`, `'30 GB Images'`, `'15.2 GB Data'`, `'Containers (8)'`, and four fake rows (`'Fedora 39', '12.4 GB used'`, `'Arch Linux', '8.1 GB used'`, …) with a hard-coded `value: 0.66` donut. Its buttons are dead (`onPressed: () {}` on the menu, refresh, "View All"; "Optimize Now" does nothing).

**Important correction to the brief's suggestion:** "Real disk usage via the backend stats API" is **not** available. `ContainerStats` (verified fields: `cpu_percent`, `memory_usage`, `memory_limit`, `memory_percent`, `network_io`, `block_io`) carries **no disk figure**. Delivering this page for real requires a **new backend capability** — e.g. `podman system df` / `docker system df` for the image/container/volume breakdown, plus per-container size (which podman does not report cheaply; likely `du` over the overlay dir, or `podman ps --size`).

**Recommendation: P1, and only with the new backend work scoped.** Proposal: add `disk_usage()` to the backend returning a real struct; port the page as a genuine page and add it as a nav destination or (better) a section on Settings, since an unreachable page in Flutter means it never earned a nav slot. If the backend work is not funded, **drop it entirely** rather than ship the mock — a page of plausible fake numbers is worse than no page.

**`task_page.dart` — drop.** 10 lines, `Center(child: Text('Task Manager (Not Implemented)'))`, unreachable. `activity_logs_page` already is the task UI. No known ask for a separate task manager.

### 4.5 Missing features — classification

The taxonomy requested is P0-parity (exists in Flutter, must survive) / P1-gapfill (new) / drop. Note that §4.1's start-container is P1-by-taxonomy but Blocking-by-severity; that mismatch is intentional and should be preserved in PLAN.md rather than flattened.

| Feature | Class | Rationale / libcosmic approach |
|---|---|---|
| Start / restart container | **P1 — blocking** | §4.1. Gates 3 screens' existing copy. New backend fn + buttons + banner CTAs. |
| Native file picker | **P0** | §4.3. `xdg-portal` feature + `cosmic::dialog::file_chooser`. Backups Export/Import and the Home Directory field are unusable without it. |
| Local image list / pull / delete | **P1** | Genuinely absent, at the **backend** level: `list_images()` is `distrobox create --compatibility`, i.e. the distro catalogue, not local images. A real image manager needs new `podman images` / `pull` / `rmi` plumbing. Note the Flutter page is *named* "Images" but shows a distro picker — so there is no P0 item here, only a naming problem plus optional new capability. Recommend: rename the page's job to "Distro Catalogue" for parity, and scope image management separately as P1. |
| Container search / filter | **P1** | Absent from `containers_page` (which has no search at all, while images/apps/packages/logs all do). `text_input::search_input()` + a filter over the in-memory list — cheap, high value once you have many containers. |
| Persistent task history | **P1** | `TaskInfo` lives in `AppStateProvider._activeTasks` only; `_maxOutputLines = 500` truncates; everything is lost on exit. `activity_logs_page` shows "history" that is really the live session. Proposal: persist task records via `cosmic_config` (libcosmic's own config store, `Core::watch_config`, `CosmicConfigEntry`) or a small on-disk log. |
| Keyboard shortcuts | **P1** | Free-ish; §5.1. `keyboard_nav::subscription()` + `Core::set_keyboard_nav(true)`. |
| i18n | **P1** | §5.2. libcosmic has the whole pipeline; the cost is extracting ~600 strings, not plumbing. |
| a11y semantics | **P0** | §5.3. The `a11y` feature is on by default; the work is choosing widget APIs that carry labels (unlabelled `button::icon` and bare `Icon` need labels). Flutter has no semantics either, so this is *net-new*, but shipping a desktop app with no AT support is not acceptable — treat as P0 alongside the other quality gates. |
| Text scaling | **P0 (free)** | §5.4. Inherited from COSMIC's global scale via `Settings::default_text_size`/`scale_factor`. Do not build an in-app slider. |
| Container clone options | **P1** | `backups_page`'s Clone dialog hard-codes `init: false, nvidia: false, homePath: null, volumes: []` while the details-page clone dialog exposes name + home dir only. Unify into the create wizard's config step (reuse), which is better than either. |
| **Theme toggle** | **DROP** | §4.6. |
| Real terminal emulation | **DROP (for this migration)** | `container_terminal_page` shows a command to copy and says "Full terminal emulation is planned for a future release". The README claims "a built-in terminal emulator so you can drop into any container with a single click" — **the README is wrong** and should be corrected. A VTE/PTY widget is a large, separate work item. Keep the copy-command page (it is honest and useful) and keep the caveat text; do not scope a terminal into the UI migration. |
| "Optimize Now" (disk page) | **DROP** | Belongs to the orphan; no backend capability, no definition of what it optimises. |

### 4.6 Theme toggle — drop, with justification

`main.dart` hard-codes `themeMode: ThemeMode.system` and ships **no** theme UI, so removing a toggle removes nothing. In libcosmic the light/dark decision is not the app's to make:

- `Core` reads the system mode itself: `Core::system_theme_mode() -> ThemeMode`, backed by `ThemeMode::config()` from cosmic-config, and `Core::system_is_dark()`.
- `theme::active()`, `theme::system_preference()`, `theme::system_dark()/system_light()` resolve the live theme.
- The app is *notified* of changes via `Application::system_theme_update(keys, theme)` and `system_theme_mode_update(keys, mode)` — callbacks the app implements if it needs to react, not controls it owns.
- The **COSMIC Settings app** owns the user's light/dark/high-contrast choice, and `theme::is_high_contrast()` exists too.

Adding a per-app toggle would fight the desktop: the app would either have to persist an override (inconsistent with every other COSMIC app) or be overridden by the next system change. **Drop it, and inherit theme + accent + high-contrast + radius for free.** Corollary for the checklist: this is also why the hard-coded `Colors.green/orange/blue/red` status palette must be replaced with `cosmic_theme` roles — those literals are the only thing preventing the app from looking correct in all COSMIC themes.

---

## 5. Accessibility and i18n

### 5.1 Keyboard navigation and shortcuts

**Focus traversal is already solved and is off by default per-app.** `Core::set_keyboard_nav(true)` (with `Core::keyboard_nav()`) enables it; `cosmic::keyboard_nav::subscription()` yields:

```
Action::{ Escape, FocusNext, FocusPrevious, Fullscreen, Search }
```

bound to `Tab` / `Shift+Tab`, `Escape`, `F11`, and `Ctrl+F` respectively (verified in `keyboard_nav.rs`).

Plan:
1. In `init`, call `core.set_keyboard_nav(true)` and subscribe to `keyboard_nav::subscription()`.
2. Handle `Action::Escape` → close the active dialog/context drawer (currently Flutter relies on `barrierDismissible`; there is no Escape handling).
3. Handle `Action::FocusPrevious/FocusNext` — libcosmic routes focus itself; just never swallow Tab.
4. Handle `Action::Search` → focus the active page's search field. This makes the existing search inputs (images, logs, apps, packages) reachable without the mouse, and is the reason `Ctrl+F` is worth wiring even though no Flutter page had it.
5. Add app-specific accelerators as page-scoped commands, kept deliberately small and non-conflicting:
   - `Ctrl+R` — refresh the current page (every page currently has a Refresh button; this is the highest-value addition).
   - `Ctrl+N` — new container (Containers page).
   - `Enter` / `Space` on focused list rows — open details (comes free if rows are real `button`s rather than `mouse_area`).
6. Every `button::icon` (and every bare `icon()`) needs an accessible label — icon-only buttons are the dominant control in this app (refresh, copy, stop, overflow, delete, folder) and an unlabelled icon button is invisible to a screen reader. Use `widget::tooltip` for the sighted hint **and** confirm the button carries its label into the a11y tree; the widget's `a11y_nodes` impls are `#[cfg(feature = "a11y")]`-gated, so verify with the feature on.

### 5.2 i18n

**COSMIC apps use Fluent, and libcosmic ships the pipeline.** Verified in-tree: `src/localize.rs` (a `FluentLanguageLoader` static, `fl!` / `flc!` macros wrapping `i18n_embed_fl`), `i18n.toml` (`fallback_language = "en"`, `[fluent] assets_dir = "i18n"`), and an `i18n/` directory with ~50 locales. `LANGUAGE_LOADER` + `localize()` + `localizer()` are provided; `localize()` calls `i18n_embed::DesktopLanguageRequester::requested_languages()`.

**Stance:** adopt Fluent with `fl!`, `assets_dir = "i18n"`, and follow whatever `justfile`/`i18n.toml` conventions the parent project sets. Notes for PLAN.md:

- The app has **no** i18n today (`intl` is used only for `DateFormat` in `activity_logs_page`, not for translation), so this is net-new, but it is *cheap plumbing* — the cost is extracting ~600 user-visible strings from 16 screens.
- **Do the extraction during the port, not after.** Every string touched by the port should land in the `.ftl` file as it moves. A "i18n phase 2" would mean re-touching all 16 screens.
- libcosmic's own strings (About widget: "links", "developers", "translators", "license") are already localised, which is another reason to use `widget::about()` over a hand-rolled About card.
- Relative-time formatting in `activity_logs_page` (`'Just now'`, `'${diff.inMinutes}m ago'`, `DateFormat('MMM d, yyyy')`) must become Fluent messages with plural/select forms — do not hard-code English strings into the Rust port.

### 5.3 Screen reader / AT

The `a11y` cargo feature is **in default** and forwards to `iced/a11y` + `iced_accessibility`; widgets implement `a11y_nodes(...)` under `#[cfg(feature = "a11y")]` (verified across `header_bar`, `selectable_text`, `popover`, `layer_container`, `id_container`, `autosize`, `aspect_ratio`, `dnd_source`, `text_editor`, `applet/column`). So the tree is produced essentially for free *for widgets that have an accessible role and label*.

The real work, in priority order:
1. **Label every icon-only control** (see §5.1.6) — highest impact, touches nearly every screen.
2. **Use real interactive widgets rather than `mouse_area`/`container`+`on_press`** so that rows/buttons are focusable and announced. The Flutter app's tappable `Card`+`InkWell` rows should become `list::button`, `button::standard`, etc.
3. **Do not encode meaning in colour alone.** Status is currently a coloured dot plus coloured text; the text already carries the state ("Running 2 hours"), so keep it and make sure the colour is decorative. The `Opacity(0.7)` treatment for stopped containers in `updates_page` should be replaced with a text/colour class.
4. **Announce async completions** — task completion and errors currently appear only as snackbars or silently. Toasts (`toaster`) plus a live-region-equivalent announcement is the goal; verify what `iced_accessibility` offers for announcements at plan time (not verified in this pass).
5. Verify with a real screen reader (Orca) once the shell runs; treat as an acceptance gate, not a checkbox.

### 5.4 Text scaling

Handled by the desktop, not the app. `Settings::default_text_size(f32)` (default 14.0) and `Settings::scale_factor` (read from the `COSMIC_SCALE` env var at startup, verified in `app/settings.rs`) are the app's only inputs; the COSMIC compositor/settings own the user's scale. **Do not add an in-app text-size slider** — it would be the only COSMIC app with one. Requirements instead: use theme text styles (`widget::text::title3`, `body`, `caption`) rather than hard-coded pixel sizes, and avoid fixed heights that clip at large scale (the Flutter app has several: 80px action buttons in `container_terminal_page`, 48px icon boxes, a 300px console in the progress dialogs, 100px label columns in `_buildDetailRow`).

---

## 6. Full parity checklist

Status key: **exists** (present and working in Flutter) · **dead** (present but does nothing / unreachable / mock) · **missing** (absent) · **dup** (duplicated N×, needs consolidation) · **drop**.

### 6.1 Shell and navigation (8)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 1 | 8 nav destinations, fixed order | exists | `nav_bar::Model` + `insert().icon().text().data()`, `activate_position(0)` |
| 2 | Nav rail ≥600px, extended ≥1000px | exists | **Accept COSMIC condensed+toggle**; drop both breakpoints (§1) |
| 3 | Bottom nav bar <600px | exists | **Drop** — no COSMIC equivalent |
| 4 | Nav labels (divergent sets: Home/Boxes/Pkgs/Logs vs Dashboard/Containers/…) | exists | Pick the rail labels; one set |
| 5 | Per-destination icons, selected/unselected variants | exists | `Entity::icon()`; use the theme's symbolic icons |
| 6 | Page switching | exists | `on_nav_select()` → `nav_model.activate(id)` |
| 7 | Tab state preserved on switch | missing | Comes free — state lives on `App`, not the page |
| 8 | Global gates (distrobox-missing, env-blocked) | dup (per-page, drifting copy) | Implement once at shell level around `view()` |

### 6.2 Dashboard (26)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 9 | Header title + icon | exists | window title / `header_bar()` |
| 10 | Refresh action | exists | `header_end()` `button::icon` + `Ctrl+R` |
| 11 | Full-page loading spinner | exists | `progress_bar::indeterminate_circular()` — **but keep content during refresh** (§3.4) |
| 12 | Environment-blocked view (icon, title, message, "Check Again") | exists | Shared gate view (§6.1.8); `empty_state` helper |
| 13 | Distrobox-not-found view (title, copy, "Check Again") | exists | Shared gate view |
| 14 | System status card (SYSTEM STATUS, healthy/degraded headline, running-of-total) | exists | `settings::section` + `text::title4`/`body` |
| 15 | Inline error strip inside status card | exists | `widget::warning(msg)` (has `.on_close`) |
| 16 | Stat card: Total Containers | exists | `flex_row`/`cards` of label+value |
| 17 | Stat card: Running | exists | as above, theme success colour |
| 18 | Stat card: Stopped | exists | as above, theme warning colour |
| 19 | Active Tasks section (conditional, only when non-empty) | exists | `settings::section` + conditional `.add_maybe()` |
| 20 | Task card: spinner, description, "In progress…"/"Completed" | exists | inline `progress_bar` + shared task component |
| 21 | Task card: cancel button | exists | `button::icon("process-stop-symbolic")` + label |
| 22 | Containers section header | exists | `settings::section().title()` |
| 23 | "View all" → containers tab | **dead** (empty callback) | Implement: `nav_model.activate(containers_id)` |
| 24 | Container row: distro icon, name, status dot, status text, chevron | exists | shared `container_row` helper (§6.3) |
| 25 | Container row: inline Stop when running | exists | `button::icon` + confirm |
| 26 | Container row: tap → details | exists | `list::button().on_press()` |
| 27 | Empty-containers card | exists | `empty_state` helper (§3.4) |
| 28 | Quick action: New Container | **dead** (empty callback) | Implement: push create wizard |
| 29 | Quick action: Upgrade All | **dead** (snackbar redirect) | Implement for real (§4.2) |
| 30 | Quick action: Stop All + confirm | exists | `confirm()` helper (§3.3), destructive class |
| 31 | Quick action: Refresh | exists | same as #10 |
| 32 | Pull-to-refresh | exists | **Drop** (touch idiom) |
| 33 | Distro icon mapping (8-outcome, 3 variants) | dup (9 files, 3 divergent variants) | One `distro_icon()` (§3.6) |
| 34 | Status colour/label mapping | dup (4 files: 2 named helpers + 2 inline copies) | One `status_colour()`/`status_label()` |

### 6.3 Containers list + card (12)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 35 | Header "Containers" + refresh | exists | `header_end()` |
| 36 | Loading / not-installed / error / empty states | dup | Shared gate + `empty_state`; **note** the error state is bare `Text('Error: …')` — improve |
| 37 | Container card list | exists | `list_column` + `list::button` |
| 38 | Card: distro icon with status dot overlay | exists | `widget::icon` + `container` badge |
| 39 | Card: name / status text / image path | exists | `text::body` + `text::caption` |
| 40 | Card: inline "Open Terminal" when running | exists | `button::icon`, labelled |
| 41 | Card: ⋮ → quick actions | exists (bottom sheet) | `context_drawer` (§3.2) |
| 42 | Card: long-press → quick actions | exists | **Drop** (touch idiom); wire right-click `context_menu` instead |
| 43 | Card: tap → details | exists | `list::button().on_press()` |
| 44 | FAB "New Container" → wizard | exists | **Drop FAB** → `header_end()` suggested button (§3.5) |
| 45 | Status dot colour/label | dup | shared helper |
| 46 | **Start / restart container** | **missing** | §4.1 — blocking; new backend fn |

### 6.4 Container quick-actions sheet (6)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 47 | "Details" row | exists | `list::button` in drawer |
| 48 | "Open Terminal" row, disabled when stopped | exists | `.on_press_maybe(Option)` |
| 49 | "Stop Container" row (running only) | exists | conditional row + confirm |
| 50 | "Upgrade Container" row | exists | shared task dialog (§3.1) |
| 51 | "Delete Container" row → confirm | exists | `confirm()` destructive |
| 52 | Delete confirm dialog copy | **dup/inconsistent** vs details page | Shared string via `confirm()` |

### 6.5 Container details (13)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 53 | Header: name + back | exists | nested page + `header_start()` back button |
| 54 | Header action: Open Terminal (running only) | exists | `header_end()` conditional |
| 55 | Header action: Refresh | exists | `header_end()` |
| 56 | Hero card: icon, name, image URL chip | exists | `settings::section` + `container` chip |
| 57 | Image URL copy → clipboard + snackbar | exists | `iced::clipboard` + toaster |
| 58 | Status card: status icon, dot, label, container ID | exists | `settings::item` rows |
| 59 | Status card: Stop button (running only) | exists | `button::standard` + confirm |
| 60 | Tile: Upgrade Container + progress dialog + inline spinner | exists | shared task component |
| 61 | Tile: Applications → Apps page | exists | nested page push |
| 62 | Tile: Clone Container → dialog (name `-clone`, home dir, error text, spinner) | exists | `dialog` + `text_input`; **unify with backups clone** (§4.5) |
| 63 | Tile: Open Terminal, disabled when stopped | exists | `.on_press_maybe()` |
| 64 | Danger Zone: Delete + confirm | exists | destructive class + `confirm()` |
| 65 | **Missing: start/restart** | **missing** | §4.1 |

### 6.6 Container terminal page (12)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 66 | Header: name + refresh | exists | `header_bar` |
| 67 | Info header: icon, name, status pill, monospace image | exists | `row` + `text`/`selectable_text` |
| 68 | Not-running warning banner | **dead** ("Start the container…" — no start action) | `widget::warning` + **CTA button** once §4.1 lands |
| 69 | `distrobox enter` command display (selectable, monospace) | exists | `widget::selectable_text` + monospace `text` |
| 70 | Copy icon button + snackbar | exists | `button::icon` (labelled) + clipboard + toaster |
| 71 | "Copy Command" filled button | exists | `button::suggested` |
| 72 | Quick action: Copy Command (running only) | exists | `button` in a row |
| 73 | Quick action: Upgrade Packages → snackbar only | exists (weak) | Route to shared task dialog instead of a bare snackbar |
| 74 | Quick action: Stop Container (running only) | exists | + confirm |
| 75 | Details tile: ID / Name / Image / Status | exists | `settings::item` rows |
| 76 | Help text: no real terminal yet | exists | `text::caption`; **fix README's false "terminal emulator" claim** |
| 77 | Real terminal emulation | **missing** | **Drop from this migration** (§4.5) |

### 6.7 Create wizard (19)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 78 | 3-step indicator (dots, animated) | exists | No stock stepper — build from `row`+`container` (§2) |
| 79 | Step 0: image grid w/ selection + check mark | exists | `widget::grid` + `container` border classes |
| 80 | Step 0: live search filter | exists | `text_input::search_input()` |
| 81 | Step 0: custom image URL field | exists | `text_input` |
| 82 | Step 0: Cancel / Next + "Please select an image" | exists | `button::standard`/`suggested`; inline error not snackbar |
| 83 | Step 1: selected-image preview card | exists | `settings::section` |
| 84 | Step 1: Container Name + auto-default name | exists | `text_input` |
| 85 | Step 1: Init System toggle | exists | `settings::item::builder().toggler()` |
| 86 | Step 1: NVIDIA toggle | exists | as above |
| 87 | Step 1: Advanced expansion | exists | `settings::section` (always-expanded) or keep expandable |
| 88 | Step 1: Home Directory field w/ folder suffix | **dead** (icon is not a button) | portal file chooser (§4.2); **re-scoped per D16** — the portal exposes no `directory()`/`file_name()`, so this cannot prefill |
| 89 | Step 1: Volume mounts list + add/remove | exists | `list_column` + `dialog` |
| 90 | Add Volume dialog (host, container, read-only) | exists (silent validation) | `dialog` + `text_input` + `checkbox`; **fix silent return** |
| 91 | Step 1: Back / Create + name validation | exists | `button::standard`/`suggested` |
| 92 | Step 2: progress indicator w/ running/success/fail states | exists | `progress_bar::determinate_circular` + status text |
| 93 | Step 2: live console (coloured, no auto-scroll, empty state) | exists | shared task component (§3.1) |
| 94 | Step 2: Cancel / Done / Close | exists | shared task component |
| 95 | `preselectedImage` arg | **dead** (never passed) | Wire from images page + dashboard (§4.2) |
| 96 | Name validation (empty check only) | exists | Rust `CreateArgName::new` already enforces `[a-zA-Z0-9][a-zA-Z0-9_.-]*` — **surface that error in the UI**; Flutter never calls it before create |

### 6.8 Images page (9)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 97 | Header "Images" + refresh | exists | `header_end()` |
| 98 | Headline + subtitle | exists | `text::title2` + `text::body` |
| 99 | Live search filter | exists | `search_input()` |
| 100 | Loading / error+Retry / empty (none & no-match) | exists | shared `empty_state` |
| 101 | Image grid cards (2-col, icon, name, tag, + button) | exists | `widget::grid` |
| 102 | Image details dialog (Tag, Image URL, Close, Create Container) | **dead** (snackbar + TODO) | `dialog`; wire `preselectedImage` (§4.2) |
| 103 | Custom image URL field + arrow button | **dead** (snackbar redirect) | Wire to wizard (`preselectedImage`) |
| 104 | Page semantics: distro *catalogue*, not local images | exists (mislabelled) | Rename; note `list_images()` = `distrobox create --compatibility` |
| 105 | Local image management (list / pull / delete) | **missing** | P1, needs new backend plumbing (§4.5) |

### 6.9 Package manager (17)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 106 | Header + refresh (no tooltip on refresh here) | exists | `header_end()` |
| 107 | 2 tabs (Installed / Search w/ dynamic label) | exists | `tab_bar::horizontal` + `segmented_control` |
| 108 | Loading / not-installed / no-containers gates | exists | shared gate |
| 109 | Container dropdown w/ running dots | exists | `widget::dropdown` |
| 110 | Container status pill (Running/Stopped) | exists | `container`+`text` badge |
| 111 | "Container is not running" warning banner | exists | `widget::warning` + start CTA (§4.1) |
| 112 | Search bar, Enter-triggered, disabled when stopped | exists | `search_input()` + `Ctrl+F`; honour disabled state |
| 113 | Clear-search button | exists | `button::icon` |
| 114 | Detected package-manager badge ("Detecting…") | exists | badge from `detectPackageManager()` |
| 115 | Install-from-search-box button | exists (**no empty guard**) | Disable when empty (§4.2) |
| 116 | Upgrade All + confirm | exists | `confirm()` |
| 117 | Installed tab: not-running / loading / error+Retry / empty / count | exists | `empty_state` + `list_column` |
| 118 | Search tab: idle prompt / loading / empty / count | exists | `empty_state` |
| 119 | Package row: icon, name, version chip, description, install/remove w/ tooltip | exists | `settings::item`/`list::button` |
| 120 | Install / Remove / Upgrade confirms | exists | `confirm()` (3 specs) |
| 121 | Task progress dialog | dup (identical to backups) | shared task component |
| 122 | **No failure feedback on `null` taskId** | **dead** | Route to toaster |

### 6.10 Updates (10)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 123 | Header + refresh + "Upgrade All" | exists | `header_end()` two buttons |
| 124 | Loading / not-installed / no-containers gates | exists | shared gate |
| 125 | Summary header (N containers, running/stopped copy) | exists | `text::title3` + `body` |
| 126 | RUNNING CONTAINERS section header | exists | `settings::section` |
| 127 | Running card: distro icon, name, "READY TO UPGRADE"/"UPGRADING…", tag chip, image | exists | `settings::item` + badges |
| 128 | Running card: Upgrade button / inline spinner | exists | `button::suggested` + `progress_bar` |
| 129 | STOPPED CONTAINERS section (dimmed 0.7) | exists | Replace opacity with a colour class |
| 130 | Stopped card: "Start container to enable upgrades" | **dead** | `widget::warning` + start CTA (§4.1) |
| 131 | Upgrade-all confirm + "No running containers" toast | exists | `confirm()` + toaster |
| 132 | Task progress dialog | dup | shared task component |

### 6.11 Backups (19)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 133 | Header + refresh (has tooltip) + 2 tabs w/ icons | exists | `header_bar` + `tab_bar` |
| 134 | FAB "New Snapshot" (rendered in all states) | exists (**bug**) | → `header_end()`; disappears with the move |
| 135 | Loading / not-installed / no-containers gates | exists | shared gate |
| 136 | Container dropdown (no status indicators) | exists | `dropdown`; consider adding status dots for consistency |
| 137 | Snapshots tab: loading / error+Retry | exists | shared `empty_state` |
| 138 | Snapshots tab: empty + "Create First Snapshot" | exists | `empty_state` + action |
| 139 | Snapshot row: name, created, size, Restore, Delete | exists | `list::button` rows + actions |
| 140 | Create Snapshot dialog (prefilled name, helper text, info box) | exists | `dialog` + `text_input`; **fix silent empty-name return** |
| 141 | Delete Snapshot confirm + green/red result toasts | exists | `confirm()` + toaster |
| 142 | Restore dialog (prefilled new name) | exists | `dialog` + `text_input` |
| 143 | Export card + dialog (output path, warning box) | exists | **portal save chooser** (§4.3) |
| 144 | Import card + dialog (archive path, image name) | exists | **portal open chooser** |
| 145 | Clone card (disabled w/o container) + dialog | exists | `dialog`; unify with details-page clone |
| 146 | 5× silent no-op validation on empty fields | **dead** | Inline validation / disabled primary |
| 147 | 4× silent failure when taskId is `null` | **dead** | Route to toaster |
| 148 | Clone dialog validation lacks `.trim()` | **bug** | Fix |
| 149 | Free-text paths, no file picker | **missing** | P0 portal picker (§4.3) |
| 150 | Task progress dialog | dup | shared task component |
| 151 | Undisposed `TextEditingController`s in 5 dialogs | **bug** | Moot — widgets are stateless in the new model |

### 6.12 Activity logs (11)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 152 | Header + refresh + clear-completed | exists | `header_end()` |
| 153 | Search over description + output | exists | `search_input()` + `Ctrl+F` |
| 154 | Filter chips All / Running / Success / Errors | exists | `segmented_control` single-select |
| 155 | Stats bar Total / Running / Completed / Failed | exists | `row` of label/value |
| 156 | Empty states (no activity / no match) | exists | `empty_state` |
| 157 | Timeline row: progress ring or icon, description, relative time, status pill, last-line preview | exists | `list_column` + `progress_bar` + `text::caption` |
| 158 | Output preview → full-output sheet (draggable 0.5–0.95) | exists | `context_drawer` (§3.2) — resize range is lost, accept it |
| 159 | Severity colouring in output (error/warning/ok) | exists | shared console component line classes (§3.1) |
| 160 | Status derived by **string-matching output text** | **fragile** | Real `TaskState` enum from Rust (§3.1) |
| 161 | History is in-memory only (500-line cap, lost on exit) | **missing** (persistence) | P1 via `cosmic_config` / on-disk log (§4.5) |
| 162 | Relative time formatting (`Just now`, `5m ago`, `DateFormat`) | exists | Fluent messages w/ plural+select (§5.2) |

### 6.13 Settings (9)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 163 | Distrobox version + per-row refresh | exists | `settings::item::builder().control(button)` |
| 164 | Total / running containers, installed yes-no | exists | `settings::item` rows |
| 165 | Refresh All Data + "Data refreshed" toast | exists | action row + toaster |
| 166 | Stop All Containers + confirm | exists | `confirm()` + destructive/warning class |
| 167 | Upgrade All Containers | **dead** (snackbar redirect) | Implement for real (§4.2) |
| 168 | Clear Completed Tasks + toast | exists | action row + toaster |
| 169 | About: app name, Source Code link, Distrobox docs link | exists | **`widget::about()`** (feature `about`) — replaces the whole card |
| 170 | Danger Zone: Delete All + confirm w/ warning box | exists | `confirm()` destructive |
| 171 | **No persisted preferences at all** | **missing** | `cosmic_config` (`Core::watch_config`, `CosmicConfigEntry`) — see §7 |

> **T13 additions (B3) — no parity rows, by design.** The migration adds a
> `show_skipped_lines` preference (default `false`, `AppConfig`) and the Dashboard
> surface it gates: the STATUS card gains a caption — `skipped_summary` in
> `app/src/views.rs`, reading "N rows skipped — could not be parsed." — when the
> setting is on *and* rows were dropped. Neither gets a row in the checklist above:
> rows 1–193 are the **frozen Flutter-parity set** (D19), and this capability has no
> Flutter counterpart — the Flutter app had no tolerant parser to report on, so there
> is nothing to reach parity with. It is recorded as **I13** in PLAN.md §4 where
> migration-introduced changes belong, and the preference is tabulated in
> architecture.md §7.

### 6.14 Apps page (8)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 172 | Header: title + container subtitle + refresh | exists | `header_bar` + `header_end()` |
| 173 | Loading / error+Retry | exists | shared `empty_state` |
| 174 | Search over name + exec | exists | `search_input()` |
| 175 | "Manual Binary Export" button | exists | `button::suggested` |
| 176 | "Installed in Container" + "N Found" badge | exists | `settings::section` + badge |
| 177 | Empty states (no apps / no match) | exists | `empty_state` |
| 178 | App card: icon, name, exec, export toggle, EXPORTED label | exists | `widget::grid` + `toggler` + `text::caption` |
| 179 | Export Binary dialog (path field) | exists | `dialog` + `text_input`; consider container-side browse |

### 6.15 Orphans (2)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 180 | `disk_usage_page` — unreachable, hard-coded numbers, dead buttons | **dead** | §4.4: P1 with **new** backend work (no disk field in `ContainerStats`), else drop |
| 181 | `task_page` — "Not Implemented" stub, unreachable | **dead** | **Drop**; `activity_logs_page` is the task UI |

### 6.16 Cross-cutting (12)

| # | Item | Status | libcosmic approach |
|---|---|---|---|
| 182 | 4× duplicated progress-dialog classes (+1 divergent console) | **dup** | One shared task component (§3.1) |
| 183 | 10× hand-rolled confirm dialogs w/ inconsistent copy (of 23 `AlertDialog` uses) | **dup** | One `confirm()` helper (§3.3) |
| 184 | Distro icon mapping diverges across 9 files (3 variants: 8-outcome ×6 files, 10-outcome ×1, 6-outcome ×2) | **dup/bug** | One `distro_icon()` (§3.6) |
| 185 | Distro colour mapping duplicated ×3; status colour ×4 | **dup** | Shared helpers |
| 186 | Hard-coded `Colors.green/orange/blue/red` palette | exists | `cosmic_theme` roles — required for theme/accent/high-contrast (§4.6) |
| 187 | Snackbar policy inconsistent (3 / 0 / 4 / silent) | **dup** | Toaster for every mutation result |
| 188 | 500 ms `isTaskRunning` polling (2 pages) | exists | Push updates via `Subscription` (§3.1) |
| 189 | Keyboard shortcuts | **missing** | `keyboard_nav::subscription()` + `Core::set_keyboard_nav(true)` (§5.1) |
| 190 | a11y semantics / AT support | **missing** | `a11y` feature (in default) + labelled controls (§5.3) |
| 191 | Text scaling | **missing (free)** | COSMIC global scale; avoid fixed px heights (§5.4) |
| 192 | i18n | **missing** | Fluent via `fl!` + `i18n.toml` (§5.2) |
| 193 | Theme toggle | **missing** | **Drop** — COSMIC owns it (§4.6) |

**Count corrections folded in before freezing (T0).** Each was re-verified by grep against `lib/` (28 Dart files, 12,364 lines; 8,942 hand-written, i.e. excluding the generated `lib/src/rust/`): `_getDistroIcon` is in **9 files**, not 8 (19 occurrences; §3.6, rows 33/184); the status helpers are **2 named helpers plus 2 files that re-inline the same switch**, not 4 named helpers (row 34); `AlertDialog` appears **23×**, of which **10** are confirm/destructive dialogs — ux.md's old "11" was an off-by-one on the confirm subset, not an `AlertDialog` count (§3.3, row 183); and the repo carries **65 PNGs totalling 3,739,984 bytes (3.57 MiB)**. That last figure is a repo asset count rather than a checklist row, recorded here for the audit trail. Row ids 1–193 are unchanged and no status moved, so the tally below stands as originally written.

**Totals: 193 checklist rows, ids contiguous 1–193.** Counted by status cell: **exists 148 · dead 15 · dup 14 · missing 13 · bug/fragile 3**. The "drop" decisions are recorded in the *approach* column rather than as a status (a dropped item is still "exists" or "missing" today), so `drop` is not a status count; see §4.4 and §4.5 for the 6 explicit drops (`task_page`, `disk_usage_page`-as-mock, terminal emulation, theme toggle, pull-to-refresh, bottom nav bar) plus the FAB.

Higher than the recon's ~110 because it is counted at the affordance level (each card, dialog, button, and gate is its own row) and because duplication and bugs are tracked as first-class items rather than folded into their parent feature. Collapse to ~110 by merging the per-screen state rows (#11–13, #36, #100, #108, #117, #124, #135, #173) and the per-screen task-dialog rows (#121, #132, #150) into their shared components.

**The 15 `dead` rows are the migration's real risk** — they are affordances that look functional and are not. Full list: #23 View all · #28 New Container · #29 Upgrade All (dashboard) · #68 terminal start banner · #88 folder-icon file picker · #95 `preselectedImage` · #102 image "Create Container" · #103 image custom-URL arrow · #122 pkg failure feedback · #130 updates start banner · #146 backups validation · #147 backups failure feedback · #167 settings Upgrade All · #180 disk_usage page · #181 task_page. The 3 `bug`/`fragile` rows are #148 (missing `.trim()`), #151 (undisposed controllers), and #160 (status by string-matching).

---

## 7. Open questions for the devil's advocate

**Navigation and structure**

1. **Is dropping the 1000px auto-expanded rail acceptable?** The Flutter app guarantees the labels are always visible on a wide window; COSMIC condenses on width and lets the user toggle. If the product requirement is "labels always visible on desktop", we must override `Application::nav_bar()` and reimplement the chrome — do we accept that cost, or do we accept the COSMIC behaviour?
2. **Does anyone actually want the bottom nav bar?** It only appears <600px, which on a desktop-only app (Linux RPM, per `build-rpm.sh`/`.spec`) may be dead code. Can we drop it outright rather than argue about its replacement?
3. **Nested pages vs multi-window.** Details / terminal / apps / create are pushed routes today. Inside one window with an in-app back stack, a user who opens two container details cannot compare them side by side; with `multi-window` they can. Is single-window-with-back the right default, or is losing multi-container comparison a real regression?
4. **Rename the "Images" page?** It shows distrobox's distro catalogue, not local images. Renaming improves honesty but changes the nav the user knows. Rename, or keep the label and make the page actually about images?

**Scope and dead ends**

5. **Should start-container block the whole migration, or just the three affected pages?** If it is truly blocking, it must be built before page work begins. If we defer it, three screens ship with copy that promises a capability that does not exist. Which is the lesser evil?
6. **Is `disk_usage` worth new backend work?** It is unreachable in Flutter, which suggests nobody used it. Building a real disk-usage backend (`podman system df` + per-container sizing) is a genuine feature, not a port. Fund it, or drop the page?
7. **Are the "dead" quick actions a UI bug or an unfinished feature?** "New Container", "View all", the two "Upgrade All" redirects, and the image-dialog "Create Container" are all one-liners to fix. If they were one-liners, why were they never done — was there a reason (e.g. navigation was genuinely hard in Flutter)? If the reason was Flutter-specific, we fix them all; if not, we need to know what the reason was.
8. **`widget::about()` replaces the About card — is that wanted?** It brings COSMIC's standard layout (developers/translators/license sections) and localised strings, but it will look different from the current three-row card. Cosmetic parity vs convention.

**Patterns and quality**

9. **Is a `ContextDrawer` an acceptable replacement for the bottom sheet for task output?** The Flutter sheet is user-resizable (0.5–0.95 drag). The drawer is a fixed-width panel. If resizing the console matters to users, the answer changes.
10. **Do we fix the 5-branch/8-branch distro-icon divergence?** Fixing it is correct but is a behaviour change in `package_manager_page` and `backups_page` (new icons appear). Is that in scope for a migration, or does it need product sign-off?
11. **Can we trust a real `TaskState` enum to replace string-matching?** The current string-sniffing exists because the Rust side does not classify outcomes. Changing `models/task.rs` is a backend change with FRB codegen implications. Is that in scope for the UX migration, or does it need its own plan?
12. **Does replacing the `Colors.*` literals with `cosmic_theme` roles change the app's identity?** The app has a deliberate brand (primary `#137FEC`, Inter font). COSMIC themes impose the accent colour. Is the app willing to lose its brand colour inside COSMIC, and if not, what is the sanctioned way to keep it (there is a `theme` override hook via `Application::style()`)?
13. **Persisted preferences: does anything need them?** Today nothing is persisted at all — not the selected container, not the last-used image, not the nav destination. Is that a gap to fill (P1 via `cosmic_config`) or a non-issue because the app is stateless by design?

**Accessibility and i18n**

14. **Is "extract ~600 strings during the port" realistic, or will it double the port's time?** If the team cannot absorb it, is the honest answer "i18n later, and accept re-touching every screen", and who owns that debt?
15. **Does the file picker work in the Flatpak build? — [resolved: D16/UX-15.]** The question was asked about `rfd`; `rfd` is not used. The picker is `ashpd` through libcosmic's `xdg-portal` feature, talking to `org.freedesktop.portal.FileChooser`. Under Flatpak that is the *supported* route rather than a risk: portals exist precisely so sandboxed apps can open files, the broker runs outside the sandbox, and no extra `finish-args` entry is required (§1.4). The residual check is only that `xdg-desktop-portal` plus a desktop backend is running (§3). Two corrections came with the resolution: the portal exposes neither `directory()` nor `file_name()`, so prefilled export names are undeliverable and checklist row #88 was re-scoped (D16); and `AGENTS.md`'s "all command execution through `CommandRunner`" rule is unaffected — a file chooser is not command execution, so the portal path does not cross it.
16. **Has anyone verified Orca against an `a11y`-feature libcosmic app on this distro?** If AT support is unproven in practice, calling it P0 commits us to work we have not scoped.
17. **Which strings must NOT be localised?** Container names, image URLs, command output, and the `distrobox enter` command must stay verbatim. The Flutter code interpolates user data directly into translatable strings (`'Install "$packageName" in "…"?'`, `'Delete "$name"?'`, `'Upgrading ${containerName}'`). Interpolating into a translatable string is a classic i18n mistake — the port should use Fluent placeholders, and we should decide the rule before extracting.

**Parity process**

18. **Who owns the "intended changes" list?** This document proposes several behaviour fixes (distro icons in 2 pages, failure toasts that did not exist, disabled buttons instead of silent no-ops). Each is an improvement, but together they mean the libcosmic app will not be a pixel-and-behaviour-identical port. Is that accepted, and is there a review step for each?
19. **Is 193 rows the right granularity for `PLAN.md`, or should it be the collapsed ~110?** The granularity determines how the work is estimated and tracked; a 193-row checklist implies a much larger effort than a 110-row one, and neither is wrong — but the number chosen will drive the schedule conversation.
