# Gosh Distrobox Manager

A native Linux GUI for managing [Distrobox](https://distrobox.it/) containers,
built in Rust with [`libcosmic`](https://github.com/pop-os/libcosmic).

## What It Does

Gosh Distrobox Manager gives you a visual way to work with Distrobox containers.
You can create, clone, start, stop, and remove containers without touching the
terminal.

Beyond basic container management, you can browse and export applications from
your containers to the host, manage packages using whatever package manager the
container has (apt, dnf, pacman, zypper, and others), and keep an eye on
resource usage like CPU and memory.

The app also handles backups through snapshots — take one before making changes
and restore it later if something goes wrong — and can export entire containers
to tar archives for backup or sharing.

Long-running operations stream their output in real time, so you can see what is
happening during a container creation or a package installation rather than
watching a spinner.

## Requirements

- Linux with Wayland or X11
- [Distrobox](https://distrobox.it/) 1.5 or newer, with Podman or Docker behind it
- A Rust toolchain, to build from source

## Building

```bash
git clone https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager.git
cd Gosh-Distrobox-Manager
cargo build --workspace --release
```

The binary lands at `target/release/gosh_distrobox_manager`. Run it directly:

```bash
./target/release/gosh_distrobox_manager
```

The workspace is two crates: `core/` is the backend library (all container,
runtime and desktop-file logic) and `app/` is the `libcosmic` binary. The UI
never runs a command itself — it goes through the backend.

### Development loop

```bash
cargo run -p gosh_distrobox_manager
```

`./scripts/verify.sh` is the gate: formatting, a release build, clippy,
the test suite, version agreement across the packaging files, desktop/metainfo
validation, the vendoring `--check`, and the packaging tests. `--fast` skips
the two slowest stages. CI runs exactly this script, so a local pass and a green
build mean the same thing.

## Installation

### Flatpak

The Flatpak manifest is at `flatpak/io.github.gosh_distrobox_manager.json` and
builds fully offline from the vendored crate sources in
`flatpak/cargo-sources.json`:

```bash
flatpak-builder --user --install --force-clean \
    .flatpak-builder flatpak/io.github.gosh_distrobox_manager.json
```

### RPM

See [RPM-BUILD.md](RPM-BUILD.md). A prebuilt RPM is not published — the script
and spec build from a source tarball.

## Project Structure

```
core/                       backend library crate
  src/backends/             Distrobox, Podman, Docker, Flatpak integration
  src/backends/desktop_file.rs   desktop-entry parsing (Exec tokenizer, field codes)
  src/fakers/               CommandRunner abstraction + test doubles
  data/                     .desktop, metainfo and icons — the single source
app/                        libcosmic binary crate
  src/                      pages (dashboard, containers, images, packages, …)
scripts/verify.sh           the gate CI runs
flatpak/                    manifest + vendored crate sources
docs/migration/             the migration's decisions, plan and architecture
```

`core/data/` is the one place desktop integration files live. The RPM spec and
the Flatpak manifest both install from it, so there is no second copy to drift
out of sync.

## Architecture

Everything that shells out lives behind the `CommandRunner` abstraction
(`core/src/fakers/command.rs`). That is what lets the same backend run natively,
under Flatpak (via `flatpak-spawn --host`), and inside a container. **Never call
`std::process::Command` directly in backend logic** — `clippy.toml` bans it, and
the ban is deliberate rather than stylistic.

All backend calls run inside `Task`/`Subscription` futures so the UI thread
never blocks on a subprocess.

`docs/migration/architecture.md` has the full picture, and
`docs/migration/DECISIONS.md` records the choices behind it.

## Contributing

Contributions welcome. Feel free to open issues or submit pull requests.

`AGENTS.md` describes the layout and the conventions a change is expected to
follow; `docs/migration/PLAN.md` tracks the migration's remaining work.

## Acknowledgments

This project wouldn't exist without [Distrobox](https://github.com/89luca89/distrobox)
by Luca Di Maio, the tool that makes running any Linux distribution inside your
terminal possible. Gosh Distrobox Manager is a GUI wrapper around it.

The Rust backend architecture comes from [DistroShelf](https://github.com/ranfdev/distroshelf)
by ranfdev — a GTK4/Rust application this project was originally forked from.

The UI is built with [libcosmic](https://github.com/pop-os/libcosmic), the
COSMIC desktop's toolkit.

## License

Open source. See [COPYING](COPYING) for details.
