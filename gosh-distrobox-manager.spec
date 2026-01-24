Name:           gosh-distrobox-manager
Version:        1.0.0
Release:        1%{?dist}
Summary:        A Flutter application for managing Distrobox containers

License:        GPL-3.0-or-later
URL:            https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  gtk3-devel
BuildRequires:  desktop-file-utils
Requires:       gtk3
Requires:       distrobox

%description
Gosh Distrobox Manager is a modern, cross-platform GUI for managing Distrobox 
containers. You can create, clone, start, stop, and remove containers without 
touching the terminal. The app includes a built-in terminal emulator so you 
can drop into any container with a single click.

%prep
# No prep needed - using pre-built Flutter bundle

%build
# Build is done separately with Flutter

%install
rm -rf %{buildroot}

# Get the source directory (where the spec file is located)
SRCDIR="%{getenv:PWD}"

# Install the application bundle
mkdir -p %{buildroot}%{_libdir}/%{name}
cp -r ${SRCDIR}/build/linux/x64/release/bundle/* %{buildroot}%{_libdir}/%{name}/

# Install the launcher script
mkdir -p %{buildroot}%{_bindir}
cat > %{buildroot}%{_bindir}/gosh_distrobox_manager << 'EOF'
#!/bin/bash
exec %{_libdir}/%{name}/gosh_distrobox_manager "$@"
EOF
chmod +x %{buildroot}%{_bindir}/gosh_distrobox_manager

# Install desktop file
mkdir -p %{buildroot}%{_datadir}/applications
install -m 644 ${SRCDIR}/linux/data/share/applications/io.github.gosh_distrobox_manager.desktop \
    %{buildroot}%{_datadir}/applications/

# Install icons
mkdir -p %{buildroot}%{_datadir}/icons/hicolor
cp -r ${SRCDIR}/linux/data/share/icons/hicolor/* %{buildroot}%{_datadir}/icons/hicolor/

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/io.github.gosh_distrobox_manager.desktop

%files
%{_libdir}/%{name}/
%{_bindir}/gosh_distrobox_manager
%{_datadir}/applications/io.github.gosh_distrobox_manager.desktop
%{_datadir}/icons/hicolor/*/apps/io.github.gosh_distrobox_manager.*

%post
/bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
/usr/bin/update-desktop-database &> /dev/null || :

%postun
if [ $1 -eq 0 ] ; then
    /bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null
    /usr/bin/gtk-update-icon-cache %{_datadir}/icons/hicolor &>/dev/null || :
fi
/usr/bin/update-desktop-database &> /dev/null || :

%posttrans
/usr/bin/gtk-update-icon-cache %{_datadir}/icons/hicolor &>/dev/null || :

%changelog
* Fri Jan 24 2025 Gosh Distrobox Manager Team <dev@example.com> - 1.0.0-1
- Initial RPM release
- Cross-platform icon system
- Desktop integration for Linux
- URL launcher for external links
