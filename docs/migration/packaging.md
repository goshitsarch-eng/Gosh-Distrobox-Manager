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
  "env": {
    "CARGO_HOME": "/run/build/gosh_distrobox_manager/cargo",
    "CARGO_NET_OFFLINE": "true",
    "RUSTFLAGS": "-C link-arg=-fuse-ld=mold"
  }
},
"modules": [
  {
    "name": "gosh_distrobox_manager",
    "buildsystem": "simple",
    "build-commands": [
      "cargo --offline build --release --locked --verbose",
      "install -Dm0755 target/release/gosh_distrobox_manager -t /app/bin/",
      "install -Dm0644 core/data/io.github.gosh_distrobox_manager.desktop /app/share/applications/io.github.gosh_distrobox_manager.desktop",
      "install -Dm0644 core/data/io.github.gosh_distrobox_manager.metainfo.xml /app/share/metainfo/io.github.gosh_distrobox_manager.metainfo.xml",
      "install -Dm0644 core/data/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg /app/share/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg",
      "install -Dm0644 core/data/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg /app/share/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg",
      "install -d /app/share/gosh_distrobox_manager/distro-icons",
      "install -Dm0644 core/data/icons/*.svg -t /app/share/gosh_distrobox_manager/distro-icons/"
    ],
    "sources": [
      { "type": "file", "path": "../Cargo.toml", "dest": "." },
      { "type": "file", "path": "../Cargo.lock", "dest": "." },
      { "type": "dir", "path": "../core", "dest": "core" },
      { "type": "dir", "path": "../app", "dest": "app" },
      "cargo-sources.json"
    ]
  }
]
```

Notes:
- **Paths are post-T1, not `rust/`.** T1 (`git mv rust core`) made the repository a
  *virtual* workspace at the root, so `Cargo.toml`/`Cargo.lock` are at the repo root,
  `target/` is `<root>/target/`, and the data files are `core/data/…`. The pre-T1 draft
  of this block said `data/…`, which was correct only while the build directory root
  was `rust/`. Source `path`s are resolved relative to the **manifest's** directory
  (`flatpak/`), hence the `../`.
- `CARGO_HOME` **must** be `/run/build/<module-name>/cargo` — the path is keyed to the
  module name. Getting this wrong produces a "registry not found" offline failure.
- `--offline` is mandatory, and `CARGO_NET_OFFLINE=true` is set in addition so the
  sandbox cannot silently fall back to the network if `--offline` is ever dropped from
  the command line. See §1.3 ("`--share=network` must be OFF for the build").
- `--locked` is deliberate: it fails if `Cargo.lock` disagrees with `Cargo.toml`, which
  is the drift class §4 exists to eliminate.
- `RUSTFLAGS=-C link-arg=-fuse-ld=mold` is §5.1's mitigation. `mold` and `ld.mold`
  ship in the rust-stable extension's `bin`, which `append-path` puts on `PATH`, so
  the flag resolves inside the sandbox with no extra module (verified: §0.1).
- **In-repo manifests use local `file`/`dir` sources; a Flathub submission replaces
  them with a single `git` source pinned to the release commit.** `<TAG_COMMIT_SHA>` in
  the pre-T1 draft of this block was an unfilled placeholder and could never have built:
  a local manifest cannot reference a commit that is not pushed. The local form is what
  `verify.sh` §2.1 stage 9 builds and what T2's gate was run against; the switch is a
  release-checklist item (§4.2), not a design change.
- Source installs are flat `.desktop`/`.metainfo.xml` files, **not** `.in` templates.
  The `.in` files currently in `core/data/` require `@bindir@`-style substitution that
  nothing in a pure-Rust build performs. See §1.5. **These plain files do not exist yet**
  — §1.5 is the task that creates them, and until it lands the three `core/data/…`
  install lines fail; see §2.5's sequencing note.
- The distro-logo install target `/app/share/gosh_distrobox_manager/distro-icons/` is
  **provisional**: nothing in `core/src` reads an icon path today (the Flutter UI used
  Material icons, not these SVGs), so the runtime lookup path is T3/T4's to fix. Whatever
  it becomes must match this line exactly — a mismatch here is the §1.4.2 silent-failure
  class, reintroduced by a filesystem path rather than a permission. See §2.5.

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
| → git dep `cosmic-settings-daemon` | `https://github.com/pop-os/dbus-settings-bindings` — **no `rev`**; resolved `eed01dd3609e90e3c8cd043656734c500956c793` |
| → git dep `cosmic-freedesktop-icons` | `https://github.com/pop-os/freedesktop-icons` — **no `rev`**; resolved `ab4c57b8e416c6af9297cb04d101889896fd9a92` |

**There are more than one of these pointers, and two of them are not pinned by anyone.**
Beyond libcosmic itself and its two submodules, libcosmic's Linux target block
(`Cargo.toml:175` and `:180`) declares two further git dependencies:

```toml
cosmic-settings-daemon = { git = "https://github.com/pop-os/dbus-settings-bindings" }
freedesktop-icons = { package = "cosmic-freedesktop-icons", git = "https://github.com/pop-os/freedesktop-icons" }
```

Both sit in unconditional `cfg(unix, …)` / `cfg(unix, not(macos))` blocks, so they are
**always compiled, never feature-gated away** — no feature list of ours can drop them. And
neither carries a `rev` or `tag`, so unlike the submodule pointers above they are not even
recorded in *someone's* tree: cargo resolves them to the default branch HEAD at lockfile
generation time. Our reproducibility therefore rests on **three** git pointers, not one
(REVIEW A8; `PLAN.md` row "Further git pointers"), and the two unpinned ones are the weaker
pair. This is exactly what the sidecar mechanism (`git-manifests/` + `git-packages.json`,
§1.3 procedure below, D12/PKG-3) converts from silent drift into a hard, offline,
CI-detectable failure: `generate-cargo-sources.py --check` asserts that the lockfile's git
sources are precisely the ones the sidecar describes.

The submodule SHAs are **recorded here for documentation only** — they are not written into
the manifest, because flatpak-builder reads them from the libcosmic tree. Recording them
matters for *reproducibility auditing*: if libcosmic's tree is ever force-dated or the
submodule pointer moves, these are the values a build must be reproducing.

The two git-dep SHAs are recorded the same way, for the same auditing purpose, with one
caveat they do not share with the submodules: they are the upstream default-branch HEADs
*observed on 2026-09-11*, not values anyone pins. They are expected values for "what this
plan was verified against", not a guarantee — the authoritative record is the sidecar
committed in T2, which is regenerated with the lockfile and checked in CI.

##### Correction (T2, measured): the lockfile has **eleven** git sources, not five

The five entries above are correct as *pointers* — the things a human has to think about
— but they are not the set of git sources `Cargo.lock` records, and the sidecar is keyed
on the latter. Resolving libcosmic at the pinned rev with `PLAN.md` §1's feature list
(`default-features = false`, `dbus-config` dropped per D23) and taking the lockfile's
distinct `source = "git+…"` strings yields **11** sources:

| # | Git source (as it appears in `Cargo.lock`) | Pinned by |
|---|---|---|
| 1 | `pop-os/libcosmic?rev=a401af8b…` | our `rev` |
| 2 | `pop-os/dbus-settings-bindings` | **nobody** (branch HEAD) |
| 3 | `pop-os/freedesktop-icons` | **nobody** (branch HEAD) |
| 4 | `pop-os/cosmic-protocols?rev=32283d7` | libcosmic |
| 5 | `jackpot51/rust-atomicwrites` | `cosmic-config` |
| 6 | `pop-os/winit.git?tag=cosmic-0.14` | `iced` |
| 7 | `pop-os/softbuffer?tag=cosmic-4.0` | `iced` |
| 8 | `pop-os/window_clipboard.git?tag=sctk-0.20` | `iced` |
| 9 | `pop-os/smithay-clipboard?tag=sctk-0.20` | `iced` |
| 10 | `iced-rs/cryoglyph.git?rev=e429a025…` | `iced` |
| 11 | `wash2/accesskit?tag=cosmic-0.14` | `iced` |

Sources 4–11 are *inside* the `iced` submodule's own dependency graph and are reached
only because the submodule is populated; they are not visible from libcosmic's manifest.
Two consequences:

- **The `iced` submodule is not merely a build-time convenience — it is load-bearing for
  the lockfile's git set.** Unpinning it changes sources 6–11.
- **The sidecar must cover all eleven, and it does** (T2: `git-packages.json` has 11
  remotes / 51 packages; `git-manifests/` has 51 normalised manifests). D24's "five" and
  REVIEW A8's "three" are both counts of *documented pointers*; neither is a count of
  lockfile sources. The generator is agnostic — it derives the set from the lockfile —
  so nothing needs re-deciding, but a reviewer counting five entries in the sidecar would
  wrongly conclude it was stale.

The measured sources 1–3 resolve to exactly the SHAs recorded above, independently
confirming this section's pins (`eed01dd3…`, `ab4c57b8…`) on 2026-09-11.

libcosmic at this rev: `version = 1.0.0`, `edition = "2024"`, `rust-version = "1.93"`,
`[lib] name = "cosmic"`. All verified from the fetched `Cargo.toml`. MSRV 1.93 < our
1.98.1, so no toolchain problem.

#### Procedure to (re)generate `cargo-sources.json`

**This section replaced the pre-T2 draft, which described upstream's
`flatpak-cargo-generator.py`. We do not use it** (D5): it cannot express the `iced`
submodule's path dependencies or the per-package subdirectory layout, and it emits no
sidecar. The ported generator in `flatpak/` is the tool, and it is the same one the
sibling project ships — see `flatpak/generate-cargo-sources.py`.

```bash
# One-time, on a networked machine, and again whenever the *git* set in Cargo.lock
# changes (a new/removed/pinned git dependency). Network is needed here and only here.
python3 flatpak/generate-cargo-sources.py --refresh-git

# After any dependency change at all. Offline; no network. This is the CI step.
cargo update                      # or edit Cargo.toml; then, from the repo root:
python3 flatpak/generate-cargo-sources.py
python3 flatpak/generate-cargo-sources.py --check   # must exit 0
```

Three facts about the tool that matter when reading its output:

- **The lockfile is the only input that pins anything.** `git-packages.json` carries no
  URL and no commit — the lockfile's source string is the dict key, and every `git`
  source, `commit`, and `[source."…"]` config key the generator emits is derived from it
  by `parse_git_source`. This is why a pin cannot drift between two files (R8), and why
  `--check` is a real staleness test rather than a diff of two copies of the same data.
- **`--refresh-git` runs `cargo vendor --locked --versioned-dirs` against `--project`**,
  which defaults to the **workspace root** (the sibling defaulted to `rust/`; T1 moved
  the workspace manifest to the root and this generator follows it). Pointing `--project`
  at `core/` instead would vendor only that member's subgraph and silently omit
  everything `app/` pulls in.
- **`--refresh-git` populates submodules** (`git submodule update --init --recursive`)
  before harvesting. That is separate from, and does not replace, flatpak-builder's own
  submodule checkout at build time (§1.3). Both must hold; neither is redundant.

Expected shape at the pinned rev with `PLAN.md` §1's feature list: 11 git remotes,
51 packages, 51 normalised manifests. A `--refresh-git` that reports a different package
count means the iced submodule pointer moved.

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
  "--talk-name=org.a11y.Bus",
  "--filesystem=xdg-config/cosmic:rw"
]
```

| Argument | Justification |
|---|---|
| `--share=ipc` | Shared-memory transport for the toolkit; required for X11 shared-memory and standard for every GUI flatpak (CosmicTweaks, GNOME apps all carry it). |
| `--socket=wayland` | Primary display protocol on COSMIC (verified: `WAYLAND_DISPLAY=wayland-1`). |
| `--socket=fallback-x11` | X11 only when no Wayland compositor is present. **Deliberately `fallback-x11`, not `x11`** — `x11` grants X11 access even when Wayland is available, which is a needless widening. CosmicTweaks uses `fallback-x11`. |
| `--device=dri` | GPU access for wgpu/GL rendering. Without it the app falls back to software rendering or fails to create a surface. |
| `--talk-name=org.freedesktop.Flatpak` | **Load-bearing.** Permits `flatpak-spawn --host`, which is how *every* distrobox/podman command escapes the sandbox. `rust/src/app_state.rs:18` and `rust/src/api.rs:145` install `map_flatpak_spawn_host` whenever `/.flatpak-info` exists. **Without this argument the app is completely non-functional under Flatpak** — it would appear to launch and then every operation would fail. |
| `--talk-name=org.a11y.Bus` | **Load-bearing for the P0 a11y claim.** AT-SPI lives behind the accessibility bus, which the sandbox does not see by default. Without this name `accesskit_unix` is inert: Orca and any other screen reader would find an app with no accessibility tree at all, making `ux.md §5.3`'s P0 rating false on the only shipping channel (REVIEW E2/UX-16). Measured in the sibling project, whose manifest carries exactly this argument. |
| `--filesystem=xdg-config/cosmic:rw` | Read-write access for cosmic-config (`~/.config/cosmic/<app-id>/v1/…`). This is how settings persist under libcosmic; it replaces the GSettings backend being deleted (§1.6). CosmicTweaks carries the equivalent. |

**Why `:rw` and not the sibling's `:ro`.** The sibling manifest carries
`--filesystem=xdg-config/cosmic:ro` and it is tempting to copy the character-for-character
grant — but the two apps use this directory differently. The sibling only *watches*
COSMIC's config (it is an observer of system-wide theme keys), so read-only is sufficient
and strictly safer. **We write into it**: cosmic-config persists this app's own settings at
`~/.config/cosmic/io.github.gosh_distrobox_manager/v1/`, which is exactly what replaces the
deleted GSettings backend. Downgrading to `:ro` would make every settings write fail at
runtime, silently, inside the sandbox — the failure class §5.4 exists to catch. `:rw` is the
minimum grant that supports stated behaviour; do not "fix" it toward the sibling's `:ro`
without first removing config persistence.

#### 1.4.1 Deliberate omissions (and why)

- **`--filesystem=home` — omitted.** See correction C1. Nothing in the sandboxed process
  touches host home paths; distrobox/podman and the desktop-file scanner all run on the
  host via `flatpak-spawn`. Granting it would be an unjustified over-grant and a likely
  Flathub review rejection.
- **`--share=network` — omitted.** No in-sandbox HTTP client exists. Container image
  pulls are performed by host `podman`, outside the sandbox. **Open question Q4** if the
  new UI ever fetches anything itself (release notes, icon packs, update checks).
- **`--socket=x11` — omitted** in favour of `fallback-x11` (§1.4).
- **`--talk-name=com.system76.CosmicSettingsDaemon` — omitted, permanently.** Q5 is closed
  by D23: we do **not** enable libcosmic's `dbus-config` feature, so `Cosmic::init` never
  builds the settings-daemon proxy (`libcosmic/src/app/cosmic.rs:119-124`) and
  `watch_config` always takes the `config_subscription` file-watcher path
  (`libcosmic/src/core.rs:392-404`). Nothing we compile ever contacts that daemon, so the
  name is not needed on COSMIC and its absence costs nothing on GNOME. This is the
  measured sibling configuration (its D27b), and the 15-errors-per-20s failure it
  eliminates was the reason. Add this name back only together with `dbus-config`, on a
  two-build measurement showing a need.
- **`--filesystem=xdg-data/distroshelf-terminals.json:rw` — removed.** The terminal list
  moves into cosmic-config (ARCH-Q5/D11), which deletes the file, the path, and this grant
  together. See §1.4.2 — and note the consequence below: this was the *only* direct
  filesystem access in the sandboxed process, so the sandbox now holds **no** writable
  path outside `xdg-config/cosmic`.
- **`--filesystem=xdg-config/gtk-3.0:ro`, `xdg-config/gtk-4.0:ro`, `xdg-config/kdeglobals:ro`,
  `xdg-data/color-schemes:ro` — omitted.** These are CosmicTweaks-specific (it previews
  GTK/KDE themes). libcosmic themes itself.
- **`--device=all`, `--allow=devel`, `--socket=session-bus`, `--filesystem=host` — never.**
  Each is a broad escalation with no use here.

**PKG-6 follow-through: the per-file allowlist is now empty.** Because the terminals grant
was the only host path the sandboxed process touched (A7/C1), the final set has no
`--filesystem=` entry for a single file and no path that must match a Rust constant
character-for-character. That removes §5.2's "new direct `std::fs` usage" failure mode at
the root rather than guarding it, and it is the strongest available answer to §5.2: the
sandbox can no longer be misconfigured into a silent write error, because it has nothing
left to misconfigure. Keep it that way — a future `--filesystem=<single file>` grant
reintroduces the whole class and needs the unit test §1.4.2 used to require.

#### 1.4.2 The `distroshelf-terminals.json` problem — resolved by deletion

The only sandbox-visible host path *was* named after the **pre-rename project name**
(`distroshelf`). This was rename drift of the same family as the metainfo/spec drift in §4.
The two options were:

1. **Rename it** to `gosh_distrobox_manager-terminals.json`, update
   `supported_terminals.rs:112-114`, and grant `xdg-data/gosh_distrobox_manager-terminals.json:rw`.
   Cost: existing custom terminal lists are silently dropped (a one-time migration shim
   could read the old path, but this is a pre-1.0 app and the drift is already user-visible).
2. **Fold it into cosmic-config**, making the file disappear and the grant unnecessary.
   This is the cleaner outcome — it aligns with §1.6's rationale for deleting GSettings.

**Option 2 is taken (ARCH-Q5, D11).** The terminal list becomes a cosmic-config entry
alongside every other setting, `dirs::data_dir().join("distroshelf-terminals.json")`
disappears, and with it the grant, the drift, and the whole path-mismatch failure mode:

- **No grant to keep in sync.** The `xdg-data/distroshelf-terminals.json:rw` entry is gone
  from §1.4. Nothing in the manifest now names a single file, so there is no pair of
  string constants that can silently disagree.
- **No silent write error to guard against.** The old failure was a mismatch between the
  Rust path constant and the manifest grant, which surfaced only at runtime and only as an
  `error!` log line (the write path logs nothing louder). With the file gone, that failure
  cannot be constructed.
- **One persistence mechanism, not two.** Settings and terminals both live under
  cosmic-config, so the §1.6 GSettings deletion leaves exactly one config backend rather
  than one-and-a-half.
- **The unit test §1.4.2 used to mandate is no longer required** — there is no resolved
  path constant left to assert. The equivalent coverage moves to the config round-trip
  test in §2.2, which exercises real persistence instead of a path string.

The migration cost is the same one option 1 carried: an existing user's custom terminal
list is not carried over automatically. That is accepted for a pre-1.0 app; if it proves
to matter, the DistroShelf import path (PKG-9) is the natural place to read the old file
once, on the host, through the runner.

### 1.5 Desktop file, metainfo, and icon fixes

**Outcome (T14).** Both copies named below were resolved to one file, and neither
surviving path is the one this section predicted. The template (`*.desktop.in`) and
the Flutter tree's copy were **deleted**; the single source is
`core/data/io.github.gosh_distrobox_manager.desktop`, a plain non-`.in` file
installed verbatim by both the Flatpak manifest and the RPM spec. `core/` replaced
`rust/` as the crate name in T1/S2, which is why the recommendation below says
`rust/data/` — read it as `core/data/`.

The `.in` templates that existed to feed the meson build are all gone with it:
`io.github.gosh_distrobox_manager.{desktop,metainfo.xml,service}.in` and
`gschema.xml`. The generated `.desktop` and `.metainfo.xml` are now the real
files rather than build products, and `core/data/icons/meson.build` went too.

At this section's writing time there were **two** desktop files and they disagreed:

| File | State |
|---|---|
| `rust/data/io.github.gosh_distrobox_manager.desktop.in` | GTK-era template. **Stale.** Deleted in T14. |
| `linux/data/share/applications/io.github.gosh_distrobox_manager.desktop` | Flutter-era. Richer, but also needs edits. Deleted in T14. |

**Recommendation (as written):** keep exactly one, as a plain (non-`.in`) file under `rust/data/`,
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

**This sketch was implemented with five changes; the live script is authoritative.**
Read `scripts/verify.sh` itself rather than transcribing from here. The divergences,
all of which landed during T4 and were exercised by T14:

- It grew from 7 stages to **11**, and now wraps each command in a `stage N "<label>"`
  helper that prints a banner, so a failure is attributable without reading the whole
  log. `--fast` runs stages 1–8 and skips stages 9–11 (the flatpak build and both
  smoke runs). `EXPECTED_STAGES` in the script is the single source of the count —
  this number, the workflow comments and `tests/test_packaging.py` assert against
  it (T21).
- **`--manifest-path rust/Cargo.toml` → `--workspace`** at the repository root. T1
  hoisted `Cargo.lock` and a virtual manifest to the root and renamed the crate to
  `core/`, so `rust/Cargo.toml` has not existed since S2. The sketch's `--locked`
  intent survives.
- **`rust/data/` → `core/data/`** in stage 6 (S2's rename), and stage 6 validates the
  generated `.desktop`/`.metainfo.xml` rather than `.in` templates.
- **Three stages the sketch did not anticipate**: 5 `check-versions.sh` (D14 — why the
  sketch's stage numbering in prose below is off by one from the script's), 7 the
  offline vendoring staleness gate `generate-cargo-sources.py --check` (T2/D26), and 8
  `tests/test_packaging.py` (T4/D26, stdlib only, no pytest).
- **Stage 9 uses `--user --install` plus `--repo`, not `--repo` alone** (D26d):
  the sketched `--repo=repo` form never installed anything for the smoke stages to
  test. The repo (`.flatpak-builder/repo`) is retained so the tag job can
  `flatpak build-bundle` the release from it (T21/I20). Stages 10 and 11 run the
  smoke test twice — a positive run and a negative one asserting the app fails
  cleanly *without* the `flatpak-spawn` grant (two stages since T16, not one that
  prints twice).

The original sketch follows, unedited, as the record of what was intended.

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

**Outcome (T14).** The rewrite below was carried out — by T4 for the flatpak job, not
by T14. T14 checked it and found one remaining inaccuracy, which it fixed: the
workflow's header comment claimed "then artifact publish on tags" but **no step
uploads an artifact**, so the comment advertised a capability the job does not have.
It now says so explicitly, and also records that the Flathub git-source switch
(§4.2) has not happened. T14 deliberately did **not** delete the workflow, which the
comment in `rust.yml` had been claiming was dead — by then it was working code, and
this section's own diagnosis is the reason it is easy to mistake for dead.

The three jobs are now: `rust.yml` (fmt + build + clippy + test), and
`packaging-metadata.yml` + `flatpak.yml` (both drive `./scripts/verify.sh`, `--fast`
and full respectively, so CI and the local gate cannot drift apart). T14 dropped the
`cosmic-migration` branch entry from the two `push` triggers, since the migration
branch is merged; `main` plus the nightly schedule is what remains.

**Outcome (T21 — I20 closed, arm (a)).** The tag job now exports a `.flatpak`
bundle from the OSTree repo `verify.sh` stage 9 retains and attaches it to the
GitHub Release for the tag (`flatpak.yml`, job-scoped `contents: write`).
`actions/upload-artifact` was deliberately not used — run storage is not
release assets. Arm (b) (drop the tags trigger) was rejected: job 3 is
specified "tags + `workflow_dispatch` only" and D19 requires T3 to gate the
release, so deleting the trigger would remove the only shippable-artifact
certifier. `rust.yml` now calls `verify.sh --fast` instead of re-spelling
fmt/build/clippy/test, so all three jobs share the one script. Unverified from
here: the `workflow_dispatch` dry run and a fork tag showing the bundle
attached — both need a GitHub remote, so they are release-checklist items
(§4.2), not claims.

The original diagnosis follows, unedited. `.github/workflows/flatpak.yml` is **dead as written** (verified): it invokes
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

#### 2.5.1 What CI runs — the job plan (T2)

**This subsection is the specification of the CI jobs, not their implementation.**
`.github/workflows/rust.yml` is **not** edited by T2 — T4 owns that file and owns wiring
these steps into `scripts/verify.sh` (D13). What T2 fixes here is *what the jobs do and in
what order*, so T4 has an unambiguous target and so the `--check` gate below has a named
home.

Three jobs, cheapest first. Stages 1–8 are the per-push gate; 9–11 are tag/dispatch
only. (Written as 1–5 / 6–7 when the gate had 7 stages; re-numbered in T21 — the
sketch's prose below keeps its original numbers as the record of what was intended.)

| # | Job | Runs on | Steps |
|---|---|---|---|
| 1 | `rust` | every push + PR | `cargo fmt --all -- --check`; `cargo build --workspace --release --locked`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --locked` |
| 2 | `packaging-metadata` | every push + PR | the block below |
| 3 | `flatpak` | tags + `workflow_dispatch` only | full `./scripts/verify.sh` (stages 1–11 — the tag build certifies the whole gate, not just 9–11); on tags, export a `.flatpak` bundle from the retained repo and attach it to the GitHub Release (T21/I20 arm (a)) |

Job 3 needs two build-directory decisions that §2.1's draft does not settle, both of
which leave untracked files in the tree if taken literally (T2, measured):

- `--repo=repo` in §2.1 writes an OSTree repository to `<root>/repo/`, and **`repo/` is
  not in `.gitignore`** (verified: `git check-ignore repo/` → no match). `build/` *is*
  ignored, but only incidentally — by the Flutter-era `build/` entry, not because anyone
  intended it. Prefer retargeting both to `--repo=.flatpak-builder/repo` and
  `--state-dir=.flatpak-builder/state`: that directory is self-ignored, because
  flatpak-builder writes its own `.flatpak-builder/.gitignore` containing `*`. This is
  also what the sibling's `build-flatpak.sh` does. T4 owns `.gitignore` and should decide;
  T2 deliberately did not add an entry, since a guessed path would only mask the choice.
- flatpak-builder creates `.flatpak-builder/` (ccache + checksums) in its **current
  working directory** unless `--state-dir` says otherwise. T2's gate runs did exactly that
  at the repository root. It is self-ignored, so it is harmless, but pinning
  `--state-dir` makes the side effect explicit and keeps the cache in one place.

Job 2 — `packaging-metadata` — is new, and is where T2's generator lands. It is the job
that makes §4's drift class and §1.3's vendoring drift fail in CI instead of in review:

```yaml
# packaging-metadata job, in order. All offline, all fast (<1 min).
- run: python3 flatpak/generate-cargo-sources.py --check      # T2 — see below
- run: desktop-file-validate core/data/io.github.gosh_distrobox_manager.desktop
- run: appstreamcli validate --no-net core/data/io.github.gosh_distrobox_manager.metainfo.xml
- run: ./scripts/check-versions.sh                            # D14: Cargo / spec / metainfo
```

**The `--check` step, and why it belongs in this job.** This is the step T2 adds:

```bash
python3 flatpak/generate-cargo-sources.py --check
```

It regenerates `cargo-sources.json` in memory from `Cargo.lock` plus the committed
sidecar (`git-packages.json` + `git-manifests/`) and exits non-zero if the committed
`cargo-sources.json` differs. It needs **no network and no flatpak**, which is why it sits
in the fast job rather than behind the flatpak build, and it is what turns §1.3's silent
failure modes into a red job:

- A dependency change that was not followed by a regeneration → `cargo-sources.json is out
  of date`.
- A git dependency added, removed, or re-pinned without `--refresh-git` → `no
  git-packages.json entry for <source>`, or `git-packages.json is stale for <source>`
  with the exact missing/extra package list.
- A sidecar whose normalised manifests were not refreshed → `missing normalised manifest
  git-manifests/<name>-<version>/Cargo.toml`.

It does **not** catch a `Cargo.lock` that is out of sync with `Cargo.toml` — that is
`--locked` in job 1 — nor a lockfile that resolves a *different* upstream state (the two
unpinned git deps, §1.3): `--check` compares the sidecar against whatever the lockfile
says, so it is a staleness gate, not a "the upstream branch did not move" gate. Only job 3,
by building offline, turns the latter into a failure.

The step is placed **first** in the job because it is the fastest and the one most likely
to fail on a dependency-touching PR. Note that it must run from the repository root: the
generator's `--lockfile` and `--project` defaults are derived from the script's own
location (`flatpak/..`), so it works from any cwd, but the paths printed in its errors are
root-relative.

**Sequencing note (T2, unresolved).** `--check` is green today, but the artifact it guards
is not yet the right one: `Cargo.lock` at T2's commit still holds the pre-migration
`flutter_rust_bridge` graph, because `app/` (and therefore libcosmic, and therefore all
11 git sources) does not exist until T3. `cargo-sources.json` therefore describes a
dependency set that T14 deletes. **The moment `app/` lands, the lockfile gains the 11 git
sources and both `cargo-sources.json` and the `flatpak` job become meaningful; the
sidecar (`git-packages.json` + `git-manifests/`) is already complete and validated for
that state.** See §2.5.2.

#### 2.5.2 The vacuity gap, and the fixture that closes it (T2, open)

**O1 is right about the mechanism, and it must be fixed before T4.** At T2's commit the
lockfile has **zero** git sources, so:

- `generate-cargo-sources.py --check` never enters its git branch. `git-packages.json`
  and `git-manifests/` are read by *nothing* on that path — the sidecar loop iterates
  `collect_git_sources(packages)`, which is empty. A green `--check` therefore certifies
  the registry path and says nothing whatsoever about the git path.
- The `flatpak` job (§2.5.1 job 3) fails at source-download time on `../app`, before any
  Rust compiles, so it too exercises no git source.

**So the committed artifacts are not self-validating at T2.** What is true of them is
narrower than "the git path is tested": `git-packages.json` is byte-identical to a real
`--refresh-git` output over a real resolved libcosmic graph, the 51 `git-manifests/`
entries all exist and none still inherit from a workspace root, and the generator is
function-identical (16/16) to the sibling's committed generator. Those are *evidence*,
not a *gate*. A regression in the git path between T2 and T3 would land green.

**The sibling hit this exact problem and solved it; we did not port the solution.** Its
`tests/test_packaging.py` carries a `CASES` list of `[("live", …), ("fixture", …)]` and
runs every git assertion over both, with `tests/fixtures/git-deps/` holding a trimmed
real lockfile (covering rev-pinned, tag-pinned, and commit-only sources), its sidecar, and
its normalised manifests. Its own comment states the reason: *"The live lockfile has no
git dependencies until T6 lands, so every git assertion below would pass vacuously against
it alone — and a test that cannot fail reads as coverage while the git path regresses."*
Its `test_fixture_actually_exercises_the_git_paths` guards the guard, so the fixture cannot
quietly decay into vacuity itself.

**Assignment needed (T2 cannot do it).** T2's scope is `flatpak/**` and this document;
the sibling's harness lives in `tests/`, outside it. Recommended split:

1. **T4 owns the runner.** It owns `scripts/verify.sh` and D13's stage list, and D13
   already names `generate-cargo-sources.py --check` as a stage — so T4 is where a
   `python3 -m pytest tests/test_packaging.py` (or a dependency-free equivalent, since
   pyyaml/pytest are not host deps and CI would have to install them) becomes a real
   stage. Until then D13's stage list is **optimistic about this step**: it lists a
   `--check` that cannot fail on the git path it is meant to protect.
2. **The fixture belongs beside the generator** and can land in `flatpak/` (T2's scope)
   if the lead prefers to keep the test data with the tool: `flatpak/fixtures/git-deps/`
   with the same three artefacts. Either location works; what must not happen is the
   fixture being dropped in the port.

Selective port — the sibling's manifest/licence/trademark tests
(`test_manifest_installs_first_and_third_party_license_material`,
`test_product_trademark_notice_is_present`, `test_manifest_uses_a_pinned_rust_toolchain_archive`,
`test_flatpak_builds_only_the_cosmic_binary`, `test_cosmic_icon_theme_is_bundled`,
`test_build_script_runtime_matches_the_manifest`) describe *that* app's YAML manifest,
pinned toolchain tarball, and licence bundling. Ours is JSON, uses the rust-stable SDK
extension rather than a toolchain archive, and has no `build-flatpak.sh`. Do **not** port
them by rote. The generator tests (fixture cases, `parse_git_source`, sidecar-shape,
`--check` staleness) and the two finish-args-guard names are the portable set.

**Note the enforced-count trap.** The sibling's `test_fixture_lockfile_generates_cleanly`
hard-codes `kinds.count("git") == 3` and an inline count derived from a 7-package fixture.
Any port must recompute those from our own fixture rather than inherit them, or the test
will assert the sibling's graph against ours and fail for the wrong reason.

---

## 3. Non-COSMIC verification plan (GNOME — the current desktop)

The requirement: the app must work on GNOME, which is this machine's desktop, not just on
COSMIC. libcosmic is *designed* for COSMIC but is a plain winit/iced application and runs
elsewhere; the specific risks are theming, the file picker, and the icon theme.

| Concern | Mechanism | Verification |
|---|---|---|
| **Window creation / rendering** | winit + `fallback-x11`/Wayland | Launch under a GNOME session; window appears, no surface errors |
| **Theming** | libcosmic ships its own theme engine; it does not use GTK themes. It will render *COSMIC-styled*, not Adwaita. **This is expected, not a bug** — document it so it is not reported as one | Visual check |
| **File picker** | Use `ashpd` / XDG **Desktop Portal** (`org.freedesktop.portal.FileChooser`) rather than GTK dialogs (D16). This is libcosmic's `xdg-portal` feature, which pulls `ashpd` directly — there is no `rfd` in the build and therefore nothing to configure: the old "configure rfd to use its portal backend, not the GTK backend" step described a knob that does not exist, and is deleted (REVIEW UX-15). Enabling `rfd` at all would be a regression, since its GTK backend needs GTK in the sandbox and would contradict the portal-only decision | Open the export/import picker under GNOME; confirm the portal dialog appears (not a GTK one). Also confirm `xdg-desktop-portal` is running — under Flatpak the picker is portal-mediated by construction, natively it is a runtime dependency |
| **Icon theme** | On GNOME, `cosmic-icons`/`pop-icon-theme` are not installed. libcosmic resolves icons from the *system* theme at runtime (verified: `build.rs` skips bundled icons on Linux, `src/icon_theme.rs` reads the theme) | Confirm icons fall back gracefully (missing-icon placeholder is acceptable; a panic or blank window is not). Ship a fallback or declare the icon-theme dependency |
| **Portal availability** | Portals need a running `xdg-desktop-portal` + a GNOME backend | `flatpak run` under GNOME; check the picker and any `OpenURI` call |
| **Settings persistence** | cosmic-config writes to `~/.config/cosmic/…` regardless of DE. No COSMIC daemon required for plain file persistence | Round-trip test (§2.2 C); then confirm the file appears under GNOME |
| **`com.system76.CosmicSettingsDaemon`** | Absent on GNOME — but no longer reached. D23 drops libcosmic's `dbus-config`, so the proxy is never built and `watch_config` always uses the file watcher | The former "main GNOME-specific failure risk" is retired by construction: the failing code path is not compiled. Keep the manual check in §5.4 (run under GNOME, watch for `CosmicSettingsDaemon` errors in the log) as a **negative** assertion — the correct observable is **zero** such errors, which is what the sibling measured after dropping the feature (15/20s → 0) |

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

**Outcome (T14).** Every row below was resolved, and E14 is satisfied: all nine
sources agree at 1.0.2, verified by `./scripts/check-versions.sh` (which greps all of
them and is stage 5 of `verify.sh`, so this cannot silently regress). Note the
scoping that mattered — E14's requirement was to cover `RPM-BUILD.md` and
`build-rpm.sh` as well as the manifests, and the checker does.

What the rewrite did, item by item, against the table and the bullet list below:

- **Versions** — `spec` `Version:` and `%changelog` already read 1.0.2/1.0.2-1 and
  `RPM-BUILD.md` already referenced 1.0.2 before T14; nothing needed changing. The
  `rust/data/…metainfo.xml.in` row is moot: that template was deleted and the
  installed `core/data/…metainfo.xml` carries the `<releases>` entry.
- **The 18 MB tarball** — deleted, and `.gitignore` now carries a
  `gosh-distrobox-manager-*.tar.gz` guard so `git add -A` cannot put it (or its
  successor) back. That guard is the part of this row that is easy to skip and the
  part that actually prevents recurrence.
- **`Summary:`** — now "Graphical interface for managing Distrobox containers",
  matching the metainfo `<summary>` verbatim.
- **`BuildRequires: gtk3-devel` / `Requires: gtk3`** — removed. Build deps are now
  `cargo`, `rust`, `desktop-file-utils`, `libappstream-glib`; `Requires: distrobox`
  stays. No `-devel` packages are needed because the Flatpak path builds the C
  dependencies with `flatpak-builder`, and the RPM path links GTK 3 through
  `libcosmic` as a soname rather than by explicit `Requires:`.
- **`%install` / `%files`** — rewritten: `cargo build --workspace --release --locked`
  and individual install lines from `core/data/`. The `%{getenv:PWD}` hardcode and
  the `%{_libdir}/%{name}/` bundle directory are gone.
- **`%post`/`%postun`/`%posttrans`** — all removed. Beyond the GTK-specific
  `gtk-update-icon-cache` this section flagged, the remaining
  `update-desktop-database` and hicolor `touch` calls are obsolete on Fedora: glib2
  ships `%transfiletriggerin` file triggers that maintain both per transaction.
- **Maintained or retired** — maintained. The spec is a live second packaging path.

Unverified, and it should be said plainly: `rpmbuild` is not installed on the
development host and no `verify.sh` stage invokes it, so this rewrite is reviewed but
**not executed**. Building the RPM is the first real test of it.

The table as originally written follows.

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
- [ ] If the git set in `Cargo.lock` changed (a git dep added, removed, or re-pinned),
      run `python3 flatpak/generate-cargo-sources.py --refresh-git` **first** — it needs
      network and is the only step that rebuilds `git-packages.json`/`git-manifests/` —
      then the plain `generate` above, then `--check`. A `--check` failure naming
      `--refresh-git` in its message means exactly this step was skipped.
- [ ] **On Flathub submission only: switch the module's local `file`/`dir` sources to a
      single `git` source pinned to the release commit** (§1.2 notes). The repository
      manifest keeps local sources, because a local manifest cannot reference a commit
      that is not pushed; `verify.sh` stage 9 builds the local form. Reverting to local
      sources for local builds is expected and fine.
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
  the fast stages (1–8) as the per-push gate; reserve the full build for tags and manual
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
- **The `distroshelf-terminals.json` grant — retired, not fixed.** This was called the
  highest-probability silent failure in the design (§1.4.2): a filename/path mismatch that
  fails at runtime with nothing louder than an `error!` line. It is no longer a risk,
  because the grant and the file it named are both gone (ARCH-Q5/D11 — terminals move into
  cosmic-config). The sandboxed process now holds **no** single-file grant, so there is no
  path constant that can drift out of agreement with the manifest. The residual obligation
  is narrower: this was the *only* direct filesystem access in the sandbox, so if a future
  change adds one, it arrives with the grant *and* the silent-failure mode *and* the need
  for the path assertion §1.4.2 used to mandate.
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

5. **Do we adopt libcosmic's `dbus-config`, and if so what happens on GNOME? — [closed: D23.]**
   No: we use plain cosmic-config file persistence (`config_subscription` file watcher) and
   skip the daemon route entirely, so the former "main GNOME-specific failure risk" (§3) is
   retired by construction. Revisit only on a two-build measurement showing a need.

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
