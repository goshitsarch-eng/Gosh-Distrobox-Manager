#!/bin/bash
# Script to build RPM for Gosh Distrobox Manager

set -e

VERSION="1.0.2"
RELEASE="1"
NAME="gosh-distrobox-manager"
ARCH="x86_64"

echo "Building Gosh Distrobox Manager RPM..."

# Create build root structure
BUILDROOT="$HOME/rpmbuild/BUILDROOT/${NAME}-${VERSION}-${RELEASE}.fc43.${ARCH}"
rm -rf "$BUILDROOT"
mkdir -p "$BUILDROOT"

# Install the application bundle
echo "Installing application files..."
mkdir -p "$BUILDROOT/usr/lib64/$NAME"
cp -r build/linux/x64/release/bundle/* "$BUILDROOT/usr/lib64/$NAME/"

# Install the launcher script
echo "Creating launcher script..."
mkdir -p "$BUILDROOT/usr/bin"
cat > "$BUILDROOT/usr/bin/gosh_distrobox_manager" << 'EOF'
#!/bin/bash
exec /usr/lib64/gosh-distrobox-manager/gosh_distrobox_manager "$@"
EOF
chmod +x "$BUILDROOT/usr/bin/gosh_distrobox_manager"

# Install desktop file
echo "Installing desktop file..."
mkdir -p "$BUILDROOT/usr/share/applications"
cp linux/data/share/applications/io.github.gosh_distrobox_manager.desktop \
    "$BUILDROOT/usr/share/applications/"

# Install icons
echo "Installing icons..."
mkdir -p "$BUILDROOT/usr/share/icons/hicolor"
cp -r linux/data/share/icons/hicolor/* "$BUILDROOT/usr/share/icons/hicolor/"

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
