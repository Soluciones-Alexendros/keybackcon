Name:           keybackcon
Version:        2.3.1
Release:        1%{?dist}
Summary:        Control RGB del teclado AERO X16 vía HID LampArray
License:        MIT
URL:            https://github.com/Soluciones-Alexendros/keybackcon
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  cargo
BuildRequires:  gettext
Requires:       glibc
# GUI opcional: libadwaita >= 1.5
Suggests:       python3-gobject gtk4 libadwaita

%description
CLI y mesa de luz para el teclado AERO X16 (HID LampArray).
La GUI requiere GTK 4 y libadwaita >= 1.5.

%prep
%autosetup

%build
cargo build --release --locked
mkdir -p gui/locale/es/LC_MESSAGES
msgfmt -o gui/locale/es/LC_MESSAGES/keybackcon.mo po/es.po

%install
install -D -m755 target/release/keybackcon %{buildroot}%{_bindir}/keybackcon
install -D -m755 packaging/keybackcon-gui %{buildroot}%{_bindir}/keybackcon-gui
install -d %{buildroot}%{_datadir}/keybackcon/gui
install -m644 gui/*.py %{buildroot}%{_datadir}/keybackcon/gui/
install -D -m644 packaging/desktop/keybackcon.desktop %{buildroot}%{_datadir}/applications/keybackcon.desktop
install -D -m644 assets/icons/keybackcon.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/keybackcon.svg
install -D -m644 packaging/udev/70-keybackcon.rules %{buildroot}%{_prefix}/lib/udev/rules.d/70-keybackcon.rules
install -D -m644 packaging/tmpfiles.d/keybackcon.conf %{buildroot}%{_prefix}/lib/tmpfiles.d/keybackcon.conf
install -D -m644 packaging/sysusers.d/keybackcon.conf %{buildroot}%{_prefix}/lib/sysusers.d/keybackcon.conf
install -D -m644 packaging/systemd/keybackcon.service %{buildroot}%{_userunitdir}/keybackcon.service
install -D -m644 packaging/systemd/keybackcon-animation@.service %{buildroot}%{_userunitdir}/keybackcon-animation@.service
install -D -m644 gui/locale/es/LC_MESSAGES/keybackcon.mo %{buildroot}%{_datadir}/locale/es/LC_MESSAGES/keybackcon.mo
install -D -m644 gui/org.iniciativas.keybackcon.gschema.xml %{buildroot}%{_datadir}/glib-2.0/schemas/org.iniciativas.keybackcon.gschema.xml
install -D -m644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%{_bindir}/keybackcon
%{_bindir}/keybackcon-gui
%{_datadir}/keybackcon/
%{_datadir}/applications/keybackcon.desktop
%{_datadir}/icons/hicolor/scalable/apps/keybackcon.svg
%{_prefix}/lib/udev/rules.d/70-keybackcon.rules
%{_prefix}/lib/tmpfiles.d/keybackcon.conf
%{_prefix}/lib/sysusers.d/keybackcon.conf
%{_userunitdir}/keybackcon.service
%{_userunitdir}/keybackcon-animation@.service
%{_datadir}/locale/es/LC_MESSAGES/keybackcon.mo
%{_datadir}/glib-2.0/schemas/org.iniciativas.keybackcon.gschema.xml
%license LICENSE

%changelog
* Sat Oct 17 2026 Alexendros <operaciones@alexendros.dev> - 2.3.1-1
- Empaquetado RPM inicial (candidato; no acreditado hasta prueba en Fedora).
