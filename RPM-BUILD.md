# RPM Package Build Instructions

## Building the RPM

To build the RPM package for Gosh Distrobox Manager:

```bash
# Ensure you have a release build
flutter build linux --release

# Run the build script
./build-rpm.sh
```

This will create: `gosh-distrobox-manager-1.0.0-1.fc43.x86_64.rpm`

## Installing the RPM

### Using DNF (recommended)
```bash
sudo dnf install ./gosh-distrobox-manager-1.0.0-1.fc43.x86_64.rpm
```

### Using RPM directly
```bash
sudo rpm -ivh ./gosh-distrobox-manager-1.0.0-1.fc43.x86_64.rpm
```

## What Gets Installed

- **Binary**: `/usr/bin/gosh_distrobox_manager`
- **Application files**: `/usr/lib64/gosh-distrobox-manager/`
- **Desktop file**: `/usr/share/applications/io.github.gosh_distrobox_manager.desktop`
- **Icons**: `/usr/share/icons/hicolor/*/apps/io.github.gosh_distrobox_manager.*`

## Post-Installation

After installation:
- The app will appear in your application launcher (GNOME Activities, KDE Kickoff, etc.)
- Icon caches and desktop database are automatically updated
- You can run it from terminal with: `gosh_distrobox_manager`

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
- **Version**: 1.0.0
- **Architecture**: x86_64
- **Size**: ~11 MB (installed: ~37 MB)
- **License**: GPL-3.0-or-later
- **Requires**: gtk3, distrobox

## Files Included

The RPM package includes:
- Gosh Distrobox Manager application binary
- Flutter runtime libraries
- URL launcher plugin
- Application assets and fonts
- Desktop entry file
- Icons in multiple resolutions (16px to 512px + SVG)
