%{!?blockuntu_version:%global blockuntu_version 0.1.0}
%{!?blockuntu_release:%global blockuntu_release 1}
# systemd-rpm-macros provides this on Fedora. Keep the Fedora location as a
# fallback so a Fedora-targeted candidate can also be built on Ubuntu.
%{!?_unitdir:%global _unitdir /usr/lib/systemd/system}

Name:           blockuntu
Version:        %{blockuntu_version}
Release:        %{blockuntu_release}%{?dist}
Summary:        Local Linux focus blocker
License:        MIT
URL:            https://github.com/AstuteCouch/BlocKuntu
Source0:        %{name}-%{version}.tar.gz

# This spec produces a self-hosted RPM. Fedora repository submission additionally
# requires vendored/offline Rust and npm dependencies.
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  libappindicator-gtk3-devel
BuildRequires:  libxdo-devel
BuildRequires:  librsvg2-devel
BuildRequires:  make
BuildRequires:  nodejs
BuildRequires:  npm
BuildRequires:  openssl-devel
BuildRequires:  pkgconfig(gtk+-3.0)
BuildRequires:  pkgconfig(webkit2gtk-4.1)

Requires:       e2fsprogs
Requires:       libayatana-appindicator-gtk3
Requires:       polkit
Requires:       shadow-utils
Requires:       systemd
Requires:       xdg-utils
Recommends:     wmctrl

%description
BlocKuntu is a local Linux focus blocker with a privileged daemon, Native
Messaging bridge, and desktop GUI for local website and application blocking.
Browser policies are deferred until each store-installed extension sends a
verified heartbeat, then the matching extension is force-installed and locked.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --manifest-path focusd/Cargo.toml --release --locked
cargo build --manifest-path native-host/Cargo.toml --release --locked
(
  cd focus-gui
  npm ci
  BLOCKUNTU_BUILD_NUMBER="%{version}-%{release}" npm run tauri -- build --no-bundle
)

%install
install -Dpm 0755 focusd/target/release/blockuntud \
  %{buildroot}%{_bindir}/blockuntud
install -Dpm 0755 native-host/target/release/blockuntu-native \
  %{buildroot}%{_bindir}/blockuntu-native
install -Dpm 0755 focus-gui/src-tauri/target/release/blockuntu-gui \
  %{buildroot}%{_bindir}/blockuntu-gui
install -Dpm 0755 scripts/setup-confined-firefox-native-host.sh \
  %{buildroot}%{_libexecdir}/blockuntu/setup-confined-firefox-native-host.sh

install -d %{buildroot}%{_bindir}
cat >%{buildroot}%{_bindir}/blockuntu-setup-confined-firefox <<'EOF'
#!/bin/sh
exec /usr/libexec/blockuntu/setup-confined-firefox-native-host.sh "$@"
EOF
chmod 0755 %{buildroot}%{_bindir}/blockuntu-setup-confined-firefox

install -Dpm 0644 packaging/deb/blockuntu.toml \
  %{buildroot}%{_sysconfdir}/blockuntu/config.toml

for size in 32 64 128; do
  install -Dpm 0644 focus-gui/src-tauri/icons/${size}x${size}.png \
    %{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/blockuntu.png
  install -Dpm 0644 focus-gui/src-tauri/icons/${size}x${size}.png \
    %{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/blockuntu-gui.png
done

install -d %{buildroot}%{_datadir}/applications
cat >%{buildroot}%{_datadir}/applications/local.blockuntu.gui.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=BlocKuntu
Comment=Linux focus blocker frontend
Exec=/usr/bin/blockuntu-gui
Icon=blockuntu-gui
StartupWMClass=blockuntu-gui
StartupNotify=true
Terminal=false
Categories=Utility;
X-GNOME-UsesNotifications=true
EOF
chmod 0644 %{buildroot}%{_datadir}/applications/local.blockuntu.gui.desktop

install -Dpm 0644 packaging/systemd/blockuntu.socket \
  %{buildroot}%{_unitdir}/blockuntu.socket
sed \
  's#ExecStart=/usr/local/bin/blockuntud serve#ExecStart=/usr/bin/blockuntud --defer-browser-policy-repair-until-heartbeat serve#' \
  packaging/systemd/blockuntu.service \
  >%{buildroot}%{_unitdir}/blockuntu.service
install -pm 0644 packaging/systemd/blockuntu-watchdog.service \
  %{buildroot}%{_unitdir}/blockuntu-watchdog.service
install -Dpm 0644 packaging/systemd/blockuntu-hosts.path \
  %{buildroot}%{_unitdir}/blockuntu-hosts.path
sed 's#ExecStart=/usr/local/bin/blockuntud repair-hosts#ExecStart=/usr/bin/blockuntud repair-hosts#' \
  packaging/systemd/blockuntu-hosts.service \
  >%{buildroot}%{_unitdir}/blockuntu-hosts.service
chmod 0644 %{buildroot}%{_unitdir}/blockuntu.service \
  %{buildroot}%{_unitdir}/blockuntu-hosts.service

install -d \
  %{buildroot}%{_libdir}/mozilla/native-messaging-hosts \
  %{buildroot}%{_libdir}/librewolf/native-messaging-hosts \
  %{buildroot}%{_libdir}/waterfox/native-messaging-hosts \
  %{buildroot}%{_sysconfdir}/opt/chrome/native-messaging-hosts \
  %{buildroot}%{_sysconfdir}/chromium/native-messaging-hosts \
  %{buildroot}%{_sysconfdir}/opt/edge/native-messaging-hosts \
  %{buildroot}%{_sysconfdir}/opt/vivaldi/native-messaging-hosts \
  %{buildroot}%{_sysconfdir}/vivaldi/native-messaging-hosts
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.json \
  >%{buildroot}%{_libdir}/mozilla/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_libdir}/mozilla/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.json \
  >%{buildroot}%{_libdir}/librewolf/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_libdir}/librewolf/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.json \
  >%{buildroot}%{_libdir}/waterfox/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_libdir}/waterfox/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.chrome.json \
  >%{buildroot}%{_sysconfdir}/opt/chrome/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_sysconfdir}/opt/chrome/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.chrome.json \
  >%{buildroot}%{_sysconfdir}/chromium/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_sysconfdir}/chromium/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.chrome.json \
  >%{buildroot}%{_sysconfdir}/opt/edge/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_sysconfdir}/opt/edge/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.chrome.json \
  >%{buildroot}%{_sysconfdir}/opt/vivaldi/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_sysconfdir}/opt/vivaldi/native-messaging-hosts/blockuntu_native.json
sed 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  packaging/native-messaging/blockuntu_native.chrome.json \
  >%{buildroot}%{_sysconfdir}/vivaldi/native-messaging-hosts/blockuntu_native.json
chmod 0644 %{buildroot}%{_sysconfdir}/vivaldi/native-messaging-hosts/blockuntu_native.json

%pre
if ! getent group blockuntu >/dev/null 2>&1; then
  groupadd --system blockuntu
fi

%post
create_installation_serial() {
  serial_file="/etc/blockuntu/installation-id"
  legacy_serial_file="/var/lib/blockuntu/installation-id"
  if [ -s "${serial_file}" ] && \
    grep -Eq '^BKI-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}$' "${serial_file}"; then
    rm -f "${legacy_serial_file}"
    return 0
  fi

  install -d -o root -g root -m 0755 /etc/blockuntu
  if [ -s "${legacy_serial_file}" ] && \
    grep -Eq '^BKI-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}$' "${legacy_serial_file}"; then
    install -o root -g root -m 0644 "${legacy_serial_file}" "${serial_file}"
    rm -f "${legacy_serial_file}"
    return 0
  fi

  random_hex="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf 'BKI-%s\n' "${chunks}" >"${temp_file}"
  install -o root -g root -m 0644 "${temp_file}" "${serial_file}"
  rm -f "${temp_file}"
  rm -f "${legacy_serial_file}"
}

create_recovery_credential() {
  credential_file="$1"
  prefix="$2"
  if [ -s "${credential_file}" ]; then
    return 0
  fi
  random_hex="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf '%s-%s\n' "${prefix}" "${chunks}" >"${temp_file}"
  install -d -o root -g root -m 0755 /etc/blockuntu
  install -o root -g blockuntu -m 0640 "${temp_file}" "${credential_file}"
  rm -f "${temp_file}"
}

create_installation_serial
if [ ! -e /var/lib/blockuntu/recovery-credentials-hidden ]; then
  create_recovery_credential /etc/blockuntu/uninstall-recovery.txt BLOCKUNTU-UNINSTALL-RECOVERY
  create_recovery_credential /etc/blockuntu/tier1-edit-key.txt BLOCKUNTU-TIER1-EDIT
fi

if [ -d /run/systemd/system ]; then
  rm -f /run/systemd/system/blockuntu.service.d/99-rpm-transaction.conf
  rm -f /run/systemd/system/blockuntu-watchdog.service.d/99-rpm-transaction.conf
  rmdir /run/systemd/system/blockuntu.service.d >/dev/null 2>&1 || true
  rmdir /run/systemd/system/blockuntu-watchdog.service.d >/dev/null 2>&1 || true
  systemctl daemon-reload || true
  systemctl enable --now blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

%preun
reject_package_uninstall() {
  cat >&2 <<'EOF'
BlocKuntu refuses direct package-manager removal.

Open BlocKuntu Settings and use its uninstall action instead. That action
prepares this package removal safely before it invokes dnf.
EOF
  exit 1
}

authorize_settings_uninstall() {
  lease_path="/run/blockuntu/package-removal-lease"
  lease_token="${BLOCKUNTU_PACKAGE_REMOVAL_LEASE:-}"

  [ -n "${lease_token}" ] || reject_package_uninstall
  [ -r "${lease_path}" ] || reject_package_uninstall

  IFS=' ' read -r expected_token expires_at <"${lease_path}" || reject_package_uninstall
  case "${expires_at}" in
    ''|*[!0-9]*) reject_package_uninstall ;;
  esac
  now="$(/usr/bin/date -u +%s)"
  [ "${now}" -le "${expires_at}" ] || reject_package_uninstall
  [ "${lease_token}" = "${expected_token}" ] || reject_package_uninstall

  rm -f "${lease_path}"
}

allow_systemd_stop() {
  [ -d /run/systemd/system ] || return 0
  mkdir -p /run/systemd/system/blockuntu.service.d \
    /run/systemd/system/blockuntu-watchdog.service.d
  cat >/run/systemd/system/blockuntu.service.d/99-rpm-transaction.conf <<'EOF'
[Unit]
RefuseManualStop=no

[Service]
Restart=no
EOF
  cat >/run/systemd/system/blockuntu-watchdog.service.d/99-rpm-transaction.conf <<'EOF'
[Unit]
RefuseManualStop=no

[Service]
Restart=no
EOF
  systemctl daemon-reload >/dev/null 2>&1 || true
}

remove_empty_dir() {
  rmdir "$1" >/dev/null 2>&1 || true
}

remove_hosts_block() {
  hosts_path="/etc/hosts"
  [ -f "${hosts_path}" ] || return 0
  grep -q "BEGIN BLOCKUNTU MANAGED" "${hosts_path}" 2>/dev/null || return 0
  if command -v chattr >/dev/null 2>&1; then
    chattr -i "${hosts_path}" >/dev/null 2>&1 || true
  fi
  temp_hosts="$(mktemp)"
  awk '
    /BEGIN BLOCKUNTU MANAGED/ { skip = 1; next }
    /END BLOCKUNTU MANAGED/ { skip = 0; next }
    skip != 1 { print }
  ' "${hosts_path}" >"${temp_hosts}"
  install -m 0644 "${temp_hosts}" "${hosts_path}"
  rm -f "${temp_hosts}"
}

remove_browser_policies() {
  for policy in \
    /etc/firefox/policies/policies.json \
    /etc/opt/chrome/policies/managed/blockuntu.json \
    /etc/chromium/policies/managed/blockuntu.json \
    /etc/brave/policies/managed/blockuntu.json \
    /etc/opt/opera/policies/managed/blockuntu.json \
    /etc/opt/edge/policies/managed/blockuntu.json \
    /etc/vivaldi/policies/managed/blockuntu.json \
    /etc/opt/vivaldi/policies/managed/blockuntu.json; do
    if [ -f "${policy}" ] && grep -qi "blockuntu" "${policy}" 2>/dev/null; then
      rm -f "${policy}"
    fi
  done
  remove_empty_dir /etc/firefox/policies
  remove_empty_dir /etc/firefox
  remove_empty_dir /etc/opt/chrome/policies/managed
  remove_empty_dir /etc/opt/chrome/policies
  remove_empty_dir /etc/chromium/policies/managed
  remove_empty_dir /etc/chromium/policies
  remove_empty_dir /etc/brave/policies/managed
  remove_empty_dir /etc/brave/policies
  remove_empty_dir /etc/opt/opera/policies/managed
  remove_empty_dir /etc/opt/opera/policies
  remove_empty_dir /etc/opt/edge/policies/managed
  remove_empty_dir /etc/opt/edge/policies
  remove_empty_dir /etc/vivaldi/policies/managed
  remove_empty_dir /etc/vivaldi/policies
  remove_empty_dir /etc/opt/vivaldi/policies/managed
  remove_empty_dir /etc/opt/vivaldi/policies
}

if [ "$1" -eq 0 ]; then
  authorize_settings_uninstall
  policy_recovery="/etc/blockuntu/policy-recovery.toml"
  if [ -e "${policy_recovery}" ] && command -v chattr >/dev/null 2>&1; then
    chattr -i "${policy_recovery}" >/dev/null 2>&1 || true
  fi
  remove_hosts_block
  remove_browser_policies
fi

allow_systemd_stop
if [ -d /run/systemd/system ]; then
  systemctl stop blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
  if [ "$1" -eq 0 ]; then
    systemctl disable blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
  fi
fi

%postun
if [ "$1" -eq 0 ]; then
  rm -f /run/systemd/system/blockuntu.service.d/99-rpm-transaction.conf
  rm -f /run/systemd/system/blockuntu-watchdog.service.d/99-rpm-transaction.conf
  rmdir /run/systemd/system/blockuntu.service.d >/dev/null 2>&1 || true
  rmdir /run/systemd/system/blockuntu-watchdog.service.d >/dev/null 2>&1 || true
  if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
    systemctl reset-failed blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
  fi
  rm -rf /etc/blockuntu /var/lib/blockuntu /run/blockuntu
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

%posttrans
# On an RPM upgrade/reinstall, the old package's %preun runs after the new
# package has been staged and can stop the protected units. This transaction
# hook runs last and restores the services only while BlocKuntu is still
# installed. It is deliberately not used to authorize a final removal.
if [ -d /run/systemd/system ] && /usr/bin/rpm -q blockuntu >/dev/null 2>&1; then
  rm -f /run/systemd/system/blockuntu.service.d/99-rpm-transaction.conf
  rm -f /run/systemd/system/blockuntu-watchdog.service.d/99-rpm-transaction.conf
  rmdir /run/systemd/system/blockuntu.service.d >/dev/null 2>&1 || true
  rmdir /run/systemd/system/blockuntu-watchdog.service.d >/dev/null 2>&1 || true
  systemctl daemon-reload >/dev/null 2>&1 || true
  systemctl enable --now blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path >/dev/null 2>&1 || true
fi

%files
%doc README.md Docs/INSTALLATION.md Docs/UNINSTALL.md
%license LICENSE
%{_bindir}/blockuntud
%{_bindir}/blockuntu-native
%{_bindir}/blockuntu-gui
%{_bindir}/blockuntu-setup-confined-firefox
%{_libexecdir}/blockuntu/setup-confined-firefox-native-host.sh
%config(noreplace) %{_sysconfdir}/blockuntu/config.toml
%config(noreplace) %{_sysconfdir}/opt/chrome/native-messaging-hosts/blockuntu_native.json
%config(noreplace) %{_sysconfdir}/chromium/native-messaging-hosts/blockuntu_native.json
%config(noreplace) %{_sysconfdir}/opt/edge/native-messaging-hosts/blockuntu_native.json
%config(noreplace) %{_sysconfdir}/opt/vivaldi/native-messaging-hosts/blockuntu_native.json
%config(noreplace) %{_sysconfdir}/vivaldi/native-messaging-hosts/blockuntu_native.json
%{_libdir}/mozilla/native-messaging-hosts/blockuntu_native.json
%{_libdir}/librewolf/native-messaging-hosts/blockuntu_native.json
%{_libdir}/waterfox/native-messaging-hosts/blockuntu_native.json
%{_unitdir}/blockuntu.socket
%{_unitdir}/blockuntu.service
%{_unitdir}/blockuntu-watchdog.service
%{_unitdir}/blockuntu-hosts.path
%{_unitdir}/blockuntu-hosts.service
%{_datadir}/applications/local.blockuntu.gui.desktop
%{_datadir}/icons/hicolor/32x32/apps/blockuntu.png
%{_datadir}/icons/hicolor/32x32/apps/blockuntu-gui.png
%{_datadir}/icons/hicolor/64x64/apps/blockuntu.png
%{_datadir}/icons/hicolor/64x64/apps/blockuntu-gui.png
%{_datadir}/icons/hicolor/128x128/apps/blockuntu.png
%{_datadir}/icons/hicolor/128x128/apps/blockuntu-gui.png

%changelog
* Thu Jul 30 2026 BlocKuntu <local@blockuntu.invalid> - %{version}-%{release}
- Initial self-hosted Fedora RPM
