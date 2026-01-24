# Gosh Distrobox Manager Copilot Instructions

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
