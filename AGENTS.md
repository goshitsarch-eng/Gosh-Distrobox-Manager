> **MIGRATION IN PROGRESS — READ THIS FIRST (branch `cosmic-migration`).**
>
> Everything below this box describes the **Flutter** application, which is being
> deleted. The project is migrating to a **libcosmic/Rust** UI on this branch:
> `flutter_rust_bridge` is removed, `lib/`, `test/`, `pubspec.yaml` and the
> platform trees go with it (task S8/T14), and the Rust backend is split into
> `core/` + `app/` (T1). **Treat the Flutter instructions here as historical** —
> they will actively mislead you if you follow them.
>
> Authoritative for the migration: `docs/migration/PLAN.md` (task list and status),
> `DECISIONS.md` (D1–D23, the settled choices), `architecture.md`, `ux.md`,
> `packaging.md`, and `REVIEW.md`. Where this file and those disagree, they win.
>
> Still true and still binding:
> - **Never call `std::process::Command` directly in backend logic.** All command
>   execution goes through the `CommandRunner`/`Command` abstraction so the app works
>   under Flatpak (`flatpak-spawn --host`) and inside a Distrobox container. This rule
>   survives the migration unchanged and is enforced by a `clippy.toml`
>   `disallowed-methods` entry (T1).
> - Container/runtime logic lives in the Rust backend, never in UI code.
>
> The rest of this file is rewritten in T14. Do not add to it before then.

# Gosh Distrobox Manager Copilot Instructions

> The section below is the pre-migration documentation, retained until T14 rewrites
> it. See the box above before acting on any of it.

## Project Overview
Gosh Distrobox Manager is a **Flutter** application for managing [Distrobox](https://distrobox.it/) containers. It uses **Rust** for the backend logic, connected via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

**Note**: This project was previously a GTK4/Rust application. Legacy GTK resources may still exist in `rust/data/` or `data/`, but the active UI is pure Flutter in `lib/`.

## Architecture

### Stack
- **Frontend**: Flutter (Dart) using `Provider` for state management.
- **Backend**: Rust (in `rust/` directory).
- **Bridge**: `flutter_rust_bridge` (FRB) v2.

### Directory Structure
- `lib/`: Flutter UI and application logic.
  - `lib/src/rust/`: **Generated** Dart code for the Rust bridge. Do not edit manually.
  - `lib/providers/`: State management (calls Rust APIs).
  - `lib/screens/`: UI Screens.
- `rust/`: The Rust backend crate.
  - `rust/src/api.rs`: The main API surface exposed to Flutter.
  - `rust/src/frb_generated.rs`: **Generated** Rust bridge code.
  - `rust/src/backends/`: Core logic for interacting with container runtimes (Podman, Docker, Flatpak).

## Development Workflow

### 1. Running the App
Standard Flutter workflow:
```bash
flutter run
```
This automatically compiles the Rust crate and links it.

### 2. Modifying Rust API
If you change `rust/src/api.rs` or other exposed Rust types:
1.  Make your changes in Rust.
2.  Run the codegen (requires `flutter_rust_bridge_codegen` installed):
    ```bash
    flutter_rust_bridge_codegen generate
    ```
    *Note: Check project docs if a specific script or `justfile` exists for this, but this is the standard command.*
3.  Use the updated API in Dart.

### 3. State Management
- **Dart**: `AppStateProvider` (`lib/providers/app_state.dart`) holds the source of truth for the UI.
- **Pattern**: The provider calls async Rust functions (e.g., `api.getContainers()`) and updates local state (`_containers`, `_isLoading`), notifying listeners.

## Key Concepts

### Command Execution (`CommandRunner`)
Gosh Distrobox Manager must run in both **Native** and **Flatpak** environments.
- **Abstraction**: `rust/src/fakers/command_runner.rs` defines a `CommandRunner` trait.
- **Native**: Executes commands directly.
- **Flatpak**: Detects Flatpak environment and wraps commands with `flatpak-spawn --host` automatically.
- **Logic**: See `rust/src/backends/flatpak.rs`.

**Rule**: NEVER use `std::process::Command` directly in backend logic. Always use the `CommandRunner` or provided helpers to ensure Flatpak compatibility.

### Distrobox Integration
- Logic is encapsulated in `rust/src/backends/distrobox/`.
- Desktop file parsing uses a shell script: `rust/src/backends/distrobox/POSIX_FIND_AND_CONCAT_DESKTOP_FILES.sh`.

## Gotchas
- **Generated Files**: `lib/src/rust/` and `rust/src/frb_generated.rs` are auto-generated. **Do not edit them.**
- **Legacy Files**: Ignore `meson.build` or `.ui` files in `rust/data/` or `data/` unless you are specifically working on packaging/migration tasks. The UI is built in Dart.
