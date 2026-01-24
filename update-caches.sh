#!/bin/bash
# Script to manually update icon caches after installation

echo "Updating icon caches..."

# Update system icon cache
if [ -d /usr/share/icons/hicolor ]; then
    echo "Updating /usr/share/icons/hicolor cache..."
    sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi

# Update desktop database
if [ -d /usr/share/applications ]; then
    echo "Updating desktop database..."
    sudo update-desktop-database /usr/share/applications 2>/dev/null || true
fi

# Update user icon cache if exists
if [ -d ~/.local/share/icons/hicolor ]; then
    echo "Updating ~/.local/share/icons/hicolor cache..."
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor 2>/dev/null || true
fi

echo ""
echo "Done! You may need to:"
echo "1. Log out and log back in"
echo "2. Or restart your desktop environment"
echo "3. Or run: killall plasmashell && plasmashell (for KDE)"
echo "4. Or run: killall gnome-shell (for GNOME - will auto-restart)"
