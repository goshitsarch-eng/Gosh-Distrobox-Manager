#!/bin/bash
# Script to build RPM for Gosh Distrobox Manager
#
# T14 rewrote this: the old script copied a pre-built Flutter bundle out of
# build/linux/x64/release/bundle/ and wrote a launcher shim to exec it. The app
# is a single Rust binary now, so the script builds it and installs it directly.
# `VERSION` stays a literal on purpose — scripts/check-versions.sh greps this
# exact line, and deriving it from core/Cargo.toml would mean editing that
# checker in the same commit as the riskiest packaging change.

set -e

VERSION="1.0.2"
RELEASE="1"
NAME="gosh-distrobox-manager"
ARCH="x86_64"

# The Fedora release suffix, taken from rpm itself rather than hardcoded. The
# previous literal was .fc43, which mislabelled the artifact on any other
# release (this host is fc44) — the exact failure mode REVIEW E14 predicts.
DIST="$(rpm --eval '%{?dist}' 2>/dev/null || true)"
[ -n "$DIST" ] || DIST=""
if [ "$DIST" = "%{?dist}" ]; then DIST=""; fi

echo "Building Gosh Distrobox Manager RPM..."

# Build the Rust binary first. `--locked` so the build fails loudly if Cargo.lock
# and the manifests disagree, rather than silently resolving something new.
echo "Building workspace (release, locked)..."
cargo build --workspace --release --locked

# Create build root structure
BUILDROOT="$HOME/rpmbuild/BUILDROOT/${NAME}-${VERSION}-${RELEASE}${DIST}.${ARCH}"
rm -rf "$BUILDROOT"
mkdir -p "$BUILDROOT"

# Install the binary
echo "Installing application..."
install -Dpm0755 target/release/gosh_distrobox_manager "$BUILDROOT/usr/bin/gosh_distrobox_manager"

# Install desktop file, metainfo and icons from core/data (the single source;
# the Flutter-era linux/data copy is deleted with the rest of that tree).
echo "Installing desktop integration..."
install -Dpm0644 core/data/io.github.gosh_distrobox_manager.desktop \
    "$BUILDROOT/usr/share/applications/io.github.gosh_distrobox_manager.desktop"
install -Dpm0644 core/data/io.github.gosh_distrobox_manager.metainfo.xml \
    "$BUILDROOT/usr/share/metainfo/io.github.gosh_distrobox_manager.metainfo.xml"
install -Dpm0644 core/data/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg \
    "$BUILDROOT/usr/share/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg"
install -Dpm0644 core/data/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg \
    "$BUILDROOT/usr/share/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg"

# Validate desktop file
echo "Validating desktop file..."
desktop-file-validate "$BUILDROOT/usr/share/applications/io.github.gosh_distrobox_manager.desktop" || true

# Build the RPM
echo "Building RPM package..."
QA_RPATHS=$((0x0002)) rpmbuild --quiet \
    --define "_topdir $HOME/rpmbuild" \
    --define "_rpmdir $HOME/rpmbuild/RPMS" \
    --define "version $VERSION" \
    --define "release $RELEASE" \
    --buildroot "$BUILDROOT" \
    -bb gosh-distrobox-manager.spec

# Move RPM to current directory
echo "Copying RPM to current directory..."
cp "$HOME/rpmbuild/RPMS/${ARCH}/${NAME}-${VERSION}-${RELEASE}."*.${ARCH}.rpm .

echo ""
echo "Success! RPM package created:"
ls -lh ${NAME}-${VERSION}-${RELEASE}.*.${ARCH}.rpm

echo ""
echo "To install:"
echo "  sudo dnf install ./${NAME}-${VERSION}-${RELEASE}.*.${ARCH}.rpm"
echo ""
echo "Or:"
echo "  sudo rpm -ivh ./${NAME}-${VERSION}-${RELEASE}.*.${ARCH}.rpm"
