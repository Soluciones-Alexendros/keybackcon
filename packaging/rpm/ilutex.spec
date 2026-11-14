Name:           ilutex
Version:        3.0.0
Release:        1%{?dist}
Summary:        iLutex — control cromático del teclado AERO X16 vía HID LampArray
License:        MIT
URL:            https://github.com/Soluciones-Alexendros/ilutex
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
for po in po/*.po; do
  lang="${po##*/}"; lang="${lang%.po}"
  mkdir -p "gui/locale/${lang}/LC_MESSAGES"
  msgfmt -o "gui/locale/${lang}/LC_MESSAGES/ilutex.mo" "${po}"
done

%install
install -D -m755 target/release/ilutex %{buildroot}%{_bindir}/ilutex
install -D -m755 packaging/ilutex-gui %{buildroot}%{_bindir}/ilutex-gui
install -d %{buildroot}%{_datadir}/ilutex/gui
install -m644 gui/*.py %{buildroot}%{_datadir}/ilutex/gui/
install -D -m644 packaging/desktop/ilutex.desktop %{buildroot}%{_datadir}/applications/ilutex.desktop
install -D -m644 assets/icons/ilutex.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/ilutex.svg
install -D -m644 packaging/udev/70-ilutex.rules %{buildroot}%{_prefix}/lib/udev/rules.d/70-ilutex.rules
install -D -m644 packaging/tmpfiles.d/ilutex.conf %{buildroot}%{_prefix}/lib/tmpfiles.d/ilutex.conf
install -D -m644 packaging/sysusers.d/ilutex.conf %{buildroot}%{_prefix}/lib/sysusers.d/ilutex.conf
install -D -m644 packaging/systemd/ilutex.service %{buildroot}%{_userunitdir}/ilutex.service
install -D -m644 packaging/systemd/ilutex-animation@.service %{buildroot}%{_userunitdir}/ilutex-animation@.service
for mo in gui/locale/*/LC_MESSAGES/ilutex.mo; do
  lang="${mo#gui/locale/}"; lang="${lang%%/*}"
  install -D -m644 "${mo}" %{buildroot}%{_datadir}/locale/${lang}/LC_MESSAGES/ilutex.mo
done
install -D -m644 gui/org.alexendros.ilutex.gschema.xml %{buildroot}%{_datadir}/glib-2.0/schemas/org.alexendros.ilutex.gschema.xml
install -D -m644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%{_bindir}/ilutex
%{_bindir}/ilutex-gui
%{_datadir}/ilutex/
%{_datadir}/applications/ilutex.desktop
%{_datadir}/icons/hicolor/scalable/apps/ilutex.svg
%{_prefix}/lib/udev/rules.d/70-ilutex.rules
%{_prefix}/lib/tmpfiles.d/ilutex.conf
%{_prefix}/lib/sysusers.d/ilutex.conf
%{_userunitdir}/ilutex.service
%{_userunitdir}/ilutex-animation@.service
%{_datadir}/locale/es/LC_MESSAGES/ilutex.mo
%{_datadir}/locale/en/LC_MESSAGES/ilutex.mo
%{_datadir}/glib-2.0/schemas/org.alexendros.ilutex.gschema.xml
%license LICENSE

%changelog
* Sun Nov 16 2025 Alexendros <operaciones@alexendros.dev> - 2.5.0-1
- Empaquetado RPM para ilutex (candidato; no acreditado hasta prueba en Fedora).