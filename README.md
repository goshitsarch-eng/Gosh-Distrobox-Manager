# Gosh Distrobox Manager

A modern, cross-platform GUI application for managing [Distrobox](https://distrobox.it/) containers. Built with **Flutter** for the frontend and **Rust** for the backend, connected via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

## Features

### Container Management
- List and view all Distrobox containers with status indicators
- Create new containers from a wide variety of Linux distributions
- Clone existing containers
- Start, stop, and remove containers
- Upgrade container packages
- View detailed container information and statistics

### Integrated Terminal
- Built-in terminal emulator for direct container access
- Enter any container with a single click

### Application Management
- Browse applications installed inside containers
- Export/unexport applications to the host system
- Export/unexport binaries from containers

### Package Management
- Detect container's package manager (apt, dnf, pacman, zypper, etc.)
- List installed packages
- Search for packages in container repositories
- Install and remove packages with real-time output streaming

### Backup & Restore
- Create snapshots of containers
- List and manage snapshots
- Restore containers from snapshots
- Export containers to tar archives
- Import containers from archives

### Resource Monitoring
- View container resource usage (CPU, memory)
- Monitor disk usage

### Additional Features
- Dashboard with overview of all containers
- Activity logs for tracking operations
- Task management with real-time output streaming
- Flatpak support (runs natively or inside Flatpak sandbox)
- Dark/light theme support

## Screenshots

*Coming soon*

## Architecture

- **Frontend**: Flutter (Dart) with Provider for state management
- **Backend**: Rust for performance-critical operations and system interaction
- **Bridge**: `flutter_rust_bridge` v2 for seamless Dart-Rust interop

## Prerequisites

- Flutter SDK (3.x or later)
- Rust toolchain (cargo)
- `flutter_rust_bridge_codegen` v2
- Distrobox installed on the host system
- Podman or Docker as the container runtime

## Building

### Quick Start

```bash
# Clone the repository
git clone https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager.git
cd Gosh-Distrobox-Manager

# Get dependencies
flutter pub get

# Run the app
flutter run -d linux
```

### Manual Build Steps

1. **Generate Rust-Dart bindings** (only needed after modifying Rust API):
   ```bash
   flutter_rust_bridge_codegen generate
   ```

2. **Build Rust library** (optional, Flutter build does this automatically):
   ```bash
   cd rust
   cargo build --release
   ```

3. **Build for Linux**:
   ```bash
   flutter build linux
   ```

The built application will be at `build/linux/x64/release/bundle/gosh_distrobox_manager`

## Project Structure

```
.
├── lib/                    # Flutter Dart code
│   ├── main.dart           # Application entry point
│   ├── providers/          # State management (Provider/ChangeNotifier)
│   ├── screens/            # UI screens
│   ├── widgets/            # Reusable widgets
│   └── src/rust/           # Generated Rust bindings (do not edit)
├── rust/                   # Rust backend
│   ├── src/
│   │   ├── api.rs          # API exposed to Dart
│   │   ├── backends/       # Distrobox/Podman/Docker logic
│   │   ├── models/         # Data models
│   │   └── frb_generated.rs # Generated bridge code (do not edit)
│   └── Cargo.toml
├── linux/                  # Linux platform runner
├── android/                # Android platform support
├── ios/                    # iOS platform support
├── macos/                  # macOS platform support
├── windows/                # Windows platform support
└── web/                    # Web platform support
```

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## Acknowledgments

This project builds upon the work of others:

- **[Distrobox](https://github.com/89luca89/distrobox)** by [Luca Di Maio](https://github.com/89luca89) - The amazing container tool that makes running any Linux distribution inside your terminal possible. Gosh Distrobox Manager is simply a GUI wrapper around this fantastic CLI tool.

- **[DistroShelf](https://github.com/ranfdev/distroshelf)** by [ranfdev](https://github.com/ranfdev) - The original GTK4/Rust application that inspired this project. The Rust backend logic and architecture were derived from DistroShelf.

- **[flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge)** - For making Dart-Rust interop seamless.

## License

This project is open source. See the LICENSE file for details.

## Links

- [Distrobox Documentation](https://distrobox.it/)
- [Flutter Documentation](https://docs.flutter.dev/)
- [Rust Documentation](https://www.rust-lang.org/learn)
