Name:           gosh-distrobox-manager
Version:        1.0.2
Release:        1%{?dist}
Summary:        Graphical interface for managing Distrobox containers

License:        GPL-3.0-or-later
URL:            https://github.com/goshitsarch-eng/Gosh-Distrobox-Manager
Source0:        %{name}-%{version}.tar.gz

# The app is Rust: core/ is the backend crate, app/ the libcosmic binary.
# No GTK and no Flutter toolchain is involved any more (T14 removed the old
# Flutter bundle, which this spec used to install pre-built).
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib
Requires:       distrobox

%description
Gosh Distrobox Manager is a graphical interface for managing Distrobox
containers. You can create, clone, start, stop, and remove containers without
touching the terminal, browse and export applications from a container to the
host, manage packages with whatever package manager the container has, and keep
backups through snapshots.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --workspace --release --locked

%install
install -Dpm0755 target/release/gosh_distrobox_manager \
    %{buildroot}%{_bindir}/gosh_distrobox_manager

install -Dpm0644 core/data/io.github.gosh_distrobox_manager.desktop \
    %{buildroot}%{_datadir}/applications/io.github.gosh_distrobox_manager.desktop

install -Dpm0644 core/data/io.github.gosh_distrobox_manager.metainfo.xml \
    %{buildroot}%{_metainfodir}/io.github.gosh_distrobox_manager.metainfo.xml

install -Dpm0644 core/data/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg

install -Dpm0644 core/data/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg \
    %{buildroot}%{_datadir}/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg

%check
desktop-file-validate \
    %{buildroot}%{_datadir}/applications/io.github.gosh_distrobox_manager.desktop
appstreamcli validate --no-net \
    %{buildroot}%{_metainfodir}/io.github.gosh_distrobox_manager.metainfo.xml

%files
%{_bindir}/gosh_distrobox_manager
%{_datadir}/applications/io.github.gosh_distrobox_manager.desktop
%{_metainfodir}/io.github.gosh_distrobox_manager.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.gosh_distrobox_manager.svg
%{_datadir}/icons/hicolor/symbolic/apps/io.github.gosh_distrobox_manager-symbolic.svg

%post
/bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null || :
/usr/bin/update-desktop-database &> /dev/null || :

%postun
if [ $1 -eq 0 ] ; then
    /bin/touch --no-create %{_datadir}/icons/hicolor &>/dev/null
fi
/usr/bin/update-desktop-database &> /dev/null || :

%changelog
* Fri Sep 11 2026 Gosh Distrobox Manager Team <dev@example.com> - 1.0.2-1
- Version sync with core/Cargo.toml (D14); no packaging change
* Fri Jan 24 2025 Gosh Distrobox Manager Team <dev@example.com> - 1.0.0-1
- Initial RPM release
- Cross-platform icon system
- Desktop integration for Linux
- URL launcher for external links
