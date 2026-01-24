#!/bin/bash
# Run this script on your HOST system to fix the icon cache

echo "Clearing icon caches..."

# Update system icon cache
sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor

# Update desktop database
sudo update-desktop-database /usr/share/applications

# Clear KDE icon cache if using KDE
if [ -d ~/.cache/icon-cache.kcache ]; then
    echo "Clearing KDE icon cache..."
    rm -f ~/.cache/icon-cache.kcache
fi

# Clear GNOME/GTK icon cache
if [ -d ~/.cache/thumbnails ]; then
    echo "Clearing thumbnail cache..."
    rm -rf ~/.cache/thumbnails/*
fi

# Kill and restart desktop components
echo ""
echo "Icon cache updated. Now restart your desktop:"
echo ""
echo "For GNOME:"
echo "  Alt+F2, type 'r', press Enter"
echo "  OR: killall -3 gnome-shell"
echo ""
echo "For KDE Plasma:"
echo "  killall plasmashell && kstart5 plasmashell"
echo ""
echo "Or simply log out and log back in."
