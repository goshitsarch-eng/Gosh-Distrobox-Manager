# Gosh Distrobox Manager — agent instructions

A native Linux GUI for [Distrobox](https://distrobox.it/), written in Rust with
`libcosmic`. This file is the orientation a change is expected to start from.

## The two rules that are not negotiable

1. **Never call `std::process::Command` directly in backend logic.** All command
   execution goes through the `CommandRunner`/`Command` abstraction so the app
   works under Flatpak (`flatpak-spawn --host`) and inside a Distrobox
   container. `clippy.toml` enforces this with a `disallowed-methods` entry, and
   the only sanctioned indirection point is `core/src/fakers/command.rs`. If you
   need to run something, extend the runner — do not reach around it.
2. **Container and runtime logic lives in the backend crate, never in UI code.**
   `app/` renders and dispatches messages; `core/` decides. A UI module that
   parses `distrobox` output or builds a command line is a bug, even if it works.

Also binding: backend calls that can block run inside `Task`/`Subscription`
futures, never synchronously on the UI thread.

## Layout

```
core/                        backend library (crate `gosh-distrobox-core`)
  src/backends/              Distrobox, Podman, Docker, Flatpak integration
  src/backends/desktop_file.rs   desktop-entry parsing: Exec tokenizer + field codes
  src/fakers/                CommandRunner trait and its test doubles
  src/models/                DTOs, known distros
  src/service.rs             the façade the UI talks to
  data/                      .desktop, AppStream metainfo and icons — the single source
app/                         libcosmic binary (crate `gosh_distrobox_manager`)
  src/app.rs                 the Application impl: state, update(), view()
  src/<page>.rs              one module per page
  tests/                     integration tests driving the real binary
scripts/verify.sh            the gate; CI runs this, not a re-spelling of it
flatpak/                     manifest + vendored crate sources
docs/migration/              PLAN.md, DECISIONS.md, architecture.md, ux.md, REVIEW.md
```

There is no Flutter, Dart or `flutter_rust_bridge` code left. If you find a
reference to `lib/`, `pubspec.yaml`, `frb_generated.rs` or `flutter_rust_bridge`
in anything other than a historical `docs/migration/` note, it is stale.

## Working here

Run the gate before claiming a change is done:

```bash
./scripts/verify.sh          # full
./scripts/verify.sh --fast   # skips the two slowest stages
```

CI (`.github/workflows/`) runs the same script, so a local pass and a green
build mean the same thing. `cargo fmt --all`, `cargo clippy --workspace
--all-targets -- -D warnings` and `cargo test --workspace` are stages inside it.

Two conventions worth knowing before you edit:

- **`core/data/` is the single source for desktop integration.** The RPM spec
  and the Flatpak manifest both install from it. Do not reintroduce a second
  copy.
- **Version numbers are checked, not trusted.** `scripts/check-versions.sh`
  greps `core/Cargo.toml`, `app/Cargo.toml`, `Cargo.lock`,
  `gosh-distrobox-manager.spec`, `build-rpm.sh`, `RPM-BUILD.md` and the
  metainfo `<releases>` entry and fails on disagreement. `core/Cargo.toml` is
  the source of truth (D14). Run the script rather than eyeballing it.

## Documentation

`docs/migration/` is the migration's record and is authoritative where it
disagrees with anything else:

- `PLAN.md` — task list, status table, and the phase sequencing.
- `DECISIONS.md` — the settled choices (D1–D27). Read the relevant entry before
  re-litigating a design decision.
- `architecture.md` — module layout and the S1–S8 strip steps.
- `ux.md` — the Flutter-parity record. Rows 1–193 are **frozen** and cite the
  Flutter source they were measured against; those citations are provenance and
  must be preserved even though the cited files are gone (D19).
- `REVIEW.md` — the objections raised against the migration and their outcome.

## Gotchas

- **`target/` is large** (tens of GB on a working tree). Builds are not cheap;
  prefer `cargo check` while iterating.
- **`PARITY_CORPUS`** gates the differential test against GLib's `Exec`
  tokenizer. Without it set, that test reports skipped rather than passed — a
  skip is not evidence, so set it when touching `desktop_file.rs`.
- **The `.desktop` `Exec` key does not use the same unescaping as every other
  key.** `Name`/`Icon` run through `unescape_value`; `Exec` does not, because it
  has its own quoting rules layered on top. See DECISIONS.md D27 before
  touching the tokenizer.
