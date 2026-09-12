# RPM Package Build Instructions

## Building the RPM

To build the RPM package for Gosh Distrobox Manager:

```bash
./build-rpm.sh
```

The script builds the Rust workspace itself (`cargo build --workspace --release
--locked`) and then packages the result. There is no separate pre-build step —
the old instructions told you to run `flutter build linux --release` first
because the spec used to install a pre-built Flutter bundle; T14 removed both
the bundle and the step.

The Fedora release suffix comes from `rpm --eval '%{?dist}'` rather than a
hardcoded string, so the artifact is named for the release you build on.

This will create something like: `gosh-distrobox-manager-1.0.2-1.fc44.x86_64.rpm`
(the suffix varies by host; on Fedora 43 it is `.fc43`).

### Requirements for building

`cargo`, `rust`, `desktop-file-utils` (for `%check`), `libappstream-glib`
(provides `appstreamcli`), and `rpmbuild`. `build-rpm.sh` writes into
`~/rpmbuild` and copies the finished RPM into the repository root, which
`.gitignore` covers.

## Installing the RPM

### Using DNF (recommended)

```bash
sudo dnf install ./gosh-distrobox-manager-1.0.2-1.fc44.x86_64.rpm
```

### Using RPM directly

```bash
sudo rpm -ivh ./gosh-distrobox-manager-1.0.2-1.fc44.x86_64.rpm
```

## What Gets Installed

- **Binary**: `/usr/bin/gosh_distrobox_manager`
- **Desktop file**: `/usr/share/applications/io.github.gosh_distrobox_manager.desktop`
- **AppStream metadata**: `/usr/share/metainfo/io.github.gosh_distrobox_manager.metainfo.xml`
- **Icons**: `/usr/share/icons/hicolor/{scalable,symbolic}/apps/io.github.gosh_distrobox_manager*.svg`

There is no `/usr/lib64/gosh-distrobox-manager/` directory any more. The package
used to ship a Flutter bundle there (a launcher executable plus `lib/libapp.so`,
`lib/libflutter_linux_gtk.so` and the ICU data files); the application is a
single statically-linked Rust binary now, so that directory is gone.

## Post-Installation

After installation:

- The app appears in your application launcher (GNOME Activities, KDE Kickoff, etc.)
- Icon and desktop-file caches are refreshed by the package scriptlets
- You can run it from a terminal with: `gosh_distrobox_manager`

## Uninstalling

```bash
sudo dnf remove gosh-distrobox-manager
```

or

```bash
sudo rpm -e gosh-distrobox-manager
```

## Package Details

- **Package name**: gosh-distrobox-manager
- **Version**: 1.0.2
- **Architecture**: x86_64
- **Size**: ~39 MB installed, dominated by the binary
- **License**: GPL-3.0-or-later
- **Requires**: distrobox

`gtk3` is no longer a runtime requirement. The binary links GTK 3 through
`libcosmic` at build time, but the shared libraries it needs are pulled in as
sonames rather than by an explicit `Requires:` — the previous entry existed
because the Flutter engine linked GTK directly, and the old spec also needed it
to satisfy the bundled `libflutter_linux_gtk.so`.

`distrobox` remains the only explicit requirement: the application drives the
`distrobox` CLI, so it is genuinely useless without it. The container runtime
behind Distrobox (Podman or Docker) is deliberately *not* listed — Distrobox
itself accepts either, and hard-requiring one would be wrong for users running
the other.

## Files Included

The RPM package includes:

- The Gosh Distrobox Manager binary
- Desktop entry file
- AppStream metainfo
- Icons (scalable and symbolic SVG)

It does **not** include a Flutter runtime, Dart assets, fonts, or a URL-launcher
plugin — all of those left with the Flutter tree in T14.

## Verifying the package

`%check` runs two validators against the installed files, and the same two run
in `./scripts/verify.sh` stage 6 against the repository copies:

```bash
desktop-file-validate /usr/share/applications/io.github.gosh_distrobox_manager.desktop
appstreamcli validate --no-net /usr/share/metainfo/io.github.gosh_distrobox_manager.metainfo.xml
```

**Not locally verified.** `rpmbuild` is not installed on the development host,
and no stage of `./scripts/verify.sh` invokes it, so the spec and `build-rpm.sh`
are reviewed artefacts rather than tested ones. Building the RPM is the first
thing that actually exercises them.
