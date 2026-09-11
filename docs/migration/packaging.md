# Packaging & QA Plan — Flutter → libcosmic Migration

Status: **Phase 1 design. No source changes.** This document is the specification for
Phase 2 (packaging) and Phase 3 (QA). Everything asserted here was verified against
this machine or upstream sources on 2026-09-11; verification method is noted inline.

---

## 0. Verified environment (the constraints everything else follows from)

| Fact | Value | How verified |
|---|---|---|
| cargo / rustc | 1.98.1 (`Fedora 1.98.1-1.fc44`) | `cargo --version` |
| flatpak | 1.18.2 | `flatpak --version` |
| flatpak-builder | 1.4.10 | `flatpak-builder --version` |
| Flutter/Dart SDK | **absent** | no `dart`/`flutter` anywhere |
| Flutter flatpak SDK extension | **absent** | `ls ~/.local/share/flatpak/runtime/ \| grep -i flutter` → nothing |
| freedesktop Platform/SDK | 25.08 (`freedesktop-sdk-25.08.16`) | `flatpak list --runtime` |
| rust-stable extension | 25.08, rustc 1.98.1 | `flatpak list --runtime` |
| Remotes | `flathub`, `cosmic` (apt.pop-os.org) | `flatpak remotes -d` |
| Session | **live COSMIC Wayland** — `XDG_CURRENT_DESKTOP=COSMIC`, `WAYLAND_DISPLAY=wayland-1`, `DISPLAY=:1` (Xwayland) | `env`, `/run/user/1000/wayland-1` |
| Xvfb / weston / cage / dbus-run-session | **not in PATH** (Xvfb exists only as unbuilt nix `.drv`) | `which`, `ls /nix/store/*xvfb*` |
| `distrobox` CLI | **NOT INSTALLED** | `rpm -q distrobox`, no binary |
| `podman` / `docker` | **NOT INSTALLED** | `rpm -q podman` |
| `flatpak-spawn` | present at `/usr/bin/flatpak-spawn` | `which` |
| Free disk | 372 GB | `df -h` |
| Network | available (github, crates.io reachable) | `curl` |

### 0.1 What the rust-stable extension actually ships

Directory: `~/.local/share/flatpak/runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/25.08/<hash>/files/bin`

`bindgen`, `cargo`, `cargo-capi`, `cargo-cbuild`, `cargo-cinstall`, **`cargo-clippy`**,
**`cargo-vendor-filterer`**, `clippy-driver`, `ld.mold`, `mold`, `rust-analyzer`, `rustc`,
`rustdoc`, **`rustfmt`**, `rust-gdb`, `rust-gdbgui`, `rust-lldb`.

This confirms the extension is sufficient for build, lint, format, and vendor-filtering
— **no extra tooling module is needed in the manifest**.

### 0.2 The two corrections to the recon brief

> **C1 — `--filesystem=home` is NOT required.** The recon assumed home access for
> "container configs, exported .desktop files". That is wrong. Every distrobox/podman
> interaction goes through `CommandRunner`, which under Flatpak becomes
> `flatpak-spawn --host …` and therefore executes **on the host**, outside the sandbox,
> with full host access. Verified: `rust/src/app_state.rs:18` and `rust/src/api.rs:145`
> install `map_flatpak_spawn_host` when `/.flatpak-info` exists, and the desktop-file
> discovery script is `include_str!`-embedded (`rust/src/backends/distrobox/distrobox.rs:20`)
> and run through the runner — it reads `$HOME/.local/share/applications` **on the host**.
> Granting `--filesystem=home` would be an unnecessary, Flathub-blocking over-grant.

> **C2 — iced is a git *submodule*, not in-tree.** The recon said "iced vendored in-tree".
> At the pinned rev, `iced`, `iced_core`, `iced_widget`, `iced_runtime`, `iced_renderer`,
> `iced_futures`, `iced_tiny_skia`, `iced_winit`, `iced_wgpu`, `iced_accessibility` are all
> `path = "./iced/…"` dependencies resolved through a **git submodule**
> (`.gitmodules` → `https://github.com/pop-os/iced.git`). This is the single most
> important fact for source vendoring. See §1.3.

---

## 1. Flatpak manifest design

Target file: **`flatpak/io.github.gosh_distrobox_manager.json`** (currently does not exist;
CI references a bare root-level `.json` that was never committed — that CI is dead, §2.4).

### 1.1 Runtime / SDK / base

```json
{
  "id": "io.github.gosh_distrobox_manager",
  "runtime": "org.freedesktop.Platform",
  "runtime-version": "25.08",
  "base": "com.system76.Cosmic.BaseApp",
  "base-version": "stable",
  "sdk": "org.freedesktop.Sdk",
  "sdk-extensions": ["org.freedesktop.Sdk.Extension.rust-stable"],
  "command": "gosh_distrobox_manager"
}
```

**Why this exact shape** — it is byte-for-byte the shape used by
`flathub/dev.edfloreshz.CosmicTweaks` (verified: fetched
`raw.githubusercontent.com/flathub/dev.edfloreshz.CosmicTweaks/master/dev.edfloreshz.CosmicTweaks.json`),
a **shipping COSMIC app on the same 25.08 runtime**, and that app is installed on this
machine. Copying a known-good COSMIC flatpak skeleton is worth more than first-principles
design here.

**Why the COSMIC BaseApp.** `com.system76.Cosmic.BaseApp` (Flathub) installs
`pop-icon-theme`, `cosmic-icons`, and `just`, then removes `just` in a cleanup script
(verified: fetched the BaseApp manifest and `cleanup.sh`). libcosmic's `build.rs` only
generates *bundled* icons on non-Unix/macOS targets (verified: `build.rs` guards
`generate_bundled_icons()` behind `CARGO_CFG_UNIX` / `target_os == "macos"`), so on Linux
libcosmic resolves icons from the **system icon theme at runtime** via
`src/icon_theme.rs`. Without `cosmic-icons` + `pop-icon-theme` installed the app renders
with missing/blank icons. The BaseApp is the supported way to get them.

**Important:** the BaseApp does **not** provide a prebuilt libcosmic. CosmicTweaks vendors
libcosmic itself through `cargo-sources.json`. So we do not get to skip §1.3.

### 1.2 Module layout

Single module. The Rust binary is the whole app; there is no C UI, no `meson`, no
Flutter bundle. Framing the module on `simple` (not `meson`) is deliberate: the
existing `rust/data/icons/meson.build` is the only meson file in the tree and the old
CI drove `meson dist`, both of which are being retired.

```json
"build-options": {
  "append-path": "/usr/lib/sdk/rust-stable/bin",
  "env": { "CARGO_HOME": "/run/build/gosh_distrobox_manager/cargo" }
},
"modules": [
  {
    "name": "gosh_distrobox_manager",
    "buildsystem": "simple",
    "build-commands": [
      "cargo --offline build --release --verbose",
      "install -Dm0755 target/release/gosh_distrobox_manager -t /app/bin/",
      "install -Dm0644 data/io.github.gosh_distrobox_manager.desktop /app/share/applications/io.github.gosh_distrobox_manager.desktop",
      "install -Dm0644 data/io.github.gosh_distrobox_manager.metainfo.xml /app/share/metainfo/io.github.gosh_distrobox_manager.metainfo.xml",
      "install -Dm0644 data/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg /app/share/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg",
      "install -Dm0644 data/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg /app/share/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg"
    ],
    "sources": [
      { "type": "git", "url": "https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager", "commit": "<TAG_COMMIT_SHA>" },
      "cargo-sources.json"
    ]
  }
]
```

Notes:
- `CARGO_HOME` **must** be `/run/build/<module-name>/cargo` — the path is keyed to the
  module name. Getting this wrong produces a "registry not found" offline failure.
- `--offline` is mandatory: the build sandbox has no network.
- Source installs are flat `.desktop`/`.metainfo.xml` files, **not** `.in` templates.
  The `.in` files currently in `rust/data/` require `@bindir@`-style substitution that
  nothing in a pure-Rust build performs. See §1.5.

### 1.3 Source vendoring strategy for the git dependency (the hard part)

#### The problem

`libcosmic` is not on crates.io (and the crates.io name `cosmic` is a squatted,
unrelated crate — never use it). It must come from git. But libcosmic's `iced`
dependency is a **submodule**, and neither `cargo vendor` nor a naive
`flatpak-cargo-generator` run gives you submodule contents in the manifest.

#### The mechanism that actually works

**`flatpak-builder` checks out git submodules by default.** The git source option
`disable-submodules` is documented as "Don't checkout the git submodules when cloning the
repository" — i.e. the default is `false`, so submodules *are* checked out. Verified two
ways: (a) the flatpak-builder command reference (`docs.flatpak.org`, string
`disable-submodules` present with that description), and (b) the local binary contains
the strings `--no-recurse-submodules`, `mirror submodule %s at revision %s`,
`submodule.%s.url` (`strings /usr/bin/flatpak-builder`).

Consequently a plain git source for libcosmic **is** sufficient; the `iced` submodule is
fetched from `https://github.com/pop-os/iced.git` at libcosmic's recorded submodule
commit during the source-download phase (which has network; the *build* phase does not).

**Real-world confirmation.** CosmicTweaks' `cargo-sources.json` (1563 entries, fetched and
parsed) contains a git source
`{"url":"https://github.com/pop-os/libcosmic","commit":"c44dbe793b64caa8604dc38816c9b791e7ecb8ac","dest":"flatpak-cargo/git/libcosmic-c44dbe7"}`
and **no entry for `pop-os/iced` at all** — a `grep` for `pop-os/iced` over the whole file
returns nothing. The app builds and ships. That is direct proof that submodule population
is flatpak-builder's job, not the generator's.

**Why the generator still needs submodules.** `flatpak-cargo-generator.py` runs
`git submodule update --init --recursive` on each git dependency (verified: grep of
`flatpak-builder-tools/cargo/flatpak-cargo-generator.py`, line ~154). It needs the
submodule present to *parse* `iced/Cargo.toml` and friends — libcosmic's path deps must
resolve to a coherent lockfile. But the iced crates are path deps, so they are **not**
emitted into `cargo-sources.json`; they are simply expected to exist in the checkout.
This is why the generator's submodule step and flatpak-builder's submodule step are two
different things that must both hold.

#### Pinned constants (verified against upstream on 2026-09-11)

| Component | Pin |
|---|---|
| libcosmic | rev `a401af8b1c54a8abd393b8c5b7c8809402f83850` (v1.0.0, 2026-09-10) |
| → submodule `iced` | `https://github.com/pop-os/iced.git` @ `ffe1f1dbe3cbfd313f9b5fe8e36a4af462cae5d7` |
| → submodule `cosmic-icons` | `https://github.com/pop-os/cosmic-icons.git` @ `343c007f37cd71716e68f01c43ecf2764b7f7c47` |

The submodule SHAs are **recorded here for documentation only** — they are not written into
the manifest, because flatpak-builder reads them from the libcosmic tree. Recording them
matters for *reproducibility auditing*: if libcosmic's tree is ever force-dated or the
submodule pointer moves, these are the values a build must be reproducing.

libcosmic at this rev: `version = 1.0.0`, `edition = "2024"`, `rust-version = "1.93"`,
`[lib] name = "cosmic"`. All verified from the fetched `Cargo.toml`. MSRV 1.93 < our
1.98.1, so no toolchain problem.

#### Procedure to (re)generate `cargo-sources.json`

```bash
# one-time, on a networked machine
pip install --user flatpak-cargo-generator
# or: curl -O https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py

cargo generate-lockfile                  # in rust/  — must be committed
python3 flatpak-cargo-generator.py rust/Cargo.lock -o flatpak/cargo-sources.json
```

`cargo-vendor-filterer` (shipped by the extension) is **not** the tool for this manifest.
It produces a filtered `vendor/` directory for in-sandbox consumption, an alternative
strategy that would require committing a large vendor tree to the repo and wiring
`.cargo/config.toml` by hand. The flathub-standard `cargo-sources.json` route is preferred:
the lockfile is a checked-in artifact, the vendor tree is reconstructed at build time,
and Flathub's own tooling and review process expect it. `cargo-vendor-filterer` remains
useful for *local* offline builds and for CI caching if we later want them.

#### `--share=network` must be OFF for the build

Do not add `--share=network` to `build-options`. If the build needs network, the vendoring
is broken and that must fail loudly rather than silently succeed on the maintainer's machine.

#### Feature selection

Depend on libcosmic with `default-features = false` plus an explicit feature list. The
default feature set includes `dbus-config`, and the `applet` feature pulls
`cosmic-panel-config` — which is declared as `git = "https://github.com/pop-os/cosmic-panel"`
**with no `rev` or `tag`** (verified in the fetched `Cargo.toml`). An unpinned git
dependency is non-reproducible and would break `--offline` builds. **Never enable
`applet`, and never enable any feature that transitively enables it.** If a future
libcosmic feature we need drags in `cosmic-panel-config`, pin it explicitly with a
`[patch]` or a `rev` in our own `Cargo.toml`.

### 1.4 finish-args — minimal set with justification

```json
"finish-args": [
  "--share=ipc",
  "--socket=wayland",
  "--socket=fallback-x11",
  "--device=dri",
  "--talk-name=org.freedesktop.Flatpak",
  "--filesystem=xdg-config/cosmic:rw",
  "--filesystem=xdg-data/distroshelf-terminals.json:rw"
]
```

| Argument | Justification |
|---|---|
| `--share=ipc` | Shared-memory transport for the toolkit; required for X11 shared-memory and standard for every GUI flatpak (CosmicTweaks, GNOME apps all carry it). |
| `--socket=wayland` | Primary display protocol on COSMIC (verified: `WAYLAND_DISPLAY=wayland-1`). |
| `--socket=fallback-x11` | X11 only when no Wayland compositor is present. **Deliberately `fallback-x11`, not `x11`** — `x11` grants X11 access even when Wayland is available, which is a needless widening. CosmicTweaks uses `fallback-x11`. |
| `--device=dri` | GPU access for wgpu/GL rendering. Without it the app falls back to software rendering or fails to create a surface. |
| `--talk-name=org.freedesktop.Flatpak` | **Load-bearing.** Permits `flatpak-spawn --host`, which is how *every* distrobox/podman command escapes the sandbox. `rust/src/app_state.rs:18` and `rust/src/api.rs:145` install `map_flatpak_spawn_host` whenever `/.flatpak-info` exists. **Without this argument the app is completely non-functional under Flatpak** — it would appear to launch and then every operation would fail. |
| `--filesystem=xdg-config/cosmic:rw` | Write access for cosmic-config (`~/.config/cosmic/<app-id>/v1/…`). This is how settings persist under libcosmic; it replaces the GSettings backend being deleted (§1.6). CosmicTweaks carries the equivalent. |
| `--filesystem=xdg-data/distroshelf-terminals.json:rw` | The **only** direct filesystem access in the sandboxed process: `rust/src/backends/supported_terminals.rs:112-114` computes `dirs::data_dir().join("distroshelf-terminals.json")`, and lines 246/262 read and write it. See §1.4.1. |

#### 1.4.1 Deliberate omissions (and why)

- **`--filesystem=home` — omitted.** See correction C1. Nothing in the sandboxed process
  touches host home paths; distrobox/podman and the desktop-file scanner all run on the
  host via `flatpak-spawn`. Granting it would be an unjustified over-grant and a likely
  Flathub review rejection.
- **`--share=network` — omitted.** No in-sandbox HTTP client exists. Container image
  pulls are performed by host `podman`, outside the sandbox. **Open question Q4** if the
  new UI ever fetches anything itself (release notes, icon packs, update checks).
- **`--socket=x11` — omitted** in favour of `fallback-x11` (§1.4).
- **`--talk-name=com.system76.CosmicSettingsDaemon` — omitted for now.** CosmicTweaks needs
  it because it *edits* COSMIC settings. We only need libcosmic's `dbus-config` feature to
  *watch* our own config file. Add it only if we adopt live theme-following. **Open question Q5.**
- **`--filesystem=xdg-config/gtk-3.0:ro`, `xdg-config/gtk-4.0:ro`, `xdg-config/kdeglobals:ro`,
  `xdg-data/color-schemes:ro` — omitted.** These are CosmicTweaks-specific (it previews
  GTK/KDE themes). libcosmic themes itself.
- **`--device=all`, `--allow=devel`, `--socket=session-bus`, `--filesystem=host` — never.**
  Each is a broad escalation with no use here.

#### 1.4.2 The `distroshelf-terminals.json` problem

The only sandbox-visible host path is named after the **pre-rename project name**
(`distroshelf`). This is rename drift of the same family as the metainfo/spec drift in §4.
Two options:

1. **Rename it** to `gosh_distrobox_manager-terminals.json`, update
   `supported_terminals.rs:112-114`, and grant `xdg-data/gosh_distrobox_manager-terminals.json:rw`.
   Cost: existing custom terminal lists are silently dropped (a one-time migration shim
   could read the old path, but this is a pre-1.0 app and the drift is already user-visible).
2. **Fold it into cosmic-config**, making the file disappear and the grant unnecessary.
   This is the cleaner outcome and is **recommended** — it aligns with §1.6's rationale
   for deleting GSettings.

Whichever is chosen, the grant must match the code exactly. A mismatch fails *at runtime*,
silently, as an unlogged write error (line 253 only logs at `error!`), which is exactly
the class of bug that escapes a smoke test. Add a unit test asserting the resolved path
constant.

### 1.5 Desktop file, metainfo, and icon fixes

There are currently **two** desktop files and they disagree:

| File | State |
|---|---|
| `rust/data/io.github.gosh_distrobox_manager.desktop.in` | GTK-era template. **Stale.** |
| `linux/data/share/applications/io.github.gosh_distrobox_manager.desktop` | Flutter-era. Richer, but also needs edits. |

**Recommendation:** keep exactly one, as a plain (non-`.in`) file under `rust/data/`,
installed verbatim by the manifest. Delete the Flutter tree's copy along with the rest
of `linux/`.

#### Desktop file — every fix

1. **`Keywords=GTK;` → remove.** Stale GTK-era keyword, meaningless for a libcosmic app,
   and actively misleading in search results.
2. **`DBusActivatable=true` → remove.** libcosmic does not implement `GApplication`/D-Bus
   activation. Leaving this set makes the shell attempt D-Bus activation, which will fail.
3. **`Exec=gosh_distrobox_manager` → `Exec=gosh_distrobox_manager`** — correct as-is, and
   it **must** match the manifest's `"command"` and the installed `/app/bin/` filename.
4. **`Icon=io.github.gosh_distrobox_manager`** — correct; ensure the icon file is named
   exactly `io.github.gosh_distrobox_manager.svg` (§1.5 icons).
5. **`Categories=Utility;` → `Categories=System;Utility;`.** Adopt the Flutter copy's
   richer value; a container manager is a system tool. `System` must not be combined with
   `Settings`; it is valid with `Utility`.
6. **Add `StartupWMClass=io.github.gosh_distrobox_manager`.** Under Wayland the shell matches
   the window to the launcher by `app_id`; under X11 it uses `StartupWMClass`. Set the
   winit/iced `app_id` to the full application ID so both paths agree. Skipping this
   produces a duplicate/detached taskbar icon — the classic symptom.
7. **Add `Comment=` and `GenericName=`** (carry over from the Flutter copy).
8. **Add `Keywords=distrobox;container;podman;docker;`** (carry over from the Flutter copy).
9. **`StartupNotify=true`** — keep.
10. **`Version=1.0`** — the Flutter copy has it; it is optional and refers to the desktop
    entry spec version, not the app. Keeping it is harmless; do not confuse it with the
    app version.

#### D-Bus service file — delete

`rust/data/io.github.gosh_distrobox_manager.service.in` runs
`@bindir@/gosh_distrobox_manager --gapplication-service`. **`--gapplication-service` is a
GTK/GApplication flag**; a libcosmic/winit binary does not parse it and will either error
or misinterpret it. Since `DBusActivatable` is also being removed, this file has no
purpose. **Delete it.** (If single-instance behaviour is wanted later, libcosmic offers a
`single-instance` feature that works over its own socket — that is the right mechanism,
not D-Bus activation.)

#### Metainfo — every fix

1. **`<developer id="io.github">` → `<developer id="io.github.goshitsarch_eng">`.**
   The current value is malformed: an AppStream developer `id` must be a reverse-DNS
   identifier, and `io.github` alone is not one. Flathub's `appstreamcli validate` rejects it.
2. **All three `<url>` elements point at a non-existent org:**
   `https://github.com/gosh-distrobox-manager/gosh-distrobox-manager` →
   `https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager` (and `…/issues` for the
   bugtracker). The `spec` file already has the correct URL, so the metainfo is simply wrong.
   Broken homepage/bugtracker URLs are a Flathub submission blocker.
3. **Release list is stale — `1.0.0` while `Cargo.toml` says `1.0.2`.** Fix as part of the
   version unification in §4, and add the missing intermediate releases.
4. **Verify `<launchable type="desktop-id">io.github.gosh_distrobox_manager.desktop</launchable>`**
   matches the installed desktop filename exactly (it currently does).
5. **`<translation type="gettext">gosh_distrobox_manager</translation>`** — only valid if
   gettext translations are actually produced. We are not producing a `.mo` catalogue in
   this build. Either wire up `i18n` extraction or **remove the element**; a
   translation declaration with no catalog behind it is a validation warning.
6. **`<content_rating type="oars-1.1" />`** — keep, empty is valid.
7. **`<branding>` colours** — harmless, keep.
8. **Add `<screenshots>`** referencing the existing `rust/data/screenshots/1..3.png`.
   Flathub requires at least one screenshot; a metainfo without screenshots will be
   flagged in review. Note the screenshots are currently Flutter UI and must be
   regenerated post-migration.

**Validation command (must pass in `verify.sh`):**
```bash
appstreamcli validate --no-net rust/data/io.github.gosh_distrobox_manager.metainfo.xml
desktop-file-validate rust/data/io.github.gosh_distrobox_manager.desktop
```
Neither tool is guaranteed present on this machine; both are in the SDK and on
`ubuntu-latest`, so CI is the reliable place to run them.

#### Icons — every fix

1. The scalable app icon `rust/data/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg`
   and symbolic `…/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg` are the two
   the manifest installs. Both exist. Good.
2. **The `linux/data/share/icons/hicolor/<N>x<N>/apps/*.png` raster set is redundant** for
   a flatpak — Flathub prefers a single scalable SVG. Keep the PNGs for the RPM if desired,
   but do not install them in the flatpak.
3. **Distro logo SVGs** (`alma.svg`, `arch.svg`, …) live in `rust/data/icons/` and are
   currently installed by `rust/data/icons/meson.build`. That meson file goes away with
   the `simple` buildsystem, so **the distro logos need an explicit install rule** in the
   module's `build-commands` if the app loads them from disk. Verify whether the migrated
   app embeds them (`include_str!`/`rust-embed`) or reads them at runtime — if runtime,
   they must be installed and their lookup path must be `$FLATPAK_DEST/share/...`, not a
   build-relative path. **Flagged as a likely silent-failure point.**
4. Keep `Icon=` in the desktop file matching the SVG basename exactly.

### 1.6 GSettings schema removal — rationale

**Delete `rust/data/io.github.gosh_distrobox_manager.gschema.xml`.**

- It is **already dead code**. A repository-wide grep for `gschema`, `gio::Settings`,
  `Gio.Settings`, `selected-terminal`, and `distrobox-executable` returns hits **only inside
  the XML file itself** — no Rust or Dart source reads it. It is a GTK4-era leftover.
- Three of its four keys are obsoleted by the migration anyway:
  - `selected-terminal` → the new config design owns terminal selection.
  - `distrobox-executable` (`'host'` | `'bundled'`) → the "bundled" path is meaningless
    under Flatpak, where `flatpak-spawn --host` is mandatory and unconditional.
  - `window-width` / `window-height` → libcosmic persists window geometry itself.
- libcosmic's configuration mechanism is **cosmic-config** (RON files under
  `~/.config/cosmic/<app-id>/`), not GSettings. Keeping a GSettings schema alongside
  cosmic-config creates two competing sources of truth for exactly the same settings.
- Removing it removes the `glib-compile-schemas` build/install step and the
  `--filesystem=xdg-config/glib-2.0/schemas` class of permissions.

Consequence: the sandbox needs no GSettings schema directory, and the persistence test
plan (§2.2) targets cosmic-config, not GSettings.

---

## 2. Build / test / CI

### 2.1 `scripts/verify.sh` — specification

Exact commands, in order. Fail-fast (`set -euo pipefail`). Each stage prints a banner so a
failure is attributable without reading the whole log.

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ---- 1. Formatting (cheap, catches whole classes of review noise) -------------
cargo fmt --manifest-path rust/Cargo.toml --all -- --check

# ---- 2. Build (release, as the flatpak will) ---------------------------------
cargo build --manifest-path rust/Cargo.toml --release --locked

# ---- 3. Lint — warnings are errors -------------------------------------------
cargo clippy --manifest-path rust/Cargo.toml --all-targets --locked -- -D warnings

# ---- 4. Unit + integration tests ---------------------------------------------
cargo test --manifest-path rust/Cargo.toml --locked

# ---- 5. Packaging metadata validation ----------------------------------------
desktop-file-validate rust/data/io.github.gosh_distrobox_manager.desktop
appstreamcli validate --no-net rust/data/io.github.gosh_distrobox_manager.metainfo.xml

# ---- 6. Flatpak build ---------------------------------------------------------
flatpak-builder --user --force-clean --disable-rofiles-fuse \
    --install-deps-from=flathub \
    --repo=repo \
    build \
    flatpak/io.github.gosh_distrobox_manager.json

# ---- 7. Smoke test ------------------------------------------------------------
./scripts/smoke-test.sh

echo "verify.sh: ALL STAGES PASSED"
```

Rationale for the ordering: cheapest and most-likely-to-fail stages first. Steps 1–4 need
no flatpak and no network beyond a populated cargo cache, so they give fast feedback.
Step 5 catches metadata drift (this project's recurring failure mode). Steps 6–7 are the
slow, environment-dependent stages.

Deviations to consider:
- `--locked` is deliberate on build/clippy/test: it fails if `Cargo.lock` is out of sync
  with `Cargo.toml`, which is precisely the drift class we are trying to eliminate (§4).
- `--disable-rofiles-fuse` avoids a hard dependency on FUSE in containers/CI.
- `--install-deps-from=flathub` must be paired with `--user` and requires the flathub
  remote; add a preflight check that errors with a readable message if the remote or the
  25.08 runtime is missing, rather than letting flatpak-builder emit a confusing failure.

**Preflight block to add at the top:**
```bash
command -v flatpak-builder >/dev/null || { echo "flatpak-builder not found"; exit 1; }
flatpak remotes --user | grep -q '^flathub' || { echo "flathub remote missing"; exit 1; }
flatpak info --user org.freedesktop.Sdk//25.08 >/dev/null 2>&1 || {
  echo "org.freedesktop.Sdk 25.08 not installed"; exit 1; }
```

### 2.2 Unit test plan

Current state: **70 `#[test]`s, all offline, all passing without network or distrobox.**
Distribution (verified by counting `#[test]` per file):

| File | Tests | Notes |
|---|---|---|
| `backends/distrobox/distrobox.rs` | 18 | highest value — distrobox argv assertions |
| `fakers/command_runner.rs` | 13 | NullCommandRunner harness |
| `fakers/command.rs` | 12 | |
| `backends/desktop_file.rs` | 10 | |
| `fakers/output_tracker.rs` | 8 | |
| `backends/flatpak.rs` | 4 | |
| `backends/host_exec.rs` | 3 | |
| `fakers/host_env.rs` | 2 | |

**Untested surface (verified):** the entire `api.rs` surface (35 public functions, zero
tests), `models/task.rs` (task/broadcast/TTL), and the package / snapshot / export / stats
code paths, plus `podman.rs`, `docker.rs`, `container_runtime.rs`, `supported_terminals.rs`,
and all of `models/`.

#### What the migration must preserve

The 70 existing tests are the migration's safety net. **They must keep passing after the
Flutter tree is deleted.** The backend is UI-agnostic; a correct libcosmic migration
should require *zero* changes to `backends/`, `fakers/`, or `models/`. If a backend test
needs editing to accommodate the UI change, that is a signal the migration has leaked
across the boundary and should be re-examined.

Specifically, `NullCommandRunner` must survive intact. It is the mechanism that makes
offline argv assertion possible and it is what makes the integration plan in §2.3
tractable.

#### New unit tests required

**A. Every `Message` variant.** The libcosmic `Application` trait requires
`type Message`. Verified: **no `Message` enum exists today** — this is new surface
introduced by the migration. Required for each variant:
1. A test that `update()` with that variant produces the expected state transition.
2. A test that the variant is reachable — i.e. some `view()` code path constructs it
   (guards against dead variants that compile but never fire).

Enumerate variants exhaustively and fail the build if a variant lacks coverage. A simple
`match` in a test that must be exhaustive will *not* catch this (the compiler is satisfied
by the test's own arm), so the practical approach is a table-driven test over a
`Vec<Message>` of all variants, asserting each is handled without panic.

**B. Business logic.** For each of the 35 `api.rs` functions, an offline test using
`NullCommandRunner` asserting the *argv actually produced*. This is the single highest-value
test work in the whole plan — it is exactly the pattern the existing 18 `distrobox.rs`
tests use and it catches the realistic regression (a command built wrong) without needing
a container runtime.

**C. Settings persistence.** Round-trip tests over cosmic-config: write a value, read it
back, assert equality; assert defaults apply on a missing/corrupt file. Critically, assert
the **resolved config path** matches the path granted in `finish-args` (§1.4) — a mismatch
is a silent runtime write failure under Flatpak.

**D. `Task` / broadcast / TTL.** `models/task.rs` and the `api.rs` helpers
(`push_task_output`, `finish_task`, `COMPLETED_TASK_TTL = 600s`,
`MAX_TASK_OUTPUT_LINES = 500`) are entirely untested. Test: output line cap enforcement,
broadcast delivery to a subscriber, completion flagging, and TTL eviction of completed
tasks. Use `tokio::time::pause()` / `advance()` so the 600-second TTL does not make the
suite slow. Note `finish_task` contains a `retain` whose guard has a subtle branch
(`None => true` keeps a completed task with no `completed_at` forever) — a test should
pin the intended behaviour here.

**E. `fakers/` coverage.** The existing fake infrastructure must be extended for any new
backend call the migrated UI makes. Requirement: a fake for every trait method, so that
no integration test in §2.3 is ever forced to touch the real filesystem or a real process.

### 2.3 Integration test plan

Constraint that shapes everything: **`distrobox` and `podman` are NOT installed on this
machine** (verified: `rpm -q distrobox` → not installed; `/usr/bin/distrobox-host-exec`
and `/usr/bin/distrobox-export` exist but are *orphans* — `rpm -qf` reports "not owned by
any package" — and `--version` reports `distrobox: 1.8.2.5` from a leftover file, with no
main `distrobox` binary). **No flow that requires a real container can be executed here.**

Therefore: **all integration tests are driven through messages + state, with
`NullCommandRunner` supplying canned output.** This is a feature, not a compromise — it is
the only approach that runs in CI on `ubuntu-latest` too.

Design:

1. Construct the app's state (the libcosmic `Application` struct) directly.
2. Feed it canned `CommandRunner` output through the existing fake infrastructure.
3. Drive it by dispatching `Message` values into `update()`.
4. Assert on the resulting state — the container list, the task list, the error banners.

Flows to cover (one test each, minimum):

| Flow | Canned input | Assertion |
|---|---|---|
| List containers | `distrobox list` table output | parsed into `Vec<ContainerInfo>`; status mapped correctly |
| Create container | `distrobox create` argv + streamed output | correct argv; task created; output appended; completion flag set |
| Delete / stop / stop-all | argv | correct argv; optimistic state update |
| Enter terminal | `enter_cmd` argv | correct argv; under Flatpak, prefixed with `flatpak-spawn --host` |
| List / export / unexport apps | desktop-file TOML (hex-encoded fixture) | parsed; `is_exported` correct |
| List / export / unexport binaries | fixture output | correct classification |
| Upgrade / clone | argv | correct argv |
| Package install / search / list | fixture output | parsed into `PackageInfo` |
| Snapshots | fixture output | parsed into `SnapshotInfo` |
| Stats | fixture output | parsed into `ContainerStats` |
| Errors | runner returns non-zero + stderr | error surfaced to state, no panic |
| **Flatpak mapping** | `/.flatpak-info` present | **every** command is wrapped as `flatpak-spawn --host <prog> …` |

The last row is the one that most needs real coverage: it is a cross-cutting invariant
(AGENTS.md states the rule "NEVER use `std::process::Command` directly"), and today it is
enforced by only 4 unit tests on `map_flatpak_spawn_host` in isolation — nothing asserts
that the *call sites* actually go through it. A test that runs a sampling of flows with
`/.flatpak-info` simulated, asserting the wrapper appears on every command, closes the
gap. This can be done by making the detection injectable rather than a bare
`Path::new("/.flatpak-info").exists()` — **flagged as a required (small) refactor**, since
the current hard-coded path makes the invariant untestable.

**Explicitly out of scope here** (cannot be verified in this environment): real container
lifecycle, real image pulls, real terminal launching, real host `flatpak-spawn` round-trips.
Record these as manual-release-checklist items (§4).

Also delete: `test/widget_test.dart` (the unmodified Flutter counter template — worthless)
along with the rest of the Flutter tree.

### 2.4 Smoke test — `scripts/smoke-test.sh`

#### Environment reality (verified)

- **This machine has a live COSMIC Wayland session** (`XDG_CURRENT_DESKTOP=COSMIC`,
  `WAYLAND_DISPLAY=wayland-1`, Xwayland on `DISPLAY=:1`). So a *real* GUI smoke test is
  possible locally — better than a synthetic one.
- **No `Xvfb`, `xvfb-run`, `weston`, `cage`, or `dbus-run-session` in `PATH`.** Xvfb
  exists only as unbuilt nix derivations (`/nix/store/*xvfb-21.1.24.drv`) and `nix` itself
  is not on `PATH`. So the headless path is **not available locally without installing it**.
- CI (`ubuntu-latest`) can `apt-get install -y xvfb` — that is the supported headless route.

Spec accordingly: **the smoke test must select its display strategy from the environment,
not assume one.**

```bash
#!/usr/bin/env bash
# scripts/smoke-test.sh — launch the built flatpak, assert it stays alive, assert clean exit.
set -euo pipefail

APP_ID=io.github.gosh_distrobox_manager
STAY_ALIVE_SECONDS="${SMOKE_STAY_ALIVE:-10}"

# --- display strategy --------------------------------------------------------
# 1. Real session (local COSMIC): use it directly.
# 2. Headless CI: wrap in xvfb-run, software rendering.
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
  if command -v xvfb-run >/dev/null 2>&1; then
    echo "smoke: no display; using xvfb-run (software rendering)"
    exec xvfb-run -a --server-args="-screen 0 1280x800x24" \
      env LIBGL_ALWAYS_SOFTWARE=1 "$0" "$@"
  fi
  echo "smoke: FATAL — no display and no xvfb-run available" >&2
  exit 1
fi

# --- launch ------------------------------------------------------------------
flatpak run "$APP_ID" &
APP_PID=$!

# --- stay-alive assertion ----------------------------------------------------
sleep "$STAY_ALIVE_SECONDS"
if ! kill -0 "$APP_PID" 2>/dev/null; then
  wait "$APP_PID" || true
  echo "smoke: FAIL — process exited within ${STAY_ALIVE_SECONDS}s" >&2
  exit 1
fi
echo "smoke: process alive after ${STAY_ALIVE_SECONDS}s"

# --- clean-exit assertion ----------------------------------------------------
kill -TERM "$APP_PID"
if wait "$APP_PID"; then
  echo "smoke: PASS — clean exit on SIGTERM"
else
  echo "smoke: FAIL — non-zero exit on SIGTERM" >&2
  exit 1
fi
```

Design notes and required follow-ups:

1. **The stay-alive interval is the core signal.** A libcosmic app that fails to create a
   surface, cannot find its icon theme, or panics in `init()` dies within a second or two.
   Ten seconds is comfortably past that. Make it overridable (`SMOKE_STAY_ALIVE`) because CI
   under software rendering is slower.
2. **SIGTERM must produce a clean exit.** This is the assertion that catches a missing or
   broken shutdown path — a real risk, since teardown of the `CommandRunner`, the
   `tokio` runtime, and any child processes is new code in the migration.
3. **`init()` must do no blocking work.** The current backend spawns commands
   (`TerminalRepository::new` in `supported_terminals.rs` "Asynchronously fetch flatpak
   terminals" at line 135). If the migrated `init()` blocks on that, the app will hang and
   the stay-alive test passes while the UI is frozen. Mitigation: log a readiness line from
   the app and have the smoke test grep the journal for it, rather than only checking
   liveness. **Recommended upgrade** — makes the test assert *responsiveness*, not just
   *existence*.
4. **The smoke test must also assert the negative case: no distrobox installed.**
   This machine has no `distrobox`, and neither does a default CI runner. That is a
   *useful* condition: the app must start, render an empty container list, and not crash
   or hang when the host tool is absent. Exit code 0 with a staying-alive process is the
   pass condition. Optionally assert a "distrobox not found" state is displayed.
5. **Under Flatpak the app needs `--talk-name=org.freedesktop.Flatpak`** to spawn anything.
   `flatpak run` applies the manifest's finish-args, so this is exercised automatically.
   A useful additional assertion: confirm that with the argument *removed*, operations
   fail — proving the argument is load-bearing rather than cargo-culted.
6. **For installing the built flatpak in CI**, either
   `flatpak-builder --user --install --force-clean …` (simplest) or
   `flatpak install --user --noninteractive repo "$APP_ID"`. The former is preferred —
   it avoids needing an OSTree repo round-trip.
7. **Wayland vs X11:** in the local COSMIC session, prefer Wayland (`WAYLAND_DISPLAY` set).
   In Xvfb, `fallback-x11` applies. Both paths should be exercised by at least one CI run
   each if budget allows.

### 2.5 CI

`.github/workflows/flatpak.yml` is **dead as written** (verified): it invokes
`flatpak-builder … io.github.gosh_distrobox_manager.json` — a manifest that does not exist
at the repository root or anywhere else — and then runs `ninja test` and `meson dist`
inside the build sandbox against `.flatpak-builder/build/gosh_distrobox_manager/_flatpak_build`,
a meson build directory that no part of this repository produces. The only meson file in
the tree is `rust/data/icons/meson.build`. The workflow has never run successfully.

**Rewrite it** to:

1. `actions/checkout` (bump from the pinned `@v3`).
2. Install `flatpak`, `flatpak-builder`, `xvfb`, `appstream`, `desktop-file-utils`.
3. Add the flathub remote (keep the existing step — the comment about SDK deps is correct).
4. Run `./scripts/verify.sh` (which contains the flatpak build and smoke test).
5. Publish artifacts on tags only.

Also add a **non-tag CI path**: the current workflow triggers only on `v*.*.*` tags, so
nothing is validated between releases. Add `push`/`pull_request` on the default branch
running stages 1–5 of `verify.sh` (fast feedback) and reserve the full flatpak build +
smoke test for tags and manual dispatch, unless the runtime is pre-cached.

**Caching:** `cargo-sources.json` and the flatpak build dir are large and slow to
regenerate. One clear statement of intent: warm a cache keyed on `flatpak/cargo-sources.json`
and the manifest for stages 6–7. Do not cache across a `cargo-sources.json` change without
also invalidating. §5 quantifies the cost.

---

## 3. Non-COSMIC verification plan (GNOME — the current desktop)

The requirement: the app must work on GNOME, which is this machine's desktop, not just on
COSMIC. libcosmic is *designed* for COSMIC but is a plain winit/iced application and runs
elsewhere; the specific risks are theming, the file picker, and the icon theme.

| Concern | Mechanism | Verification |
|---|---|---|
| **Window creation / rendering** | winit + `fallback-x11`/Wayland | Launch under a GNOME session; window appears, no surface errors |
| **Theming** | libcosmic ships its own theme engine; it does not use GTK themes. It will render *COSMIC-styled*, not Adwaita. **This is expected, not a bug** — document it so it is not reported as one | Visual check |
| **File picker** | Use `ashpd` / XDG **Desktop Portal** (`org.freedesktop.portal.FileChooser`) rather than GTK dialogs. libcosmic exposes an `rfd` feature; configure rfd to use its portal backend, not the GTK backend | Open the export/import picker under GNOME; confirm the GNOME portal dialog appears (not a GTK one) |
| **Icon theme** | On GNOME, `cosmic-icons`/`pop-icon-theme` are not installed. libcosmic resolves icons from the *system* theme at runtime (verified: `build.rs` skips bundled icons on Linux, `src/icon_theme.rs` reads the theme) | Confirm icons fall back gracefully (missing-icon placeholder is acceptable; a panic or blank window is not). Ship a fallback or declare the icon-theme dependency |
| **Portal availability** | Portals need a running `xdg-desktop-portal` + a GNOME backend | `flatpak run` under GNOME; check the picker and any `OpenURI` call |
| **Settings persistence** | cosmic-config writes to `~/.config/cosmic/…` regardless of DE. No COSMIC daemon required for plain file persistence | Round-trip test (§2.2 C); then confirm the file appears under GNOME |
| **`com.system76.CosmicSettingsDaemon`** | Absent on GNOME. Only needed if we adopt libcosmic's `dbus-config` live-watch | Verify the app does not block or error when the daemon is missing. **This is the main GNOME-specific failure risk** for the `dbus-config` feature — see Q5 |

**Concrete GNOME verification steps:**
1. Log into a GNOME (Wayland) session.
2. `flatpak run io.github.gosh_distrobox_manager` — using the *same* built flatpak, not a
   separate build. The point is that one artifact works on both.
3. Exercise: window opens and is themed sanely; container list renders (empty, since no
   distrobox on host); the file picker opens the GNOME portal dialog; settings survive a
   restart; the app closes cleanly.
4. Repeat under a GNOME **X11** session to exercise the `fallback-x11` path.
5. Record screenshots — these also serve as the metainfo screenshots required in §1.5.

**Known limitation:** neither GNOME nor COSMIC is installed as an alternative session on
this machine beyond the running COSMIC session, so steps 1–4 require either a second
machine/VM or installing a GNOME session. **This is a real gap in what can be verified
here** (§5).

---

## 4. Version unification and release checklist

### 4.1 The drift, enumerated

| Artifact | Version today | Correct behaviour |
|---|---|---|
| `rust/Cargo.toml` → `package.version` | **1.0.2** | **The single source of truth** |
| `rust/Cargo.lock` | pins the crate at its version | must match after every bump (`--locked` enforces) |
| `gosh-distrobox-manager.spec` → `Version:` | **1.0.0** | must equal Cargo |
| `gosh-distrobox-manager.spec` → `%changelog` | last entry `1.0.0-1`, dated **Fri Jan 24 2025** | add an entry per release |
| `rust/data/…metainfo.xml.in` → `<releases>` | newest `<release version="1.0.0">` | must equal Cargo; add 1.0.1 and 1.0.2 |
| `gosh-distrobox-manager-1.0.0-linux-x64.tar.gz` | **committed, 18 MB** | stale Flutter artifact; **delete and gitignore** |
| `rust/data/…desktop.in` | no version field | n/a |

Additional spec-file drift worth fixing in the same pass (all verified):

- **`Summary: A Flutter application for managing Distrobox containers`** — wrong post-migration.
- **`BuildRequires: gtk3-devel`, `Requires: gtk3`** — the libcosmic app does not link GTK.
  These must go, and the real build deps (a Rust toolchain, and the vendored sources) must
  be declared. Note the RPM path builds from source now, not "using pre-built Flutter
  bundle" as `%prep`/`%build` currently claim.
- **`%install` hardcodes `%{getenv:PWD}`** and copies from
  `build/linux/x64/release/bundle/` — a Flutter path that will not exist. Rewrite for
  `cargo build --release` + `target/release/gosh_distrobox_manager`.
- **`%files`** references `%{_libdir}/%{name}/` (the Flutter bundle dir) — remove.
- **`%post`/`%postun`/`%posttrans`** call `gtk-update-icon-cache` on hicolor; for a
  GTK-free app this is unnecessary (harmless, but delete).
- **`Requires: distrobox`** — correct and should stay; it is a genuine runtime need.
- **License** `GPL-3.0-or-later` matches the metainfo `project_license` and `COPYING`. Good.
- The spec is a **separate packaging path** from the flatpak. Decide whether it is
  maintained (recommended: yes, for Fedora users) or retired. If maintained, its version
  must be bumped in lockstep — a second place to drift.

### 4.2 Release checklist

Every release, in order:

- [ ] Bump `rust/Cargo.toml` `package.version` (single source of truth).
- [ ] `cargo build` (no `--locked`) to refresh `rust/Cargo.lock`; commit the lock.
- [ ] Add a `<release version="X.Y.Z" date="YYYY-MM-DD">` block to the metainfo, with a
      real description. Do not remove older entries.
- [ ] Bump `Version:` in `gosh-distrobox-manager.spec`; add a `%changelog` entry with
      today's date in `Day Mon DD YYYY` format.
- [ ] Regenerate `flatpak/cargo-sources.json` from the updated `Cargo.lock`.
      **Re-pin `libcosmic` if the rev moved**, and re-record the `iced` submodule SHA (§1.3).
- [ ] Run `./scripts/verify.sh` — must pass end to end on a clean tree.
- [ ] Confirm `--locked` passes (proves Cargo.toml ↔ Cargo.lock agreement).
- [ ] Validate metainfo + desktop file (§1.5).
- [ ] Manual verification of anything the environment cannot test (§5):
      real container create/delete with `distrobox` + `podman` actually installed;
      terminal launching; a real `flatpak-spawn --host` round-trip.
- [ ] Regenerate screenshots (currently Flutter UI) and update the metainfo `<screenshots>`.
- [ ] Tag `vX.Y.Z`; CI builds, smoke-tests, and attaches artifacts.
- [ ] Verify the three versions agree post-release (Cargo, spec, newest metainfo release) —
      a 10-second grep that would have caught the current 1.0.0 / 1.0.2 drift.

**Suggested guard:** a `scripts/check-versions.sh` that greps all three and exits non-zero
on mismatch, added as an early stage in `verify.sh`. This turns the recurring drift class
into an automated failure instead of a review comment.

---

## 5. Risks

### 5.1 Vendoring tree size and build time

- **Scale reference:** CosmicTweaks' `cargo-sources.json` is **1563 source entries**
  (verified: parsed). Roughly 802 are `inline` (vendored crate contents embedded in the
  JSON). Expect our generated file to be in the same order of magnitude with libcosmic,
  iced, wgpu, and the COSMIC stack. This is a **multi-megabyte checked-in JSON file** and
  it will churn on every dependency change.
- **Build time:** every build recompiles the whole libcosmic + iced + wgpu graph from
  source, because the BaseApp does not ship a prebuilt libcosmic (§1.1). This is a
  **long** build — expect tens of minutes cold. CosmicTweaks accepts this, so it is
  normal for COSMIC flatpaks, but it makes CI-on-every-push unattractive and makes
  aggressive caching important (§2.5).
- **Mitigations:** cache the flatpak build dir in CI keyed on `cargo-sources.json`; keep
  the fast stages (1–5) as the per-push gate; reserve the full build for tags and manual
  dispatch; consider `mold` (shipped by the extension) via `RUSTFLAGS="-C link-arg=-fuse-ld=mold"`
  to cut link time, which dominates large Rust builds.
- **Disk:** 372 GB free locally, so no local constraint. CI runners are far tighter —
  a full COSMIC Rust build plus variant dirs can approach GitHub's runner limits. Watch this.
- **`cargo-sources.json` review burden:** a 1500-entry generated diff is unreviewable by
  hand. Treat it as a generated artifact; its review gate is "does the build succeed
  offline", not line-by-line inspection.

### 5.2 Sandbox permission gaps

- **`--talk-name=org.freedesktop.Flatpak` is effectively full host access.** `flatpak-spawn
  --host` lets the sandboxed app run arbitrary commands as the user. This is unavoidable
  given the architecture (the app is a *manager* for a host-side tool), and comparable
  apps do the same, but **Flathub reviewers scrutinise it heavily** and will ask why.
  Prepare the justification: the app's entire purpose is to drive host `distrobox`/`podman`;
  there is no in-sandbox alternative; the alternative (`--filesystem=host` + bundled
  distrobox) is strictly worse. **Raise this early rather than at submission.**
- **The `--filesystem` grants are narrow on purpose (§1.4).** If the migrated app adds any
  direct filesystem access — reading a config, writing a log, caching icons — the grant
  list must be revisited. **Every new direct `std::fs` call in the sandboxed process is a
  packaging change, not just a code change.** Recommend a CI grep that fails on new
  `std::fs`/`File::` usage outside an allowlist, to keep this honest.
- **The `distroshelf-terminals.json` grant is fragile** (§1.4.2): a filename/path mismatch
  fails silently at runtime. Highest-probability silent failure in the whole design.
- **`--device=dri` under software rendering** (CI/Xvfb) is present but unused — harmless,
  but do not let a green smoke test under Xvfb be mistaken for proof that GPU rendering works.
- **Over-granting is a real temptation here** because the app "obviously needs the host".
  Resist: each omitted argument (§1.4.1) is deliberate.

### 5.3 What CAN be verified in this environment

- Build, clippy `-D warnings`, fmt, all unit tests — **fully**, offline.
- The flatpak build itself (`flatpak-builder`), including that vendoring resolves and the
  build succeeds **offline** — this is a strong, end-to-end check of the whole §1.3 strategy.
- `cargo-sources.json` correctness, by construction: the sandbox has no network, so a
  vendoring gap is a hard build failure, not a silent fallback. **A successful offline
  flatpak build is a meaningful proof.**
- Launching the built flatpak in the **live COSMIC Wayland session**; stay-alive; clean
  SIGTERM exit.
- The "host tool missing" path — since `distrobox` is genuinely absent, that negative case
  is tested for free.
- Desktop-file and metainfo validation (via the SDK or CI).
- That `--talk-name=org.freedesktop.Flatpak` is load-bearing, by removing it and observing
  failure.
- All distrobox argv construction, via the existing 70 tests plus new `NullCommandRunner`
  integration tests.

### 5.4 What CANNOT be verified in this environment

- **Any real container operation.** No `distrobox`, no `podman`, no `docker` (all verified
  absent). Container create/delete/stop/upgrade/clone, image pulls, package install/search,
  snapshots, stats — **all unverifiable here**, at any level above argv assertion.
- **A real `flatpak-spawn --host` round-trip doing something meaningful** — the plumbing can
  be shown to invoke, but with no distrobox on the host there is nothing for it to run.
- **Real terminal launching** (no terminal emulator guaranteed; and §1.4.2's concerns).
- **The non-COSMIC / GNOME matrix** (§3) — needs a GNOME session not present here.
- **Headless CI smoke locally** — no `xvfb-run` (§2.4). Verifiable only in CI.
- **GPU rendering** — the smoke test in CI is software-rendered; GPU paths need a real session.
- **Flathub review acceptance** — the `org.freedesktop.Flatpak` grant is a judgement call
  by reviewers, not something a local build can settle.

The honest summary: **everything up to and including "the app builds, ships, launches, and
talks to the host correctly" is verifiable. Everything beyond that — that it actually
*manages containers* — is not, and must be a documented manual release step.**

---

## 6. Open questions for the devil's advocate

1. **Is `com.system76.Cosmic.BaseApp` worth the dependency?** It buys `cosmic-icons` +
   `pop-icon-theme` and costs us a base app in the dependency chain. Since libcosmic on
   Linux resolves icons at runtime from the system theme, could we instead ship the handful
   of icons we need and drop the base? Or is the base the only sane way to get correct
   COSMIC iconography? **Decide before writing the manifest — it is structural.**

2. **Should `libcosmic` be pinned as a git source in `cargo-sources.json`, or vendored into
   the repo?** The git route is flathub-standard and keeps the repo small, but it means
   every build re-clones/checks out libcosmic + the iced submodule, and reproducibility
   depends on flatpak-builder's submodule checkout behaviour continuing to hold. Vendoring
   (via `cargo-vendor-filterer`, which the extension ships) would make builds hermetic and
   submodule-independent at the cost of a large committed tree. **Which failure mode do we
   prefer?**

3. **The `iced` submodule is fetched from a network URL that is not recorded in our
   manifest.** Our reproducibility depends on a SHA recorded in *pop-os/libcosmic's* tree,
   not ours. If that pointer ever changes (force-push, re-tag), our build silently changes.
   Should we vendor iced explicitly to close this, or accept it and record the expected
   SHA (§1.3) as documentation only? **This is the sharpest reproducibility question in
   the plan.**

4. **Does the migrated app need `--share=network` at all?** The plan says no, because all
   container operations run on the host. But if the new UI fetches anything in-sandbox
   (update checks, release notes, icon packs, crash reporting), that answer flips. **Confirm
   against the actual Phase 2 feature set before Flathub submission** — adding a permission
   later is easy; removing one after users rely on it is not.

5. **Do we adopt libcosmic's `dbus-config`, and if so what happens on GNOME?** `dbus-config`
   uses `com.system76.CosmicSettingsDaemon`, which does not exist on GNOME. If we enable it
   we may need `--talk-name=com.system76.CosmicSettingsDaemon` (§1.4.1) and we must confirm
   the app degrades gracefully without the daemon — this is the **main GNOME-specific
   failure risk** (§3). Alternatively use plain cosmic-config file persistence and skip
   live-watch entirely. **Which, and what is the fallback when the daemon is absent?**

6. **Is `--filesystem=home` genuinely avoidable, or does `flatpak-spawn --host` have a
   caveat I have missed?** My C1 analysis is that no sandboxed code touches host home
   paths. But distrobox export writes `.desktop` files to `~/.local/share/applications` and
   the app then *displays* them — confirm the migrated code never opens those files from
   inside the sandbox (only via the host script). **If wrong, the permission set changes
   materially and the Flathub story gets harder.**

7. **Should the RPM spec be maintained at all?** It is a second packaging path that will
   drift (it already has). Options: maintain both in lockstep with an automated version
   check (§4.2), or retire the spec and ship Flatpak only. **Which is the supported
   distribution story?**

8. **What is the actual `Message` enum stability requirement?** §2.2 mandates a test per
   variant. If the enum is large and churns during Phase 2, that test suite becomes a tax.
   Is per-variant coverage the right bar, or is a smaller set of high-value behavioural
   tests plus a "no unhandled variant" exhaustiveness check sufficient? **Agree the bar
   before writing tests, not after.**

9. **Is deleting the GSettings schema safe for existing users?** The schema is already
   unread by any source (§1.6), so nothing breaks — but if any *shipped* build ever read it
   (the pre-Flutter GTK version?), users' saved terminal and window settings are silently
   lost on upgrade. **Confirm no released version ever persisted through GSettings**, or
   plan a one-time migration read.

10. **Does the smoke test prove enough?** A stay-alive + clean-SIGTERM check passes for an
    app that renders a blank window and does nothing. Is a readiness signal (§2.4 note 3)
    mandatory, and should the smoke test additionally assert that a *known UI element*
    exists (e.g. via a debug/IPC hook) rather than merely that the process lives? **What is
    the minimum bar that would actually have caught a real regression?**

11. **Vendoring churn vs. lockfile discipline.** `--locked` in `verify.sh` assumes
    `Cargo.lock` is always in sync. During an active migration that will fail constantly.
    Should `--locked` apply only in CI (where the lock is the contract) and not in the
    developer loop, or is failing fast locally worth the friction?

12. **Who owns the `cargo-sources.json` regeneration?** It is generated, large, and must be
    refreshed on every dependency change. Is it regenerated by a human before each release
    (§4.2), by CI, or by a pre-commit hook? An unowned generated artifact is a future
    broken build.
