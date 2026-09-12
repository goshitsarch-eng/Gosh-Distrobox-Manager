# Phase 1 Architecture — Flutter/FRB → libcosmic migration

Status: **S1–S8 executed.** This document was written as design-only input to
`PLAN.md`; the strip plan in §1.3 has since been carried out, and T14 completed
S8. Where execution diverged from the plan, §1.3 records the divergence at the
end rather than editing the steps — the plan is the record of what was intended,
and rewording it after the fact would destroy the only evidence of what changed.

Branch: `cosmic-migration`. Baseline: `8eacbd8` (Flutter UI + Rust backend over
flutter_rust_bridge 2.11.1). The Flutter tree is deleted as of `c8e6cd6`; tag
`pre-t14-flutter-parity` holds the last commit that still contained it.

**Verification rule used throughout:** every libcosmic API named here was read from
the actual source at the pinned rev `a401af8b1c54a8abd393b8c5b7c8809402f83850`
(shallow clone at `/tmp/libcosmic-verify`, `git log -1` → *"fix(wayland): deliver popup
Done to the parent window's widgets too"*, `Cargo.toml` `version = "1.0.0"`,
`edition = "2024"`, `rust-version = "1.93"`). Nothing below is from memory. Where a
commonly-assumed name turns out to be **wrong**, it is called out with a ⚠ marker —
those are the traps that will burn implementation time.

---

## 0. Ground truth: verified facts and the traps they create

These were confirmed against source. They are listed first because several of them
change the shape of the design.

### 0.1 libcosmic API facts (verified)

| Fact | Verified from | Consequence |
|---|---|---|
| Crate is `libcosmic`; **lib name is `cosmic`** | `Cargo.toml` `[lib] name = "cosmic"` | `use cosmic::…`, depend on `libcosmic` by git |
| Not on crates.io | no registry entry; git-only | pin `rev = "a401af8b1c54a8abd393b8c5b7c8809402f83850"` |
| `iced` is **vendored in-tree as a git submodule** (`pop-os/iced` @ `ffe1f1dbe3cbfd313f9b5fe8e36a4af462cae5d7`), not a crates.io dep | `.gitmodules`, `git submodule status` | a fresh checkout needs `--recurse-submodules`; CI/packaging must handle submodules. `iced` is **not** separately version-pinnable |
| Trait is `cosmic::Application` (re-exported `cosmic::app::Application`) | `src/lib.rs:111`, `src/app/mod.rs:323` | not `iced::Application` |
| ⚠ **`Application` has no `fn new()`. It has `fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>)`** | `src/app/mod.rs:349` | the familiar `iced::Application::new` shape does not exist. State is built in `init`, and libcosmic hands you a `Core` you **must** store |
| ⚠ **`Application::Task` ≠ `cosmic::Task`.** In `cosmic::app`, `pub type Task<M> = iced::Task<crate::Action<M>>`; but `cosmic::Task` (re-exported at `lib.rs:170`) is bare `iced::Task<M>` | `src/app/mod.rs:20` vs `src/lib.rs:170` | `update()`/`init()` must return `cosmic::app::Task<Message>` (= `iced::Task<cosmic::Action<Message>>`). Importing `Task` from `cosmic::prelude` gives you the **wrong** type and will not compile. The upstream example imports `use cosmic::app::{Core, Settings, Task};` — do the same |
| `type Executor` is a **required** associated type | `src/app/mod.rs:328` | must write `type Executor = cosmic::executor::Default;` |
| `pub type Default = single::Executor` when the `tokio` feature is on (it is, by default) | `src/executor/mod.rs` | see 0.2 — this is the single most important finding |
| Launch is `cosmic::app::run::<App>(settings, flags) -> iced::Result` | `src/app/mod.rs:151` | `Settings::default().size(Size::new(w,h))` |
| ⚠ **`cosmic::Subscription` does not exist.** `lib.rs` re-exports `iced::Task` but **not** `iced::Subscription` | `src/lib.rs` (`grep Subscription` → only a doc comment; the trait signature at `src/app/mod.rs:460` gets it from a private `use iced::{…, Subscription, …}`) | you must import `cosmic::iced::Subscription`. The upstream `examples/subscriptions/src/main.rs` does exactly `use cosmic::iced::Subscription;` |
| `Application::subscription(&self) -> Subscription<Self::Message>` | `src/app/mod.rs:460` | subscriptions are **stateless w.r.t. `&self`** — they may only borrow state, and the stream builder is a `fn` |
| `Subscription::run(builder: fn() -> S)` / `run_with(data: D, builder: fn(&D) -> S)` where `D: Hash` | `iced/futures/src/subscription.rs:182,198` | ⚠ **the builder is a plain `fn` pointer, not a closure.** It cannot capture `self` or a local receiver. This constrains live task output — see §3.4 |
| `Application::update(&mut self, message) -> Task<Self::Message>` | `src/app/mod.rs:466` | return-convention section §2.3 |
| `impl<M> From<M> for Action<M>` (`Action::App`) | `src/action.rs:77` | `Task::done(Message::X)` works if inference picks `Y = cosmic::Action<Message>`; otherwise use `cosmic::Action::App(msg)` |
| `ApplicationExt::watch_config::<T>(id) -> iced::Subscription<cosmic_config::Update<T>>` | `src/app/mod.rs:551` | free, reactive config reload — §5 |
| `on_window_resize(&mut self, id: window::Id, width: f32, height: f32)` | `src/app/mod.rs:455` | this is where window-size persistence hooks in |
| `libcosmic` on Linux **unconditionally** depends on `cosmic-config` with `features = ["dbus"]` | `Cargo.toml:174` (target-gated block) | pulls `zbus` + `cosmic-settings-daemon`. Flatpak sandboxes allow the session bus; no blocker, but it is weight you already pay |
| `libcosmic` requires `rust-version = 1.93`; workspace has 1.98.1 | `Cargo.toml`, local `cargo 1.98.1` | fine |

### 0.2 ⚠ The runtime trap: which callbacks have a runtime entered

`single::Executor` is a **real tokio multi-thread runtime**, but with
`worker_threads(1)`, and iced **enters** it around the callbacks it drives
(`Executor::enter`). Verified in the vendored submodule at the pinned rev
(`iced/winit/src/lib.rs` @ `ffe1f1dbe3cbfd313f9b5fe8e36a4af462cae5d7`): `:2030`
`let task = runtime.enter(|| program.update(message));`, `:2054`
`runtime.enter(|| program.subscription())`, `:2038` the task stream's poll, `:125`
instance creation — with `:2043` `runtime.run(stream)` driving the task. So
**`update()` is inside that runtime**: `update()` → `Backend::create_container()` →
`tokio::spawn(...)` does **not** panic with *"there is no reactor running"*.

The trap is therefore not `update()` but the contexts where **no** runtime is entered:
a bare `std::thread`, or the body of a `smol`-driven unit test (`smol` is this crate's
dev-dependency; `tokio::spawn` under `smol::block_on` has no ambient runtime — the
existing suite already drives async `Distrobox` methods that way, under
`smol::block_on`). Today `api.rs` calls `tokio::spawn` freely because FRB invoked
these functions on its own tokio runtime; that ambient runtime exists in neither of
those two contexts.

**Rule for the whole port:** any core entry point that internally spawns must be
called from inside a `Task`/`Subscription` future — i.e. wrapped in
`cosmic::task::future(…)` — never invoked synchronously. The rule is *not* justified by
a panic out of `update()` (there is none); it is justified because the same core entry
points are also reached with no runtime at all, and because routing through a `Task` is
what carries the result — and cancellation — back to the UI.
`spawn_task` (§4) documents this in its own doc-comment, and `Backend`'s API is shaped
so the only ergonomic way to call it is from a task.

### 0.3 Backend bugs confirmed by reading source

All of these were read in this repo, not inferred. Each is fixed by the §6.4 row named
with it.

**Why these carry no line numbers.** They were measured against the pre-T1 `rust/` tree
(recoverable as `git show 634f9d3:rust/src/…`), and every one of them has since moved at
least twice: T1's `git mv rust core`, the §6.4 fix that edited the very function being
cited, and T13's tolerant-parsing pass. A Phase-1 finding that cites a line number is
stale the moment its fix lands — which is a property of the finding, not of the tree — so
each item here points at its fix row instead. That is the same convention `DECISIONS.md`
adopted for the five Rust-side sites that cited *this* document by line number. Where a
number is still given below it is a number in the **current** tree, checked when this
paragraph was written (T16).

1. **`emerge` dead-ends** (§6.4, row B1). `detect_package_manager` emitted `"emerge"`
   (and `"xbps"`, `"yum"`) but `list_installed_packages` and `search_packages` matched
   `dnf|yum` and `xbps` and **not** `emerge`, so the catch-all arm returned
   `Error::CommandFailed { stderr: "Unsupported package manager: emerge" }`. Worse:
   `install_package` and `remove_package` each embedded their **own, different** detection
   script that omitted the `emerge` branch entirely, so a Gentoo container reported
   `unknown` and `exit 1`ed. Independently, `models/known_distros.rs` mapped
   `gentoo` → `PackageManager::Unknown`, and the `PackageManager` enum had neither an
   `Emerge` nor an `Xbps` variant, so the `install_cmd_for_file` / `package_file_ext`
   paths could not serve Gentoo or Void either.
   **Fixed (T8):** the three divergent scripts collapsed into one
   `distrobox.rs::detect_script`, the enum gained `Xbps` and `Emerge`
   (`known_distros.rs:56`, `:57`), `gentoo` now maps to `PackageManager::Emerge`
   (`known_distros.rs:19`), and the package-manager listing and search paths cover it.
   **Residual, and wider than this item claimed:** installing a *local file* still cannot
   serve them — `PackageManager::install_cmd` returns `None` for `Emerge | Unknown`
   (`known_distros.rs:130`) and `installable_file` returns `None` for
   `Xbps | Emerge | Unknown` (`:142`), deliberately, since emerge and xbps have no direct
   local-file verb. That is a capability the Flutter original also lacked, not a
   regression, but it means "B1 is fixed" is true of *listing* and *searching* only.
2. **Chunk-based "lines"** (§6.4, row B2). `stream_reader_to_task_output` read into a
   `[u8; 1024]` and pushed **each read** as one "line". One log line could arrive as three
   entries; three lines could arrive as one. apt/dnf/pacman progress bars (`\r`-driven)
   rendered as garbage.
   **Fixed (T5/B2):** the reader accumulates into a `Vec<u8>` and emits complete records
   split on `\n` **and** `\r`, holding the incomplete tail across reads and flushing on EOF.
3. **One bad line kills the list** (§6.4, row B3). `Distrobox::list` returned `Err` after
   `error!(…)` on a single unparseable `distrobox ls` row, discarding every good row with
   it — the entire container list failed. The same shape appeared in `list_snapshots` and
   `list_installed_packages`, though those silently skipped instead of failing, which is
   the opposite inconsistency.
   **Fixed (T13/B3):** parse failures became `warn!` + collect, and `list` returns
   `ContainerList { containers, skipped }`. `TolerantList<T>` carries the other four.
   **As built, the container type is a `Vec`, not the `BTreeMap` this row originally
   specified** — see row B3 and `distrobox.rs::ContainerList` for why.
4. **Naive `%`-strip** (§6.4, row B4). `launch_app` stripped exactly
   `" %f" " %u" " %F" " %U"` using `str::replace` — which also mangled those byte
   sequences appearing anywhere inside arguments, not just as trailing field codes. The
   Desktop Entry spec's field codes are `%f %F %u %U %i %c %k %v %m %d %D %n %N`, plus
   `%%` for a literal `%`; `%i` expands to **two** arguments (`--icon <name>`) and `%c` is
   the translated name. Separately, `cmd.arg(cleaned_exec)` passed the entire exec string
   as **one argv element**, relying on distrobox to re-split it — a correctness and
   injection hazard.
   **Fixed (T13/B4):** `desktop_file::split_exec` honours the spec's quoting and escaping
   rules, and `launch_app` feeds the resulting argv to `cmd.args()`. **As built, the
   failure mechanism was the argv slot count rather than a re-split** — `distrobox-enter`
   ends in `exec "$@"` with no `eval`, so the single-string form asked for a program whose
   name was the whole fused string. Row B4 records the measurement.
5. **No start-container op** (§6.4, row B5). `Distrobox` exposed `stop`, `stop_all`,
   `remove` and `enter_cmd` — but nothing that starts a stopped or `Created` container.
   The Dart UI's `stopContainer` / `stopAllContainers` had no inverse.
   **Fixed (T9/B5):** `Distrobox::start` (`podman start <name>`, Docker fallback) plus
   `start_and_enter`, with the `Status::Created|Exited → Up` transition observed by a
   `list()` refresh rather than assumed. This is what makes the start banners on the
   terminal and Updates pages (rows #68/#130) real CTAs instead of copy.
6. **Cancel leaks the child process** (§6.4, row B8). `cancel_task` calls
   `task.handle.abort()`. Dropping the task drops the `Box<dyn Child>`. The
   `async-process` docs are explicit (`async-process-2.5.0/src/lib.rs:223` — an
   external pin, §0.1):
   *"If the `Child` is dropped, the process keeps running in the background"* —
   `kill_on_drop` is opt-in (`:1064`) and this crate never sets it. So "Cancel" on a
   `distrobox create` abandons a still-running process and reports it as cancelled.
   **Fixed (T5/B8):** §3.3's `oneshot` + `select!` + `Child::kill()` replaced
   `JoinHandle::abort`, so the child is signalled before its handle is dropped.
7. **`podman`/`docker` fallback duplicated six times** (§6.4, row B7).
   `start`, `get_container_id`, `create_snapshot`, `list_snapshots`,
   `delete_snapshot` and `get_container_stats` each inline
   their own "try podman, on error try docker" block with the same format strings. All
   of them *do* route through `self.cmd_output_string` → `self.cmd_runner` (so the
   Flatpak / host-exec mapping is applied — good), but the runtime selection logic is
   copy-pasted and inconsistent.
   *Correction (T13/D27):* this item first read "seven times" and listed
   `export_container` and `import_container` among them. Counted,
   neither ever had a docker branch — both are podman-literal, because both are
   **streaming** (`cmd_spawn` → a `Child`) and the fallback shape is an output one. So
   six was the true count, `start` was missing from the list, and the two streaming
   sites were never in scope. The B7 row inherits this correction.
   **Fixed (T13/B7):** all six route through `Distrobox::runtime_output` plus
   `container_runtime::retarget` — one `Command::program` swap, argv byte-identical.
8. **Env-guard detection is duplicated and weaker in Rust** (§6.4, row B6). The Dart
   guard (`lib/utils/environment_guard_io.dart`, deleted with the Flutter tree in T14)
   checks the three hardcoded absolute paths **and** scans `$PATH`; the Rust guard
   (`core/src/backends/host_exec.rs::has_distrobox_host_exec`) checks only the three
   absolute paths. The precedence chain itself was written twice — in `AppState::new`
   and in `is_distrobox_installed` (both pre-T1 `rust/src/` files, since deleted).
   **Fixed (T13/B6):** the guard gained the `$PATH` scan and the chain collapsed to a
   single `env::detect` (§2.4), called once, with the `Blocked` wording preserved.

### 0.4 What is *not* a bug, despite looking like one

- The T1 modules genuinely have zero `flutter_rust_bridge` imports. Verified:
  `grep -rn flutter_rust_bridge rust/src/{backends,fakers,models}` → no hits. FRB
  coupling was exactly: `api.rs` (`#[frb]`, `StreamSink`, `pub use` re-export list),
  `rust/src/lib.rs` line 1 (`mod frb_generated;`), `frb_generated.rs`, the
  `flutter_rust_bridge` dep, `[lib] crate-type = ["staticlib","cdylib"]`, and
  `flutter_rust_bridge.yaml`. All of it is gone as of T14; `core/src/lib.rs` is now an
  eight-line `pub mod` list with no FRB item in it.
- `anyhow` is only used at the `api.rs` boundary for `map_err(|e| anyhow::anyhow!(e))`.
  Inside the backend, errors are already the typed `distrobox::Error` (thiserror).
- The legacy gschema (`rust/data/io.github.gosh_distrobox_manager.gschema.xml`) was
  read by **nobody** (no `gio`/`Settings` usage in Rust or Dart). Its keys were
  dead but their *intent* is the config spec — see §5.3. The file is **deleted**, not
  moved: T1 (`f2ffa5c`) removed it with the rest of `rust/data/`, and §5.4's key table
  is what carries the intent forward.
- `models/known_distros.rs`, `flatpak.rs`, `host_exec.rs`, `desktop_file.rs`,
  `output_tracker.rs`, `fakers/*` are clean and testable as-is. Several already carry
  `#[cfg(test)]` unit tests (e.g. `flatpak.rs`, `host_exec.rs`) that must keep passing.

---

## 1. Workspace layout

### 1.1 Target layout

```
/                                   # workspace root (was the Flutter project root)
├── Cargo.toml                      # NEW: virtual manifest, [workspace]
├── Cargo.lock                      # git mv rust/Cargo.lock Cargo.lock
├── core/                           # git mv rust core   — lib crate
│   ├── Cargo.toml                  #   name = "gosh-distrobox-core"
│   ├── src/{lib,env,service,task_runtime}.rs
│   ├── src/backends/{flatpak,host_exec,desktop_file,supported_terminals}.rs
│   ├── src/backends/distrobox/{mod,command,distrobox}.rs
│   ├── src/models/{mod,task,known_distros,dto}.rs
│   └── src/fakers/{mod,command,command_runner,host_env,output_tracker}.rs
├── app/                            # NEW: bin crate, libcosmic
│   ├── Cargo.toml                  #   name = "gosh-distrobox-manager"
│   ├── src/main.rs                 #   cosmic::app::run::<App>
│   ├── src/app.rs                  #   impl cosmic::Application
│   ├── src/message.rs, state.rs
│   ├── src/pages/*.rs              #   one per former Flutter screen
│   └── data/                       # git mv core/data app/data
│       ├── io.github.gosh_distrobox_manager.{desktop,metainfo.xml,service}.in
│       └── (gschema.xml — see §5.4)
└── docs/migration/architecture.md
```

### 1.2 Why these crate names

- **`gosh-distrobox-core`** (`gosh_distrobox_core`). Today's crate is
  `gosh_distrobox_manager` and it is both the lib *and* the shipped binary name.
  Splitting requires the lib to stop claiming the product name. `-core` states the
  contract ("no UI, no FFI, reusable, unit-testable against `NullCommandRunner`")
  and leaves the product name free for the binary.
- **`gosh-distrobox-manager`** (bin). The packaging chain already hardcodes this
  spelling and must keep working: `gosh-distrobox-manager.spec` (`Name:`,
  `%{_libdir}/%{name}`, `_bindir/gosh_distrobox_manager`),
  `rust/data/…desktop.in` (`Exec=gosh_distrobox_manager`). A hyphenated package name
  is the Cargo convention for binaries and yields `cargo run -p gosh-distrobox-manager`.
- **`APP_ID = "io.github.gosh_distrobox_manager"` must be kept verbatim.** It is the
  gschema id, the metainfo `<id>`, the `.desktop` `Icon=`, the D-Bus `.service.in`
  name, and the intended Flatpak app id. It is *also* the `cosmic_config::Config`
  id (§5) — one string, four consumers.
- **Workspace, not separate repos.** One `Cargo.lock` (so `core` and libcosmic's
  vendored `iced` resolve `tokio`/`serde`/`futures` to a single version — a real
  risk otherwise, since `core` already depends on `tokio`, `futures`, `serde`, and
  `thiserror`, all of which libcosmic/iced also pull), one `target/`, and
  `cargo test --workspace` exercising everything.
- Do **not** name the lib `gosh-distrobox-manager-core`; the binary name is the
  user-visible artifact and `-core` is unambiguous in a 2-crate workspace.

### 1.3 Strip plan — every step compiles, tests, and clippy-cleans

Invariant: after each step, `cargo build --workspace && cargo test --workspace &&
cargo clippy --workspace --all-targets -- -D warnings` is green. Nothing in this
plan is a "delete the FFI and see what breaks" step — each deletion is preceded by a
pure-move commit that relocates whatever the deletion would otherwise strand.

**Scope of the line numbers below.** Every `api.rs` line number in these steps is a
measurement of the **pre-strip** tree, when the file still existed; `api.rs` was
deleted at S8, so no number here can resolve today and none is meant to. They are left
as written on purpose — §1.3.1 records where execution diverged, and rewriting a plan
to match its outcome destroys the evidence that it was a plan.

**S1 — Workspace scaffold (no renames).**
- Add root `Cargo.toml`: `[workspace] members = ["rust"] resolver = "3"` (edition
  2024 → resolver 3; a *virtual* manifest must state it explicitly).
- `git mv rust/Cargo.lock Cargo.lock`.
- Hoist `[workspace.dependencies]` (thiserror, tokio, futures, serde, serde_json,
  async-channel, async-process, regex, toml, tracing, tracing-subscriber,
  async-trait, uuid, dirs, once_cell) and switch `rust/Cargo.toml` to
  `.workspace = true`. `flutter_rust_bridge` stays a **crate-specific** dep.
- Green: the crate is byte-identical in behaviour; only dep resolution moved.

**S2 — `git mv rust core`; rename the package.**
- `git mv rust core`; root `members = ["core"]`.
- `core/Cargo.toml`: `name = "gosh-distrobox-core"`.
- Drop `[lib] crate-type = ["staticlib","cdylib"]` → default `rlib`. Safe now: the
  only consumer of the cdylib was Flutter, which S8 deletes; `frb_generated.rs`
  compiles identically as an rlib.
- Green: no source change.

**S3 — Pure move: DTOs out of `api.rs`.**
- `api.rs` defines `AppInfo` (`:26`) and `ExportedBinary` (`:36`) and re-exports 12
  more types (`:11-22` — one `pub use` per line, counted at `e5436a0`).
- New `core/src/models/dto.rs` holding `AppInfo`, `ExportedBinary` and the
  `pub use` set (`ContainerInfo`, `CreateArgs`, `Volume`, `VolumeMode`, `Status`,
  `CreateArgName`, `DesktopEntry`, `PackageInfo`, `SnapshotInfo`, `ContainerStats`,
  `KnownDistro`, `PackageManager`). `models/mod.rs` gains `pub mod dto; pub use dto::*;`.
- `api.rs` reduces to `pub use crate::models::*;` (the re-export list is what FRB
  scans, so keep the names).
- Green: pure relocation. Ordering matters — this must land **before** S7, or S7
  deletes types the app needs.

**S4 — Pure move: task machinery out of `api.rs`.**
- `api.rs` owns `STATE` (`:42`), `MAX_TASK_OUTPUT_LINES`/`COMPLETED_TASK_TTL`
  (`:43-44`), `push_task_output` (`:46`), `finish_task` (`:56`),
  `stream_reader_to_task_output` (`:78`), `run_child_task` (`:95`).
- New `core/src/task_runtime.rs` with those, plus the registry that
  `AppState.tasks` currently holds. `api.rs` keeps only thin `#[frb]` wrappers
  calling into it.
- Green: pure relocation, FRB still wired.

**S5 — Create the `app/` bin crate; wire a skeleton `cosmic::Application`.**
- `app/Cargo.toml`: `name = "gosh-distrobox-manager"`,
  `libcosmic = { git = "https://github.com/pop-os/libcosmic", rev = "a401af8b1c54a8abd393b8c5b7c8809402f83850", features = [<PLAN §1 list>] }` (PLAN §1 is the single feature list — currently `winit`, `tokio`, `a11y`, `wayland`, `x11`, `multi-window`, `about`, `xdg-portal` with `default-features = false`; D12+D23. Do not copy a shorter list from an older revision of this doc.),
  `gosh-distrobox-core = { path = "../core" }`, plus `tokio`, `futures`.
- `app/src/main.rs` + `app.rs`: the verified skeleton from §2.3 — `type Executor =
  cosmic::executor::Default`, `fn init(core, ())`, `fn view` returning a placeholder
  `widget::Row`. One nav page only.
- `git mv core/data app/data`.
- Green: `cargo build -p gosh-distrobox-manager` builds; `core` is untouched and
  still carries its FRB modules (the dep is still declared there), so
  `cargo test -p gosh-distrobox-core` is unaffected.

**S6 — Stand up `core::service::Backend` and switch `app` onto it.**
- New `core/src/service.rs`: a `Backend` struct owning the `CommandRunner`,
  `Distrobox`, and `TaskRegistry` (i.e. today's `AppState` + `api.rs` functions),
  with methods returning `Result<T, CoreError>`.
- `app` calls `Backend` methods and stops being able to reach `api.rs` at all.
- Green: FRB shims still exist and still compile; they simply delegate to `Backend`
  too (or are left alone and become thin wrappers).

**S7 — Delete FRB entirely.**
- `git rm core/src/api.rs core/src/frb_generated.rs flutter_rust_bridge.yaml`
- `core/src/lib.rs`: drop `mod frb_generated;` and `pub mod api;` (they were lines 1
  and 2 of the pre-T1 file; the file is now a plain eight-module list).
- `core/Cargo.toml`: remove `flutter_rust_bridge`.
- `git rm -r lib/src/rust` (generated Dart).
- Green: nothing references the removed symbols any more (S3 moved the types, S4 the
  runtime, S6 the callers).

**S8 — Delete the Flutter frontend and re-point packaging.**
- `git rm -r lib test android ios macos windows web pubspec.yaml
  analysis_options.yaml linux/CMakeLists.txt linux/.gitignore fix-icon-cache.sh
  update-caches.sh flutter_rust_bridge.yaml`
- Rewrite `gosh-distrobox-manager.spec` (`%build` → `cargo build --release
  -p gosh-distrobox-manager`; drop `BuildRequires: gtk3-devel`, add
  `cargo`/`rust`/`libxkbcommon-devel`/`wayland-devel`; install `target/release/…`
  instead of a Flutter bundle), `build-rpm.sh`, `AGENTS.md`, `README.md`.
- `.github/workflows/flatpak.yml` is **already stale** (it invokes
  `io.github.gosh_distrobox_manager.json` and a Meson build that do not exist in the
  repo) — replace it wholesale in a later task, don't try to patch it.
- Green: `cargo build --workspace` is unaffected; only packaging/CI text changed.

**S9+ — the T2/T3 work and the UI build-out.** Sequenced in §7.

### 1.3.1 How execution diverged from the plan above

Recorded by T14, which finished S8. Each item is a place the steps above and the
shipped tree disagree; the steps are left as written.

- **S2's crate-type drop did not happen in S2.** `[lib] crate-type =
  ["staticlib","cdylib"]` survived until T14, which removed it once the Flutter
  FFI was actually gone. Harmless in between — nothing but Flutter ever linked
  the cdylib — but the step above reads as though it landed at S2.
- **S5's `git mv core/data app/data` never happened, deliberately.** Desktop
  integration data stayed in `core/data` and `app` reads it from there. Moving it
  to `app/` would have made the backend crate own no data while the UI crate owned
  the packaging inputs, which is backwards: the RPM spec and the Flatpak manifest
  both install from `core/data`, and `%check` validates the copies there. This is
  the "single source" rule in AGENTS.md.
- **S8 deleted more than the step lists.** Beyond the named paths, T14 also
  removed `assets/`, the rest of `linux/` (not only `CMakeLists.txt` and
  `.gitignore` — the CMake runner, `linux/runner/`, `linux/flutter/` and
  `linux/data/`), `build/linux/…` references, and the four `core/data/*.{in,
  gschema.xml}` templates plus `core/data/icons/meson.build`, which existed only
  to feed the deleted meson build. It also removed the committed 18 MB
  `gosh-distrobox-manager-1.0.0-linux-x64.tar.gz` and several Flutter-era
  scripts. Deletion and the RPM rewrite had to land in **one** commit: the spec
  and `build-rpm.sh` read `linux/data/…`, so splitting them would have left the
  RPM path broken in between (the E12 trap).
- **S8's judgement of `flatpak.yml` was wrong, in the file's favour.** The step
  says it "is already stale … replace it wholesale". By T14 it was not stale: T4
  had already rewritten it to drive `./scripts/verify.sh`, and it was correct
  except for one comment claiming an artifact-publish step that no step
  implements. T14 fixed the comment and left the workflow alone — a wholesale
  replacement would have destroyed working code.
- **S7 removed two modules the step does not name.** `core/src/app_state.rs` and
  `core/src/task_runtime.rs` went with `api.rs`; S3/S4 had moved DTOs and the task
  registry out of `api.rs` precisely so that nothing would be stranded by S7, and
  by then `Backend` (S6) owned both. `once_cell` became an unused dependency at
  that moment and was dropped from the workspace manifest.

### 1.4 One ordering constraint to respect

`git mv rust core` (S2) moves the `.git` history path for every T1 file, so **do S2
before any file-level editing**. Renaming after editing makes the diff unreadable and
`git log --follow` unreliable for exactly the 2139-line file that most needs blame.

---

## 2. Message enum

### 2.1 Coverage: all 35 `api.rs` functions

Every public item in `rust/src/api.rs`, with where it lands. "Msg" = an
`app::Message` variant; "Task" = reached via `update()` returning a `Task`.

| # | `api.rs` fn | Line | Message / disposition |
|---|---|---|---|
| 1 | `init_app` | 132 | **dropped** — `tracing_subscriber::fmt::try_init()` moves to `app::main()` |
| 2 | `get_distrobox_version` | 137 | `Distrobox(VersionRequested)` → `Distrobox(VersionLoaded(Result<String,CoreError>))` |
| 3 | `is_distrobox_installed` | 141 | folded into the env guard (§2.4); `Env(EnvProbed(EnvGuard))` |
| 4 | `get_containers` | 163 | `Containers(RefreshRequested)` → `Containers(Loaded(Result<ContainerList,CoreError>))` |
| 5 | `create_container` | 168 | `Containers(CreateRequested(CreateArgs))` → `Tasks(TaskStarted(Result<TaskId,CoreError>))` |
| 6 | `stream_task_output` | 210 | **dropped as a function** — becomes the task-output `Subscription` (§3.4) |
| 7 | `remove_container` | 247 | `Containers(RemoveRequested(String))` → `Containers(ActionFinished(Result<String,CoreError>))` |
| 8 | `stop_container` | 252 | `Containers(StopRequested(String))` → `Containers(ActionFinished(…))` |
| 9 | `stop_all_containers` | 257 | `Containers(StopAllRequested)` → `Containers(ActionFinished(…))` |
| 10 | `upgrade_container` | 262 | `Containers(UpgradeRequested(String))` → `Tasks(TaskStarted(…))` |
| 11 | `clone_container` | 304 | `Containers(CloneRequested{source,args})` → `Tasks(TaskStarted(…))` |
| 12 | `get_enter_command` | 346 | **replaced** — `Terminals(EnterRequested{container,terminal})` launches directly (§6.4-T2b) |
| 13 | `list_container_apps` | 358 | `Apps(LoadRequested(String))` → `Apps(Loaded(Result<Vec<AppInfo>,CoreError>))` |
| 14 | `export_app` | 370 | `Apps(ExportRequested{container,desktop_file_path})` → reapplies #13 |
| 15 | `unexport_app` | 375 | `Apps(UnexportRequested{…})` → reapplies #13 |
| 16 | `list_exported_binaries` | 380 | `Apps(BinariesLoadRequested(String))` → `Apps(BinariesLoaded(…))` |
| 17 | `export_binary` | 390 | `Apps(BinaryExportRequested{container,binary_path})` → reapplies #16 |
| 18 | `unexport_binary` | 395 | `Apps(BinaryUnexportRequested{…})` → reapplies #16 |
| 19 | `list_available_images` | 404 | `Images(LoadRequested)` → `Images(Loaded(Result<Vec<String>,CoreError>))` |
| 20 | `get_active_tasks` | 413 | **dropped** — replaced by the app's own `tasks: BTreeMap<TaskId, TaskView>` |
| 21 | `is_task_running` | 419 | **dropped** — `TaskView.completed` in app state |
| 22 | `cancel_task` | 429 | `Tasks(CancelRequested(TaskId))` → `Tasks(Cancelled(TaskId))` |
| 23 | `detect_package_manager` | 454 | `Packages(ManagerDetected(Result<PackageManager,CoreError>))` — note **typed**, not `String` (§6.4-B1) |
| 24 | `list_installed_packages` | 459 | `Packages(LoadRequested(String))` → `Packages(Loaded(…))` |
| 25 | `search_packages` | 464 | `Packages(SearchRequested{container,query})` → `Packages(SearchLoaded(…))` |
| 26 | `install_package` | 469 | `Packages(InstallRequested{container,package})` → `Tasks(TaskStarted(…))` |
| 27 | `remove_package` | 511 | `Packages(RemoveRequested{container,package})` → `Tasks(TaskStarted(…))` |
| 28 | `create_snapshot` | 557 | `Snapshots(CreateRequested{container,snapshot})` → reapplies #29 |
| 29 | `list_snapshots` | 562 | `Snapshots(LoadRequested{filter})` → `Snapshots(Loaded(…))` |
| 30 | `delete_snapshot` | 567 | `Snapshots(DeleteRequested(String))` → reapplies #29 |
| 31 | `restore_from_snapshot` | 572 | `Snapshots(RestoreRequested{snapshot,new_name})` → `Tasks(TaskStarted(…))` |
| 32 | `export_container_to_file` | 618 | `Backups(ExportRequested{container,path})` → `Tasks(TaskStarted(…))` |
| 33 | `import_container_from_file` | 660 | `Backups(ImportRequested{archive,image})` → `Tasks(TaskStarted(…))` |
| 34 | `get_container_stats` | 706 | `Stats(LoadRequested(String))` → `Stats(Loaded(Result<ContainerStats,CoreError>))` |
| 35 | `run_command_in_container` | 711 | `Terminals(RunRequested{container,command})` → `Terminals(RunFinished(Result<String,CoreError>))` |

New ops with no `api.rs` precedent (from §6.4): **`Containers(StartRequested(String))`**
(B5) and **`Snapshots`/`Containers(StartAndEnter{container,terminal})`**.

### 2.2 Coverage: all `AppStateProvider` members

`lib/providers/app_state.dart` is the Flutter UI contract. Mapping (Dart → `Message`):

| Dart member | Line | `Message` |
|---|---|---|
| `refresh()` | 178 | `Containers(RefreshRequested)` + `Containers(Loaded(…))` |
| `selectContainer(...)` | 205 | `Containers(Selected(Option<ContainerInfo>))` — pure UI, no async |
| `removeContainer` | 215 | `Containers(RemoveRequested)` |
| `stopContainer` | 238 | `Containers(StopRequested)` |
| `stopAllContainers` | 261 | `Containers(StopAllRequested)` |
| `upgradeContainer` | 284 | `Containers(UpgradeRequested)` |
| `cloneContainer` | 307 | `Containers(CloneRequested)` |
| `createContainer` | 331 | `Containers(CreateRequested)` |
| `getEnterCommand` | 354 | `Terminals(EnterRequested)` (now *launches*) |
| `loadContainerApps` | 362 | `Apps(LoadRequested)` |
| `exportApp` / `unexportApp` / `toggleAppExport` | 381/405/431 | `Apps(ExportRequested)` / `Apps(UnexportRequested)`; `toggleAppExport` is a UI-level dispatch choosing between them |
| `loadExportedBinaries` | 444 | `Apps(BinariesLoadRequested)` |
| `exportBinary`/`unexportBinary` | 463/487 | `Apps(BinaryExportRequested)` / `Apps(BinaryUnexportRequested)` |
| `loadAvailableImages` | 515 | `Images(LoadRequested)` |
| `_startTaskTracking` | 537 | **replaced** — the task-output `Subscription` (§3.4) |
| `isTaskRunning` | 567 | state read; `Tasks(TaskQuery)` if a refresh is ever needed |
| `cancelTask` | 571 | `Tasks(CancelRequested)` |
| `clearCompletedTasks` | 585 | `Tasks(ClearCompleted)` |
| `clearActionError` | 590 | `Ui(DismissError)` |
| `detectPackageManager` | 600 | `Packages(ManagerDetectRequested)` |
| `loadInstalledPackages` | 616 | `Packages(LoadRequested)` |
| `searchPackages` | 635 | `Packages(SearchRequested)` |
| `clearSearchResults` | 660 | `Packages(ClearSearch)` |
| `installPackage` | 666 | `Packages(InstallRequested)` |
| `removePackage` | 689 | `Packages(RemoveRequested)` |
| `loadSnapshots` | 716 | `Snapshots(LoadRequested)` |
| `createSnapshot` | 735 | `Snapshots(CreateRequested)` |
| `deleteSnapshot` | 759 | `Snapshots(DeleteRequested)` |
| `restoreFromSnapshot` | 783 | `Snapshots(RestoreRequested)` |
| `exportContainerToFile` | 809 | `Backups(ExportRequested)` |
| `importContainerFromFile` | 835 | `Backups(ImportRequested)` |
| `loadContainerStats` | 865 | `Stats(LoadRequested)` |
| `runCommandInContainer` | 884 | `Terminals(RunRequested)` |
| `isLoading`, `isDistroboxInstalled`, `environmentBlocked`, `runningContainersCount`, `stoppedContainersCount` + the 7 `isLoadingX` getters | 97-143 | **derived view helpers on `App`**, not messages. `running/stoppedContainersCount` are computed from `self.containers` via `Status` |

### 2.3 DRAFT: `Message` and the return conventions

```rust
// app/src/message.rs
use cosmic::iced::window;
use gosh_distrobox_core::{
    AppInfo, ContainerInfo, ContainerStats, CoreError, CreateArgs, EnvGuard,
    ExportedBinary, PackageInfo, PackageManager, SnapshotInfo, TaskId, Terminal,
};

#[derive(Clone, Debug)]
pub enum Message {
    // ---- libcosmic-managed UI ----
    /// Nav-bar selection; libcosmic delivers this as `cosmic::Action::Cosmic`.
    NavSelect(cosmic::widget::nav_bar::Id),
    WindowResized(window::Id, f32, f32),   // feeds window-size persistence (§5)
    Dialog(ConfirmMsg),

    Containers(ContainerMsg),
    Apps(AppMsg),
    Images(ImageMsg),
    Packages(PackageMsg),
    Snapshots(SnapshotMsg),
    Backups(BackupMsg),
    Stats(StatsMsg),
    Tasks(TaskMsg),
    Terminals(TerminalMsg),
    Env(EnvMsg),
    Config(ConfigMsg),
    Ui(UiMsg),
}

#[derive(Clone, Debug)]
pub enum ContainerMsg {
    /// Union of the Dart `refresh()` + `loadContainers()`.
    RefreshRequested,
    /// Carries the *tolerant* list result (§6.4-B3) so the UI can render
    /// "12 containers, 3 rows skipped" instead of an empty error screen.
    Loaded(Result<ContainerList, CoreError>),
    Selected(Option<ContainerInfo>),
    CreateRequested(CreateArgs),
    CloneRequested { source: String, args: CreateArgs },
    StartRequested(String),
    StopRequested(String),
    StopAllRequested,
    RemoveRequested(String),
    UpgradeRequested(String),
    /// Result of a short (non-task) mutation; `Err` becomes a toast.
    ActionFinished(Result<String, CoreError>),
}

#[derive(Clone, Debug)]
pub enum TaskMsg {
    /// `Err` here means the task could not even be *started*.
    Started { label: String, result: Result<TaskId, CoreError> },
    /// One batch of streamed output lines, appended to `TaskView.output`
    /// (ring-buffered to `MAX_TASK_OUTPUT_LINES`).
    Output { id: TaskId, lines: Vec<String> },
    Completed { id: TaskId, success: bool },
    CancelRequested(TaskId),
    Cancelled(TaskId),
    ClearCompleted,
    /// TTL sweep evidence from core; lets the UI drop its mirror entry.
    Expired(Vec<TaskId>),
}

#[derive(Clone, Debug)]
pub enum EnvMsg {
    Probed(EnvGuard),
    /// User chose "run anyway" from the blocked screen.
    OverrideAccepted,
}

#[derive(Clone, Debug)]
pub enum TerminalMsg {
    /// Launch `terminal` attached to `container`.
    EnterRequested { container: String, terminal: Terminal },
    TerminalsLoaded(Vec<Terminal>),
    RunRequested { container: String, command: String },
    RunFinished(Result<String, CoreError>),
}
```

**Return conventions.** Verified against **libcosmic's** `src/app/mod.rs:20` and
`src/action.rs:77` (the pinned rev of §0.1 — these are the framework's paths, not this
repo's, and `app/src/action.rs` does not exist):

```rust
// app/src/app.rs
use cosmic::app::{Core, Task};          // <= the alias: iced::Task<cosmic::Action<Message>>
use cosmic::iced::Subscription;         // <= NOT cosmic::Subscription (does not exist)
use cosmic::prelude::*;

// 1. no-op
fn no_op() -> Task<Message> { Task::none() }

// 2. immediate message (synchronous state follow-up)
fn done(m: Message) -> Task<Message> { Task::done(cosmic::Action::App(m)) }

// 3. run an async backend call. THE ONLY SAFE PLACE TO CALL Backend methods
//    that spawn tasks — see §0.2.
fn run<F>(f: F) -> Task<Message>
where F: std::future::Future<Output = Message> + Send + 'static {
    cosmic::task::future(f)   // X = Message satisfies X: Into<cosmic::Action<Message>>
}

// 4. many
fn batch(ts: Vec<Task<Message>>) -> Task<Message> { Task::batch(ts) }
```

`Task::done(Message::X)` also compiles via `impl<M> From<M> for Action<M>`, but only
when inference has already fixed `Y = cosmic::Action<Message>`; the explicit
`cosmic::Action::App(…)` form is used in this doc because it never fights inference.

### 2.4 The env guard, moved into core

`EnvMsg::Probed(EnvGuard)` is produced once in `init()` and re-probed on demand. The
guard becomes a **core** type so the Rust side is the single source of truth (today
Dart and Rust each have their own copy — §0.3-B8):

```rust
// core/src/env.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvMode {
    /// Neither Flatpak nor a Distrobox container: run commands directly.
    Native,
    /// `/.flatpak-info` present: prefix with `flatpak-spawn --host`.
    FlatpakHost,
    /// Inside a Distrobox with `distrobox-host-exec` on PATH: prefix with it.
    DistroboxHostExec,
    /// Inside a Distrobox *without* host-exec: refuse to run anything.
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvGuard {
    pub mode: EnvMode,
    pub message: Option<String>,
    /// `distrobox --version` succeeded (today's `is_distrobox_installed`).
    pub distrobox_installed: bool,
}

/// Single implementation of the precedence chain that was written twice in the
/// pre-T1 tree (`AppState::new` and `is_distrobox_installed`, both since deleted).
pub fn detect(runner: &CommandRunner) -> (CommandRunner, EnvGuard);
```

Precedence is preserved exactly — `/.flatpak-info` → `flatpak-spawn --host`; else
`is_distrobox_container() && has_distrobox_host_exec()` → `distrobox-host-exec`;
else native — with `has_distrobox_host_exec` widened to scan `$PATH` as the Dart
guard already does (§6.4-B6). The `Blocked` variant carries the same user-facing
message text as `environment_guard_io.dart` so the wording is preserved.

---

## 3. State struct and live task output

### 3.1 DRAFT: the application model

```rust
// app/src/app.rs
pub struct App {
    // ---- libcosmic-managed ----
    core: cosmic::Core,
    nav: cosmic::widget::nav_bar::Model,

    // ---- shared backend handle ----
    /// Owns the CommandRunner (already env-mapped), the `Distrobox`, and the
    /// task registry. Cloned into subscription builders via `BACKEND` (§3.4).
    backend: Arc<Backend>,

    // ---- routing ----
    page: Page,

    // ================= domains =================
    containers: ContainerList,          // { containers: Vec<ContainerInfo>, skipped: Vec<ParseIssue> } — see D27
    selected_container: Option<ContainerInfo>,
    container_filter: String,
    pending_confirm: Option<Confirm>,

    apps: Vec<AppInfo>,
    exported_binaries: Vec<ExportedBinary>,

    images: Vec<String>,

    packages: Vec<PackageInfo>,
    package_search: Vec<PackageInfo>,
    package_query: String,
    package_manager: Option<PackageManager>,   // typed; was a bare String

    snapshots: Vec<SnapshotInfo>,
    snapshot_filter: String,

    stats: Option<ContainerStats>,
    /// Guard against overlapping `podman stats --no-stream` calls.
    stats_in_flight: bool,

    tasks: BTreeMap<TaskId, TaskView>,
    focused_task: Option<TaskId>,

    env: EnvGuard,

    terminals: Vec<Terminal>,
    selected_terminal: Option<String>,   // `Terminal::full_command_id()`

    config: Config,
    /// Handle for writes; `None` if the config dir is unavailable (degrade, don't crash).
    config_handle: Option<cosmic_config::Config>,

    // ---- UI ----
    loading: Loading,        // small struct of bools, mirrors the 8 Dart `isLoadingX`
    toast: Option<Toast>,
}

/// The UI-side mirror of a core `Task`. `output` is a *view* buffer, appended from
/// `TaskMsg::Output`, capped at `MAX_TASK_OUTPUT_LINES`, and dropped when core
/// reports expiry — the authoritative buffer lives in core (§3.3).
pub struct TaskView {
    pub label: String,
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    pub started_at: std::time::Instant,
}
```

`Loading` is a plain struct of bools (`containers, apps, images, packages,
snapshots, stats, binaries, action`) so each domain's spinner is independent, exactly
as the eight Dart getters model it. `running_containers_count` /
`stopped_containers_count` become methods derived from
`containers.iter().filter(|c| matches!(c.status, Status::Up))`.

### 3.2 Why the domains are shaped this way

- **Every `Result` stays in the message.** The Dart provider stores `errorMessage`
  as a `String` and loses the error kind. Carrying `Result<_, CoreError>` lets the
  view distinguish "distrobox not installed" (offer setup guidance) from "podman
  socket permission denied" (offer a retry/diagnostic) — impossible today.
- **`snapshot_filter` is UI state, not a re-query.** Today `loadSnapshots({filterPrefix})`
  re-runs the backend; with the full list in state, filtering is local.
- **`stats_in_flight`** exists because `podman stats --no-stream` on a slow container
  can outlive a 1 s refresh tick and pile up.

### 3.3 Task registry in core

```rust
// core/src/task_runtime.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, serde::Serialize, serde::Deserialize)]
pub struct TaskId(pub Uuid);           // typed — see §4.2

pub enum TaskEvent {
    Output(String),                     // one complete line (post-B2 fix)
    Finished { success: bool },
}

pub struct Task {
    pub id: TaskId,
    pub label: String,
    /// Replay buffer for late subscribers, ring-capped at MAX_TASK_OUTPUT_LINES.
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    pub completed_at: Option<Instant>,
    tx: Option<async_channel::Sender<TaskEvent>>,   // async-channel is already a dep
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

pub const MAX_TASK_OUTPUT_LINES: usize = 500;      // preserved
pub const COMPLETED_TASK_TTL: Duration = Duration::from_secs(600);   // preserved
```

`TaskRegistry` owns `RwLock<HashMap<TaskId, Task>>` and offers:
`insert`, `subscribe(id) -> Option<Receiver<TaskEvent>>` (replays `output`, then
yields live events), `cancel(id) -> bool`, `sweep_expired() -> Vec<TaskId>`,
`is_running(id) -> bool`.

**TTL semantics preserved and improved.** Today `finish_task` (`api.rs:56-76`) does
the `retain` sweep as a side effect of finishing, so a registry with no finishing
tasks never sweeps, and the sweep runs while holding the write lock. Here:
`finish_task` marks completion, emits `TaskEvent::Finished`, drops the sender, and
records `completed_at`; a separate `sweep_expired()` is driven by a
`cosmic::iced::time` subscription tick in the app (see §3.4), which is also what
produces `TaskMsg::Expired`. **600 s TTL and 500-line cap are unchanged** — those
are the observable behaviours the Dart side has today.

**Cancel semantics fixed** (§0.3-B6): `cancel(id)` fires the `oneshot`, then
`run_child_task` `tokio::select!`s between `child.wait()` and the cancel signal and
calls `child.kill()` on cancel. `JoinHandle::abort` is gone — it silently orphaned
the process.

### 3.4 Live task output: mpsc → iced `Subscription`

Replacement for the FRB `StreamSink` + `broadcast` (`api.rs:46-54`, `:210-240`).

The blocking constraint is §0.1: **`Subscription::run_with`'s builder is a `fn`
pointer**, so it cannot capture the registry or a receiver. libcosmic solves the same
problem the same way — `src/widget/text_context_menu.rs:150` parking a sender in a
`OnceLock`, `src/dbus_activation.rs:17` using `run_with(TypeId::of::<T>())`. We
follow that established pattern:

```rust
// app/src/app.rs
static BACKEND: std::sync::OnceLock<Arc<Backend>> = std::sync::OnceLock::new();

impl cosmic::Application for App {
    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let (runner, env) = core::env::detect(&CommandRunner::new_real());
        let backend = Arc::new(Backend::new(runner));
        // `init` runs before any subscription is polled, so this is race-free.
        let _ = BACKEND.set(backend.clone());
        let (app, _) = (App { /* … */ backend, env /* … */ }, Task::none());
        (app, Task::batch(vec![done(Message::Containers(ContainerMsg::RefreshRequested))]))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            // (a) per-task output streams
            Subscription::batch(self.tasks.keys().copied().map(task_output_subscription)),
            // (b) TTL sweep — replaces the implicit `retain` in `finish_task`
            cosmic::iced::time::every(Duration::from_secs(30))
                .map(|_| Message::Tasks(TaskMsg::SweepExpired)),
            // (c) reactive config reload
            self.watch_config::<Config>(CONFIG_ID)
                .map(|u| Message::Config(ConfigMsg::Updated(u))),
        ])
    }
}

/// Plain `fn` — no captures, per `Subscription::run_with`'s signature.
fn task_output_subscription(id: TaskId) -> Subscription<Message> {
    Subscription::run_with(id, |id: &TaskId| {
        let id = *id;
        // `Some` because `subscription()` only iterates tasks we registered.
        let rx = BACKEND.get().and_then(|b| b.tasks().subscribe(id));
        futures::stream::unfold(rx, move |rx| async move {
            let mut rx = rx?;
            match rx.recv().await {
                Ok(TaskEvent::Output(line)) =>
                    Some((Message::Tasks(TaskMsg::Output { id, lines: vec![line] }), Some(rx))),
                Ok(TaskEvent::Finished { success }) =>
                    Some((Message::Tasks(TaskMsg::Completed { id, success }), None)),
                Err(_) => None,
            }
        })
    })
}
```

Notes on why this shape:

- **`data: D` is the `TaskId`**, so iced keys each subscription by task. When a task
  leaves `self.tasks`, the subscription is torn down automatically — no manual
  unsubscribe, which is what the FRB `StreamSink` needed (`sink.add()` returning
  `Err` was the only signal — `api.rs:224-237`).
- **`subscribe()` performs the replay** of `Task.output` before yielding live
  events, which is exactly what `stream_task_output` did by hand
  (`api.rs:211-227`: clone `output`, then `tx.subscribe()`), but without the
  subscribe-after-snapshot race the current code has (a line pushed between the
  `read()` and the `subscribe()` is lost today).
- **Batching hook.** `TaskMsg::Output { lines: Vec<String> }` already takes a `Vec`
  so a future coalescing step (drain the channel and emit one message per frame) is
  a change inside `task_output_subscription` only, not to the enum. The 500-line
  buffer means worst-case message volume is bounded already.
- `Subscription::run_with` requires `D: Hash`, hence `TaskId: Hash` (§4.2).

---

## 4. `spawn_task`, `TaskId`, and the error type

### 4.1 The eight copy-pasted bodies, collapsed

The eight pre-T1 `rust/src/api.rs` bodies — `create_container`, `upgrade_container`,
`clone_container`, `install_package`, `remove_package`, `restore_from_snapshot`,
`export_container_to_file`, `import_container_from_file` — are the same ~40 lines with
four substitutions: the label, the `"Error starting …"` prefix, the success string,
and the failure string. (They are named rather than line-numbered because `api.rs`
itself was deleted in T14; what survives is `core/src/service.rs::spawn_task`.) Two
other differences matter: four of them `await` a `Result<Box<dyn Child + Send>, Error>`
(`create`/`clone_from`/`restore_from_snapshot`), and four are **synchronous** —
`Distrobox::upgrade`, `install_package`, `remove_package`, `export_container` and
`import_container` in `core/src/backends/distrobox/distrobox.rs`.

```rust
// core/src/service.rs
pub struct SpawnTask<'a> {
    pub label: String,
    /// Prefix for a start failure, e.g. `"Error starting upgrade: "`.
    pub start_error_prefix: &'a str,
    pub success_message: &'static str,
    pub failure_message: &'static str,
}

/// The single task-spawning path. Sync call sites pass a closure whose body is a
/// bare `async { … }` block.
///
/// # Runtime
/// Internally `tokio::spawn`s the runner, so this **must** be awaited with the
/// executor's tokio runtime entered — i.e. from a `Task`/`Subscription` future,
/// never from a bare thread or a `smol`-driven test body. See
/// docs/migration/architecture.md §0.2.
pub async fn spawn_task<F, Fut>(
    registry: &TaskRegistry,
    params: SpawnTask<'_>,
    start: F,
) -> Result<TaskId, CoreError>
where
    F:   FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<Box<dyn Child + Send>, Error>> + Send,
{
    let id = TaskId::new();
    let (tx, _) = async_channel::bounded::<TaskEvent>(100);   // preserves today's cap-100
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

    let label = params.label.clone();
    let handle = tokio::spawn({
        let registry = registry.clone();
        let tx = tx.clone();
        async move {
            let result = async {
                match start().await {
                    Ok(child) => run_child_task(
                        id, registry.clone(), tx.clone(), child, cancel_rx,
                        params.success_message, params.failure_message,
                    ).await,
                    Err(e) => {
                        let _ = tx.send(TaskEvent::Output(
                            format!("{}{}", params.start_error_prefix, e))).await;
                        Err(e)
                    }
                }
            }.await;
            finish_task(&registry, id, result.is_ok());
        }
    });

    registry.insert(Task { id, label, /* … */ handle: Some(handle), cancel: Some(cancel_tx) });
    Ok(id)
}
```

Call sites become one line each, e.g.:

```rust
// was api.rs:262-301 (40 lines)
pub async fn upgrade_container(&self, name: &str) -> Result<TaskId, CoreError> {
    spawn_task(self.tasks(), SpawnTask {
        label: format!("Upgrade {name}"),
        start_error_prefix: "Error starting upgrade: ",
        success_message: "Upgrade completed successfully",
        failure_message: "Upgrade failed",
    }, || async { self.distrobox.upgrade(name) }).await
}
```

`run_child_task` keeps its current contract (both streams drained concurrently,
success/failure line appended — `api.rs:95-130`) with three changes: the cancel
`select!` (§3.3), the line-buffered reader (`stream_reader_to_task_output` →
`read_lines_to_registry`, §6.4-B2), and it now emits `TaskEvent::Output`/`Finished` over the
channel instead of reaching into the registry per line (today `push_task_output`
takes the registry write lock **once per 1024-byte chunk**, `api.rs:46-54`).

### 4.2 `TaskId`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct TaskId(Uuid);
impl TaskId { pub fn new() -> Self { Self(Uuid::new_v4()) } }
impl std::fmt::Display for TaskId { /* … */ }
```

Replaces the `String` used at `api.rs:170`, `:263`, `:305`, `:413`, `:419`, `:429`,
etc. Benefits that are concrete, not aesthetic: it satisfies `Hash` for
`Subscription::run_with`; it makes `TaskMsg::Output { id, .. }` impossible to
confuse with a container name (both are `String` today — `cancel_task(task_id: String)`
vs `stop_container(name: String)`); and it stops the `task_id.clone()` shuttle
(the current bodies clone it 4× each, `api.rs:170,176,178,198`).

### 4.3 `CoreError` — thiserror at the UI boundary

`thiserror` is already a dependency (`rust/Cargo.toml`), and the backend already
uses typed errors — `distrobox::Error` (`core/src/backends/distrobox/distrobox.rs`)
and its `CommandFailed { exit_code, command, stderr }` variant. Only
`api.rs` flattens them into `anyhow` (35 × `map_err(|e| anyhow::anyhow!(e))`),
which is precisely the information the UI needs and cannot get.

```rust
// core/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("distrobox command failed ({exit_code:?}): {stderr}")]
    CommandFailed { exit_code: Option<i32>, command: String, stderr: String },

    #[error("could not start {command}: {source}")]
    Spawn { command: String, #[source] source: std::io::Error },

    #[error("could not parse output: {0}")]
    ParseOutput(String),

    #[error("invalid container name {name:?}: {reason}")]
    InvalidName { name: String, reason: String },

    #[error("task {0} not found")]
    TaskNotFound(TaskId),

    #[error("running inside a Distrobox container without distrobox-host-exec")]
    BlockedEnvironment,

    #[error("configuration unavailable: {0}")]
    Config(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

`impl From<distrobox::Error> for CoreError` and `From<Error> for CoreError` make the
`?` operator carry the kind through. `CommandFailed` deliberately keeps `stderr` and
`exit_code` — the Flutter UI showed only a string, and the most common real failure
(`podman: permission denied` on a rootless socket) is indistinguishable from
`distrobox: no such container` at the current UI.

`anyhow` stays as a dependency but is confined to core internals; `CoreError` is what
crosses into `app`. Section §2.3's `Message` variants carry `Result<_, CoreError>`
everywhere, so the boundary is enforced by the type system rather than convention.

---

## 5. Config persistence

### 5.1 Decision: `cosmic-config`, not `dirs` + `serde`

`cosmic-config` is **already in the dependency graph** — libcosmic takes it
unconditionally on Linux with `features = ["dbus"]` (`libcosmic/Cargo.toml:174`), and
re-exports it as `cosmic::cosmic_config` (`src/lib.rs:126`). So this is not a new
dependency decision; the question is only whether to *use* it or bolt on a parallel
`dirs`+`serde` store that duplicates what libcosmic already ships.

Verified reasons to use it:

1. **Flatpak path handling already exists and is correct.**
   `cosmic-config/src/lib.rs:15-36` — `get_config_dir()` checks `FLATPAK_ID`, then
   prefers `HOST_XDG_CONFIG_HOME`, then falls back to `$HOME/.config`, then
   `dirs::config_dir()`. A hand-rolled store would put config at
   `$HOME/.config/…` under Flatpak *only if* the sandbox is granted
   `--filesystem=xdg-config`; the sandbox's own `$XDG_CONFIG_HOME` differs. This
   question is already answered in-tree.
2. **Reactive reload for free.** `ApplicationExt::watch_config::<T>(id)`
   (`src/app/mod.rs:551`) returns `iced::Subscription<cosmic_config::Update<T>>`,
   built on `config_subscription` (`cosmic-config/src/subscription.rs:22`), which
   uses a `notify::RecommendedWatcher`. External edits (a text editor, another COSMIC
   app) reload live. Wiring (c) in §3.4 is three lines.
3. **Layered writes.** `ConfigGet::get` = user override → system default
   (`lib.rs:407-413`); `Config::system` looks in `/usr/share/cosmic/<id>/v1`
   (`system_inner` → `xdg::BaseDirectories::with_prefix("cosmic").find_data_file`).
   Ships a sane default without a config file existing.
4. **Atomic writes** (`atomicwrites` dep) and **backward-compatible versioning**
   (`Config::new(name, version)` chains to `version - 1`, `lib.rs:206-211`), so a
   future v2 can migrate v1.
5. **Flat per-key storage avoids one big blob.** `ConfigSet::set(key, value)`
   (`lib.rs:447`) writes `…/v1/<key>` via `key_path` (`lib.rs:396-401`). A corrupt
   `selected_terminal` cannot take the window geometry down with it.

The `dbus` feature is already enabled by libcosmic and pulls `zbus` +
`cosmic-settings-daemon`; there is no marginal cost to using the crate, and no way to
avoid the dependency by choosing option B.

**Recorded alternative** (if the devil's advocate rejects this): `dirs` 5.0 is
already a direct dependency, `serde` is already a dependency, `toml` is already a
dependency, so `dirs::config_dir()/gosh-distrobox-manager/config.toml` +
`#[serde(default)]` is ~40 lines with no Flatpak story (must hand-roll the
`FLATPAK_ID`/`HOST_XDG_CONFIG_HOME` logic of §5.1-1) and no watcher. It is smaller and
has no zbus surface. Open question **Q5**.

### 5.2 Layout on disk

Config id: **`io.github.gosh_distrobox_manager`** and version **`1`** — the same
string as `APP_ID`, the gschema id, the metainfo `<id>`, the `.desktop` `Icon=`, and
the D-Bus service name.

| Context | Path |
|---|---|
| Native (XDG) | `$XDG_CONFIG_HOME/cosmic/io.github.gosh_distrobox_manager/v1/<key>` — one file per key (`key_path`, `lib.rs:396`) |
| Fallback | `~/.config/cosmic/io.github.gosh_distrobox_manager/v1/<key>` |
| Flatpak, `HOST_XDG_CONFIG_HOME` set | `$HOST_XDG_CONFIG_HOME/cosmic/io.github.gosh_distrobox_manager/v1/<key>` — **this is the branch we want**, so a Flatpak build driving a host `distrobox` reads host config. Requires `--filesystem=xdg-config` in the manifest |
| Flatpak, no host config | `$HOME/.config/…` (sandbox-local; correct but divergent — a config written here is invisible to a native install) |
| System defaults | `/usr/share/cosmic/io.github.gosh_distrobox_manager/v1/<key>` |
| State (`Config::new_state`, `lib.rs:291` + `get_state_dir`) | `$XDG_STATE_HOME/cosmic/…` — **not used**: everything here is user preference, not volatile runtime state |

The `Config` struct uses `#[derive(cosmic_config::cosmic_config_derive::CosmicConfigEntry)]`
(the derive crate is re-exported as `pub use cosmic_config_derive;` at
`cosmic-config/src/lib.rs:68`; the trait is `pub trait CosmicConfigEntry` at `:494`,
and libcosmic's own `src/config/mod.rs` imports it the same way). `write_entry`
(`cosmic-config-derive/src/lib.rs:185`) / `get_entry` (`:191`) serialize each field to
its own top-level key. **Q6 is CLOSED** (REVIEW.md §C ARCH-Q6): the derive uses the
Rust field name verbatim — it emits
`ConfigSet::set(&tx, stringify!(#field_name), &self.#field_name)?` with the matching
`ConfigGet::get::<#field_type>(config, stringify!(#field_name))`, and `stringify!`
takes the identifier as written, so a field `custom_terminals` is the key
`custom_terminals`, snake_case, no renaming. The §5.3 table below is therefore correct
as written and its "verify before implementing row 7" gate is satisfied. The one
consequence to carry into the table: the legacy gschema keys are **kebab-case** while
cosmic-config keys are snake_case, so the import is a rename, not a copy (§5.3).

### 5.3 Keys

Legacy keys from `rust/data/io.github.gosh_distrobox_manager.gschema.xml` are read by
**no code** (§0.4) but define the intent. Mapping, plus new keys. **The legacy import is
a rename, not a copy**: the gschema keys are kebab-case (`selected-terminal`,
`window-width`, `window-height`, `distrobox-executable`) while cosmic-config keys are the
snake_case Rust field names (§5.2, Q6 closed), so every imported key must be translated
by name:

| Key | Type | Source | Notes |
|---|---|---|---|
| `selected_terminal` | `String` | gschema `selected-terminal` (default `'gnome-terminal'`) | **Value semantics change**: today it is a bare program name; with `supported_terminals.rs` revived (§6.4-T2b) the right identity is `Terminal::full_command_id()` (program + `extra_args`), which is the crate's own dedup key (`supported_terminals.rs::full_command_id`). A legacy value of `"gnome-terminal"` still matches the built-in entry by `program` via `supported_terminals.rs::terminal_by_program` — the migration reads the legacy value, matches on program, and rewrites as a `full_command_id`. **Open question Q7** |
| `window_width` | `i32` | gschema `window-width` (default 900) | written from `on_window_resize` (`src/app/mod.rs:455`), debounced (a resize emits many events; write on a 500 ms trailing edge, not per event) |
| `window_height` | `i32` | gschema `window-height` (default 700) | same |
| `distrobox_source` | `String` | gschema `distrobox-executable` (default `'host'`) | values `"host"` \| `"bundled"`. Today unread; wired to env detection — `"host"` means resolve `distrobox` on the *host* (i.e. keep the `flatpak-spawn --host` / `distrobox-host-exec` mapping), `"bundled"` means use whatever is on the sandbox's own `PATH`. **Open question Q8** — "bundled" implies shipping a distrobox in the Flatpak, which nothing does today |
| `custom_terminals` | `Vec<Terminal>` | new | persists `TerminalRepository::save_terminal` / `delete_terminal`, which had **no backing store at all** despite returning `anyhow::Result`. `Terminal` already derived `Serialize`/`Deserialize`, so this was a storage decision only. **As built (T12):** the store sits one level up in `AppConfig::custom_terminals` (§6.2) |
| `refresh_interval_secs` | `u32` | new (default `5`) | drives the containers/stats poll + the 30 s sweep tick |
| `confirm_destructive_actions` | `bool` | new (default `true`) | gates the confirm dialog for remove/stop-all/rmi |
| `snapshot_prefix` | `String` | new (default `"gdm"`) | default prefix for `create_snapshot` names, feeds `list_snapshots(filter_prefix)` |
| `default_export_dir` | `String` | new (default `$XDG_DOWNLOAD_DIR` or `$HOME`) | `FileDialog` start dir for #32/#33 |
| `show_skipped_lines` | `bool` | new (default `false`) | surfaces `ContainerList.skipped` from §6.4-B3 |

No secret or credential is stored, so no keyring/portal consideration is needed.

### 5.4 What happens to the gschema and the `.service.in`

- `data/io.github.gosh_distrobox_manager.gschema.xml` — **delete** once §5.3 keys are
  wired. Two consumers exist in the current repo: none in code, and none in
  packaging (the `.spec` does not install it). Keep the file until the config lands
  so the intent is diffable, then remove it and drop it from any Flatpak manifest.
- `.desktop.in`, `.metainfo.xml.in` — keep, but fix stale content:
  `Keywords=GTK;` → `Keywords=Distrobox;Container;` and add
  `X-GNOME-UsesNotifications`/`StartupWMClass` as appropriate. These are T1-adjacent
  data, not code.
- `.service.in` — keep. It pairs with `DBusActivatable=true` in the `.desktop` file
  and with libcosmic's `single-instance` feature (`Cargo.toml`: `single-instance =
  ["iced_winit/single-instance", "zbus/blocking-api", "ron"]`), which provides
  `run_single_instance` and `Application::dbus_activation`
  (`src/app/mod.rs:500-513`). **Open question Q9** — whether to enable
  `single-instance`; it is a real feature win (one running instance, activation
  forwards to it) but adds a feature flag and a zbus dependency to the app crate.

---

## 6. Backend reuse plan

### 6.1 T1 — verbatim (move only)

Zero `flutter_rust_bridge` imports. Verified:
`grep -rn 'flutter_rust_bridge\|frb_generated' rust/src/{backends,fakers,models}` →
**0 hits** — the whole of `backends/` including its `mod.rs` files, the whole of
`fakers/`, the whole of `models/`. These move in S2/S3/S4 with a `git mv` and at most a
module-path fix:

`fakers/command.rs` (259), `fakers/command_runner.rs` (582, incl. the
`NullCommandRunner` / `StubChild` test infrastructure the whole suite depends on),
`fakers/output_tracker.rs` (158), `fakers/host_env.rs` (63),
`fakers/mod.rs` (9),
`backends/flatpak.rs` (67, has tests), `backends/host_exec.rs` (65, has tests),
`backends/desktop_file.rs` (167), `models/known_distros.rs` (141),
`backends/distrobox/command.rs` (11), `backends/distrobox/distrobox.rs` (2139).

`distrobox.rs` is T1 in *structure* but receives the §6.4 fixes — those are
behaviour changes, not port work.

**That list is all of `fakers/`, all of `models/known_distros.rs` and all of the
`distrobox/` module — but it is not all of `backends/`.** Four more backend files are
equally FRB-free and move in the same `git mv`, and they are **T2 (adapt), not T1
(verbatim)** — §6.2 owns them, and none of them is mentioned above:

| File | Lines | Why T2, not T1 |
|---|---|---|
| `backends/container_runtime.rs` | 161 | routed-through by §6.2's runtime helper (Q11 **closed** — helper, not trait) |
| `backends/docker.rs` | 95 | the podman→docker fallback collapses into §6.2 |
| `backends/podman.rs` | 138 | same; `PodmanEventStream` deferred (Q12). Its `map_docker_to_podman` (`:17`, used at `:87`) is **not** the fallback primitive — it maps one way and cannot choose a target |
| `backends/supported_terminals.rs` | 312 | revived, and its store moves to `cosmic-config` (ARCH-Q5) |
| **subtotal** | **599** | |

This is the reconciliation of the three statements that read as contradictory: §0.4
names only the modules that need no change at all; these 599 lines are unmodified *as
moved* and are adapted by §6.2 afterwards; and `packaging.md §2.2`'s "zero changes to
`backends/`" is true of the T1 set above only — read it as "the T1 set moves
unchanged", which is what S1–S4 actually need.

Counts, with every line count taken from the tree at `e5436a0` (23 `.rs` files under
`rust/src`, 7,431 lines total): T1 verbatim **3,661**; + the **599** T2 lines above =
4,260; + the remaining module files `backends/mod.rs` (10), `distrobox/mod.rs` (4),
`models/mod.rs` (5) and T2's `models/task.rs` (42) + `app_state.rs` (36) + `lib.rs` (7)
= 4,364 lines of hand-written non-FRB Rust; + T3's `api.rs` (713) +
`frb_generated.rs` (2,354) = 7,431.

**The one hard rule survives:** never `std::process::Command`; always `CommandRunner`
— because the env dispatch (Flatpak/host-exec) is implemented as a `map_cmd`
transform on the runner (`flatpak.rs::map_flatpak_spawn_host`,
`host_exec.rs::map_distrobox_host_exec`). Every `Command` in `distrobox.rs` already
goes through `self.cmd_runner` via `cmd_spawn`/`cmd_output`, so a bypass would
silently run inside the sandbox. Worth a
`#![deny]`-style guard: a clippy `disallowed-methods` lint on
`std::process::Command::new` in `core/` as a regression net. **Open question Q10.**

### 6.2 T2 — adapt

**`models/task.rs` (42 lines).** Replace `tx: Option<broadcast::Sender<String>>` with
`tx: Option<async_channel::Sender<TaskEvent>>`; `handle: JoinHandle<anyhow::Result<()>>`
→ `handle: Option<JoinHandle<()>>` (taken on completion so the registry does not hold
finished handles); add `cancel: Option<oneshot::Sender<()>>` and `TaskId`.
`push_output`'s ring-buffer logic is preserved verbatim — it is the
`MAX_TASK_OUTPUT_LINES` cap (`core/src/tasks.rs`).

**`app_state.rs` (36 lines) + `api.rs::is_distrobox_installed`.** The `LazyLock`
global goes away: state moves into `App`/`Backend`, constructed once in `init()`. The
duplicated precedence chain collapses into `core::env::detect` (§2.4). This is the
single highest-value dedupe in the port: today the two copies can disagree, and
`is_distrobox_installed` builds a throwaway `CommandRunner` + `Distrobox` per call
(`api.rs:144-158`) — a new process-mapping path and a fresh `Distrobox` on **every**
UI refresh.

**`backends/supported_terminals.rs` (312 lines) — revive.** `backends/mod.rs:8`
declares `pub mod supported_terminals;` but nothing in Rust or Dart constructs a
`TerminalRepository`; `Terminal`/`SUPPORTED_TERMINALS`/`FLATPAK_TERMINAL_CANDIDATES`
are dead weight. The Dart UI's `getEnterCommand` (`app_state.dart:354`) fetches argv
and hands it to the platform — terminal selection lives in the UI layer, and the
`selected-terminal` gschema key is unread. Plan:

```rust
// core/src/backends/supported_terminals.rs
impl Terminal {
    /// Build `program [extra_args…] separator_arg <enter_cmd args>`.
    pub fn launch(
        &self,
        runner: &CommandRunner,
        enter: &Command,            // Distrobox::enter_cmd(name)
    ) -> Result<Box<dyn Child + Send>, Error>;
}
```

spawned through `runner` so the Flatpak/host-exec mapping applies to the *terminal*
too (today's Dart path launches the terminal from inside the sandbox). `TerminalRepository`
gains a `Config`-backed store (§5.3 `custom_terminals`), which is what its
`save_terminal`/`delete_terminal` signatures already promise; `merge_flatpak_terminals`
(`:203`) and `is_read_only` (`:224`) finally get callers.

**As built (T12), the store sits one level up.** The custom terminals live in
`AppConfig::custom_terminals` (cosmic-config), and `TerminalRepository::with_customs`
folds them in — one reader (the D11 host-side import probe) and one writer (the config
watch) for the list, both through the env-mapped runner. `save_terminal` /
`delete_terminal` / the JSON path were **deleted rather than re-backed**: D11 removes
`distroshelf-terminals.json`, and no parity row in ux.md lets a user add or remove a
terminal, so re-backing them would have meant keeping three mutators with no producer.
The §5.3 plan above promised that this task would give `merge_flatpak_terminals`
(`:203`) and `is_read_only` (`:224`) their first callers; **it did not.** `all_terminals`
has one (`Backend::terminals`), but `is_read_only`, `terminal_by_name`,
`terminal_by_program`, `default_terminal`, `merge_flatpak_terminals` and
`fetch_flatpak_terminals` are all still callerless — no parity row renders a read-only
badge or wires a Flatpak discovery pass, so nothing in T12 gave them a producer. They
are left in place rather than deleted here (they predate this task and are candidates
for the T14 deletion pass alongside S7/S8); the record above is corrected so the
"finally get callers" line is not read as done. **T14 did not delete them.** The
deletion pass (S8) was scoped to the Flutter tree, the FRB layer and the dead cluster
that fed it; these are backend helpers with no caller, which is a different problem
with a different fix (wire or drop, decided per symbol). They remain in the tree as of
`92d0eb0` and are carried as I24 in PLAN.md rather than silently dropped.

`resolve_terminal` is the one piece this task made live: `TerminalMsg::OpenRequested`
seeds the picker from the persisted `selected_terminal` through it (#171 — the saved
preference was written and displayed but never applied). Its precedence is
customs-before-built-ins on a bare-program value; it returns `None` rather than
first-available, and `TerminalState::selected` owns that fallback, because silently
launching a terminal the user did not pick is worse than showing none.

**`backends/container_runtime.rs` (54) + `docker.rs` (95) + `podman.rs` (138).**
Currently dead — nothing calls `get_container_runtime` (`container_runtime.rs:83`),
while `distrobox.rs` re-implements the podman→docker fallback inline **six times**
(§0.3 item 7, which inherited T13/D27's correction of this count — the "seven" first
written here included two *streaming* sites that never had a docker branch, and missed
`start`, which did). Decision: **wire it, don't delete it**, because `podman.rs` carries value
the inline code does not:

- `PodmanEventStream` (`podman.rs:66`) parses `podman events` JSON and
  `PodmanEvent::is_distrobox` / `is_container_event` filter to
  distrobox container start/stop/exit. That is exactly the live container-state
  source a COSMIC app should subscribe to, and it is a `Stream` — a natural
  `Subscription::run`. It replaces the Dart side's poll-only `refresh()`.
- `ContainerRuntime::usage()` returns the `Usage` struct
  (`container_runtime.rs::Usage`) whose `Deserialize` aliases
  (`mem_usage`/`MemUsage`, `CPU`, `NetIO`, …) already handle both podman's and
  docker's `stats` field names. `get_container_stats`
  (`distrobox.rs::get_container_stats`) currently hand-parses the same data with
  `split('\t')` + `trim_end_matches('%')` + `unwrap_or(0.0)`.

Shape of the change: extend `ContainerRuntime` with the operations the six inline
sites need (`start`, `commit`, `rmi`, `images`, `stats_raw`), implement them once in
`Podman`/`Docker`, and add `Distrobox::runtime() -> Result<Arc<dyn ContainerRuntime>>`
memoized at construction. `get_container_runtime` (`:83`) already prefers Podman and
falls back to Docker with the rationale documented (`// Prefer Podman when both are
available because Podman is rootless by default`) — that reasoning is currently lost
in the inline copies. **Q11 — answered by T13 (D27): the helper, not the trait.**
`Distrobox::runtime_output(cmd, Fallback)` + `container_runtime::retarget` carry the
fallback and `ContainerRuntime` stays at three methods. Two reasons the trait branch
lost: all six sites are plain output commands over an argv `Distrobox` already
builds, so the trait would have re-encoded the same argv in two more places; and the
`#[async_trait(?Send)]` objection below is real, so growing the trait would have forced
a `Send` conversion for operations that never needed one. `get_container_runtime`
remains callerless — it cannot serve these sites (no run method, `?Send`, returns
`Option`, and drops the runner). **T14 did not delete it either** (see the note above);
the conclusion that it *should* be deleted stands, and it is filed as I24 so the
judgement survives the task that declined to act on it.

`podman.rs::map_docker_to_podman` (`:17`) is a `Command`-rewriting helper — it maps
the **program field** `docker → podman`, and only that. It is **not** the primitive
for the helper above, for a sharper reason than the one first written here: it points
one way, so it cannot produce a docker retry at all. `retarget` must *choose* the
target program, which is precisely what a `docker → podman` map cannot do (and why
routing a retry through a `Podman`-constructed runner silently rewrites it straight
back). It touches no arguments, so it also cannot rename a container the way an
earlier draft of this note claimed.

### 6.3 T3 — rewrite

- `api.rs` (713 lines) — **deleted** (S7). Its 35 functions become
  `core::service::Backend` methods (S6), with the `#[frb]`/`StreamSink`/`#[frb(init)]`
  surface removed, `anyhow` replaced by `CoreError` (§4.3), `String` task ids by
  `TaskId` (§4.2), and the 8 bodies collapsed by `spawn_task` (§4.1).
- `frb_generated.rs` (2354 lines) — deleted. `flutter_rust_bridge.yaml` — deleted.
- `lib.rs` (7 lines) — keep `pub mod backends/fakers/models`, add
  `pub mod env/service/task_runtime/error`, drop `pub mod api` + `mod frb_generated`.
  Note `pub use distrobox::*;` (`backends/mod.rs:10`) is the crate's real public
  re-export surface and is what S3's `dto.rs` should mirror.
- `lib/src/rust/*.dart` (5 files, incl. a `freezed` model file) — deleted with the
  Flutter tree.

### 6.4 The fixes, with where each lands

**The line numbers in this table locate the code each fix *replaced*, not the code as
it stands.** They are measurements of the pre-fix tree (`e5436a0`, the Phase-1
baseline), and every one of them was invalidated by the fix itself. Each row's
**As built** sentence names the symbol that carries the fix today. The trailing *File*
column names the file the fix was assigned to, which is a **plan target and not always a
current path**: the three task-runtime rows (`B2`, `B8`, `B9`) name
`core/src/task_runtime.rs`, a planned module that landed as `core/src/tasks.rs` — each
of those rows now states its current home. This is the same convention
`DECISIONS.md` adopted for the five Rust-side sites that cited this document by line
number — a citation that cannot outlive the change it describes is better written as a
name.

| ID | Fix | File |
|---|---|---|
| **B1** | **`emerge` + one package-manager table.** Replace the three divergent detection scripts (`distrobox.rs:1215`, `:1316`, `:1348`) with one `detect_package_manager -> Result<PackageManager, CoreError>` plus one verb table. Extend `PackageManager` with `Xbps`, `Emerge` (`known_distros.rs:45`); fix the `gentoo`→`Unknown` mapping (`:19`) and `void`→`Unknown` (`:32`). Verbs: apt `apt-get install -y`/`remove -y`; dnf/yum/pacman/zypper/apk as today; xbps `xbps-install -y`/`xbps-remove -y` (correct today); **emerge `emerge --ask=n <pkg>` / `emerge --unmerge <pkg>`** (note `--ask=n`, not `-y` — emerge has no `-y`). `Unknown` only when genuinely nothing is found, and then the UI offers manual command entry instead of an error toast | `distrobox.rs`, `known_distros.rs` |
| **B2** | **Line-buffered readers.** Rewrite `stream_reader_to_task_output` (`api.rs:78`) to accumulate into a `Vec<u8>` and emit complete records split on `\n` **and** `\r` (apt/dnf/pacman/emerge progress bars are `\r`-driven), stripping `\r\n`, dropping empties, holding the incomplete tail across reads, and flushing on EOF. Because both stdout and stderr feed the same task, keep the existing interleaving semantics; a per-stream buffer prevents a `\r`-less stderr chunk from ever appearing. **As built (T5):** `read_lines_to_registry` (`core/src/tasks.rs`) | `task_runtime.rs` |
| **B3** | **Tolerant list parsing.** `Distrobox::list` returns `Err` and discards everything on one bad row. Change `ContainerInfo::from_str` failures to `warn!` + collect, and return `ContainerList { containers: Vec<ContainerInfo>, skipped: Vec<ParseIssue> }`. Apply the same to `list_snapshots`, `list_installed_packages`, `get_exported_binaries`, `list_apps`. `ParseIssue` carries the raw line and the parse error so the UI can show "3 rows skipped". Also fixes the inconsistency where `list()` fails hard but its peers silently skip. **As built (T13, D27):** `containers` is a `Vec`, not the `BTreeMap` this row originally specified — the map is erased at the `Backend` boundary anyway (`service.rs` already did `into_values()`), so the container type was never observable; the `Vec` preserves the name-sort the map happened to provide, and `Deref<Target = Vec<...>>` keeps every slice consumer unchanged. Blank lines are *not* parse issues. `TolerantList<T>` carries the other four (each returns `impl Deref<Target = Vec<T>>`), and only `containers()` hands the wrapper across the `Backend` boundary — `show_skipped_lines` is specified against `ContainerList.skipped` alone. **Also (T13, D27):** the header row is now dropped by *identity* (`is_distrobox_header`, a four-literal match on `ID/NAME/STATUS/IMAGE`, which is exactly what `distrobox-list` prints at line 231 of 1.8.2.5) rather than by position. The old `.skip(1)` predated T13 — it came in with T1's `git mv rust core` — and removed whatever line was first, so a response without a header lost a real container with no log, no count, and no UI trace: the same silent-loss class this row exists to remove. `is_clean_empty()` is wired to the Containers/Backups/Packages empty states, so an all-rows-unreadable list is never rendered as "no containers yet" | `distrobox.rs`, `models/dto.rs`, `app/src/views.rs` |
| **B4** | **Correct desktop-entry field-code handling.** Add `desktop_file::split_exec(exec: &str) -> Vec<String>`: split on whitespace honoring quotes/backslash escapes (the spec's own quoting rules), then for each token strip `%%`→`%`, drop `%i`/`%c`/`%k` (**and** the extra argument `%i` would have introduced), drop `%v`/`%m`/`%d`/`%D`/`%n`/`%N` (deprecated), and strip `%f`/`%F`/`%u`/`%U`. `launch_app` (`:908`) then feeds the resulting argv to `cmd.args()` instead of `cmd.arg(one_big_string)`, fixing both the multi-argument bug and the injection hazard of interpolating a desktop-file `Exec` (which may originate inside a container) into a single argv slot. **As built (T13, D27):** the failure mechanism is the argv *slot count*, not a re-split — `distrobox-enter` ends in `exec "$@"` with no `eval` (1.8.2.5, the `--` branch: `shift; break` then `exec "$@"`), so the old single-string form asked for a program whose name was the whole fused string and the app never launched. `%i` **is** droppable-but-formable: `ExportableApp::entry.icon` is in scope, yet expanding it would be wrong, because `distrobox-export` rewrites `Icon=` to a host-side absolute path under `/run/host` (1.8.2.5, lines 582-590) that the container cannot open. An `Exec` that leaves no command after field-code removal is now a typed `Err` rather than a bare `enter --name <box> --` (a silent interactive shell in place of the app the user asked for) | `desktop_file.rs`, `distrobox.rs` |
| **B5** | **`start`.** Add to `Distrobox`: `start(&self, name) -> Result<String, Error>` (`podman start <name>`, Docker fallback, mirroring `get_container_id` `:1395`) and `start_and_enter(&self, name) -> Result<Box<dyn Child + Send>, Error>` (`enter_cmd(name)` then `cmd_spawn`). Rationale: `distrobox enter` implicitly starts a stopped container (`enter_cmd`, `:1086`), so "Start" can be `enter`-detached or a true `podman start`; the latter is what a UI button labelled "Start" should mean, and it leaves the container `Up` with no attached process. New messages: `ContainerMsg::StartRequested(String)`. Requires the `Status::Created|Exited → Up` transition to be reflected by a `list()` refresh — where the `PodmanEventStream` subscription (§6.2) pays off | `distrobox.rs` |
| **B6** | **Env guard dedupe + PATH scan.** `has_distrobox_host_exec` (`host_exec.rs:20`) gains the `$PATH` scan the Dart guard already has (`environment_guard_io.dart:28-45`), and the precedence chain collapses to `env::detect` (§2.4), called once. The `Blocked` message text is preserved so user-facing wording does not drift | `host_exec.rs`, `env.rs` |
| **B7** | **Single runtime-selection helper.** The **six** inline podman→docker blocks route through one place (§6.2). **As built (T13, D27):** this row's "seven" was an overcount — counted in `HEAD`, six functions carried a fallback (`start`, `get_container_id`, `create_snapshot`, `list_snapshots`, `delete_snapshot`, `get_container_stats`) and all six convert. `export_container`/`import_container` never had a docker branch at all (they are **streaming**, `cmd_spawn` → `Child`, which a `String`-returning helper cannot serve) and stay podman-literal. Q11 is answered by the *helper* branch, not the trait — `Distrobox::runtime_output(cmd, Fallback)` plus `container_runtime::retarget`, a `Command::program` swap that leaves the argv byte-identical. The two fallback triggers are not interchangeable: five sites retry on error (`OnError`), `get_container_id` alone retries on empty output and **propagates** a podman error (`OnEmpty`) — its original podman branch ended in `?`, so retrying there would answer a broken podman with a misleading "container not found" | `distrobox.rs`, `container_runtime.rs` |
| **B8** | **Cancel kills the child.** §3.3 — `oneshot` + `select!` + `Child::kill()` instead of `JoinHandle::abort`. **As built (T5):** `run_child_task` (`core/src/tasks.rs`) | `task_runtime.rs` |
| **B9** | **Per-chunk write lock.** `push_task_output` (`api.rs:46`) takes the registry write lock once per 1024-byte chunk. With `TaskEvent` over a channel this becomes one send per *line*, and the registry is only locked on insert/finish/sweep. **As built (T5):** `RegistryTask::push_output` (`core/src/tasks.rs`) | `task_runtime.rs` |

---

## 7. Ordering — which state slice lands in which build task

Each row is one `PLAN.md` task. The "stays buildable" column states the property that
makes the row safe to merge on its own.

| # | Task | Lands | Stays buildable because |
|---|---|---|---|
| 0 | **S1–S4** (workspace, rename, DTO move, task-runtime move) | Pure structure | Pure moves; `cargo test --workspace` still runs the existing suite |
| 1 | **S5 + skeleton app** | `app/` crate, `Core`, `Message::{NavSelect, WindowResized, Ui}`, one nav page, `init()` with `env::detect` | `app` compiles with zero domain state; `core` untouched |
| 2 | **S6 + `Backend` + `CoreError`** | `core::service::Backend`, `core::error::CoreError`, `TaskId`; the 4 *read-only* domains wired first: `containers`, `images`, `apps`, `stats` | Compiles and runs as a read-only container browser; no task spawning yet |
| 3 | **Task runtime + `spawn_task`** | `task_runtime.rs` (mpsc, `TaskEvent`, TTL sweep, cancel-kill), `spawn_task`, `Message::Tasks`, `Subscription::run_with` wiring, `Backend::create_container` | Adds write ops; first slice that needs the runtime; `S5`'s skeleton already proves the executor/subscription wiring |
| 4 | **Packages domain** | `PackageManager` enum overhaul (**B1**), `Message::Packages`, package page | Exercises `spawn_task` for install/remove; `detect_package_manager` becomes typed |
| 5 | **Snapshots + Backups domains** | `Message::{Snapshots, BackupMsg}`, snapshot/backup pages, `ContainerRuntime` wiring (**B7**), **B5** `start` | Needs `spawn_task` (restore/export/import) and the runtime helper; independent of packages |
| 6 | **Terminals + env guard** | **B6** guard, `supported_terminals` revival (T2b), `TerminalMsg`, terminal page, **B4** field codes | Independent of 4/5 except for `B4` living in `desktop_file.rs` |
| 7 | **Config persistence** | `Config` struct, `watch_config` subscription, window-size write-back, custom terminals | Everything it persists already exists by here; `selected_terminal` needs row 6, the rest need only rows 2–5 |
| 8 | **Tolerant parsing** | **B3** `ContainerList.skipped` | Touches row 2's types; deliberately late so the UI it feeds exists. **Must** land before any Flatpak/host-exec hardening |
| 9 | **S7 (delete FRB) + S8 (delete Flutter)** | — | Deletions only; §1.3 argues per-step greenness |
| 10 | **Packaging**: `.spec`, `build-rpm.sh`, Flatpak manifest, `AGENTS.md`, `README.md` | — | Post-S8; nothing compiles from these |

Two ordering constraints worth stating because they are easy to get wrong:

- **`B2` (line buffering) must land in row 3**, not row 8. It is in the same function
  that row 3 rewrites (the reader), and the task-output *view* relies on
  "one message = one line". Doing it later means touching the subscription contract
  twice.
- **`S3` (DTO move) must precede `S7`** — argued in §1.3. If they land out of order,
  S7 deletes `AppInfo`/`ExportedBinary`, which rows 2–5 depend on.

Dependency graph (row → depends on): 1→0; 2→1; 3→2; 4→3; 5→3; 6→2; 7→{2,3,4,5,6};
8→2; 9→{3,4,5,6,7}; 10→9. Rows 4, 5, 6, 8 are mutually independent after 3, so they
parallelise across build agents.

---

## 8. Open questions for the devil's advocate

Each is a decision this document made but that is genuinely arguable, with the
consequence of being wrong.

**Q1 — Executor and the `tokio::spawn` contract (§0.2).** Plan: `type Executor =
cosmic::executor::Default` and require every spawning call to go through
`cosmic::task::future`. Rejected alternative: `type Executor =
cosmic::executor::multi::Executor` (2+ worker threads, no help for `update()`), or
replacing `tokio::spawn` in core with `std::thread::spawn` + a blocking executor so
core has no ambient-runtime requirement at all. That last option also simplifies
`smol`-based tests (`smol` is already a dev-dependency). Is one option clearly
correct, or should core expose `spawn_task` in both forms? The failure mode is a runtime
panic out of a context the compiler does not model — not `update()`, which iced already
wraps in `runtime.enter()` (§0.2), but a bare thread or a `smol`-driven test body, which
is why one contract for every caller is the safe form.

**Q2 — Is per-task `Subscription::run_with` the right unit?** That is one iced
subscription per *live task*, keyed by `TaskId`, plus a `OnceLock<Arc<Backend>>`
global to satisfy the `fn`-pointer builder (§3.4). The global mirrors libcosmic's own
precedent but is still a global. Alternative: one *aggregate* subscription over a
single channel that all tasks feed, which removes the global's per-task lookups but
makes "which task completed" a message field and risks head-of-line blocking across
tasks. Which is better when 8 tasks stream simultaneously?

**Q3 — `Message` granularity.** One `Message` enum with 12 nested sub-enums (§2.3)
versus one flat enum of ~70 variants, versus per-page `Message` types composed by the
nav. The nested form keeps `update()`'s match arms readable and lets a page own its
sub-enum, but every page must then be threaded through the top level. Does libcosmic
have a convention here (the example app's enum is flat, but it is also trivial)?

**Q4 — Where does the authoritative task state live?** Plan: core owns the registry
(TTL, replay buffer, cancel), the app holds a `TaskView` mirror (§3.1, §3.3). The
alternative — core returns a receiver and forgets the task, the app owning everything
— removes the dual bookkeeping and the `TaskMsg::Expired` round-trip, but gives up
`is_task_running`/`cancel_task` for a task whose view was dropped, and makes TTL a
UI concern. Which set of invariants actually matters?

**Q5 — `cosmic-config` vs `dirs` + `serde` (§5.1).** The chosen option adds no new
dependency (libcosmic already pulls it with `dbus`) and gets Flatpak paths, layered
defaults, atomic writes, versioning, and a file watcher. The rejected option is ~40
lines using three dependencies already present (`dirs`, `serde`, `toml`) with no zbus
surface and no watcher. Cost of being wrong: a config format migration, or a Flatpak
user whose settings silently live in two places.

**Q6 — `cosmic_config_derive` key naming. CLOSED** (REVIEW.md §C ARCH-Q6; the decision
is unchanged — only the doc's own "unresolved" status was wrong). Keys are stored
one-per-file via `key_path` (`lib.rs:396`). The derive uses the Rust field name verbatim
as the key string: it emits `ConfigSet::set(&tx, stringify!(#field_name),
&self.#field_name)?` plus the matching `ConfigGet::get::<#field_type>(config,
stringify!(#field_name))`, and `stringify!` takes the identifier as written. So the §5.3
key table is correct as written and the "verify before implementing row 7" gate is
satisfied. Residual to carry: the legacy gschema keys are kebab-case
(`selected-terminal`, `window-width`, `window-height`, `distrobox-executable`) and the
cosmic-config keys snake_case, so the mapping is a rename, not a copy (§5.3).

**Q7 — `selected_terminal` migration semantics (§5.3).** Legacy values are bare
program names (`'gnome-terminal'`); the revived terminal model identifies a terminal by
`full_command_id()` = `program + extra_args`
(`supported_terminals.rs::full_command_id`). Plan: match
the legacy value against `program` and rewrite. Sub-question: what if the legacy value
matches **two** entries (e.g. several Flatpak terminals all have
`program == "flatpak"` but different `extra_args` — the crate's own doc-comment calls
this out)? First match is arbitrary. Is silently picking one acceptable, or must the
UI prompt?

**Q8 — What does `distrobox_source = "bundled"` mean?** The gschema default is
`'host'` and nothing reads it. §5.3 proposes `"host"` = map through
`flatpak-spawn --host`/`distrobox-host-exec`, `"bundled"` = use the sandbox's own
`PATH`. But nothing in the repo bundles distrobox, and the `.spec` has
`Requires: distrobox`. Either the key is dropped, or "bundled" needs a real
implementation. Which?

**Q9 — Enable libcosmic's `single-instance` feature?** It gives
`run_single_instance` + `Application::dbus_activation` and pairs with the existing
`DBusActivatable=true` desktop entry and the `.service.in`. It also adds a feature
flag and zbus to the app crate. Is a second instance actually harmful for this app
(two windows polling, two task registries, both writing the same config key files)?

**Q10 — Mechanical enforcement of the CommandRunner rule (§6.1).** The doc asserts the
rule; should it be enforced? A clippy `disallowed-methods` entry for
`std::process::Command::new` in `core/` is cheap and catches the one mistake that
silently breaks every Flatpak user. Any reason not to?

**Q11 — Extend `ContainerRuntime` or add one helper? CLOSED** (T13, D27; see §6.2).
Answer: **the helper, not the trait** — `Distrobox::runtime_output(cmd, Fallback)` over
`container_runtime::retarget`, with `ContainerRuntime` left at three methods. The
question as written assumed "seven inline fallbacks"; there were six, and all six are
plain output commands, so extending the trait would have re-encoded an argv `Distrobox`
already builds. The `?Send` objection raised below turned out to be decisive rather
than a risk: it is a second reason not to grow an `#[async_trait(?Send)]` trait for
operations that never needed `Send`. The trait's real value (e.g. `podman events` having
no docker equivalent) is untouched by this — `Q12` still owns that.

**Q12 — Should `PodmanEventStream` drive container state?** `podman.rs:66-122`
already implements a `Stream` over `podman events`, filtered to distrobox containers.
Wiring it (as a `cosmic::iced::Subscription`) replaces poll-only refresh with live
state and makes `B5`'s start/stop transitions feel instant. But it introduces a
long-lived child process and a failure mode where the stream dies silently and the UI
stops updating. Does the event stream need a watchdog/reconnect, and is it worth it
versus the `refresh_interval_secs` poll?

**Q13 — `Status::Other(String)` display.** `Status`
(`core/src/backends/distrobox/distrobox.rs::Status`) is
`Up | Created | Exited | Other(String)` — `Other` catches podman's transitional states
(`Paused`, `Restarting`) and is currently rendered as free text. With live events
(§6.2) these become reachable. Should the UI model them explicitly, or keep the
catch-all?

**Q14 — Does the app need `--recurse-submodules` in CI and packaging?** libcosmic
vendors `iced` as a submodule (§0.1). Any environment that builds the app must
initialise it, and `cargo` does not do this. Does the RPM/Flatpak build path already
handle submodules, or does the pin need to be a `git` dependency with its own
submodule-aware checkout? This bites at row 10, but discovering it then is expensive.
