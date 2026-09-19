#!/bin/sh
# Executed by virt-customize inside a powered-off, validated disposable clone.
set -eu
test_user=akhi
id "$test_user" >/dev/null
ssh-keygen -A
# Do not let the copied D-Bus ID restore the original machine ID at boot.
rm -f /var/lib/dbus/machine-id
ln -s /etc/machine-id /var/lib/dbus/machine-id
printf '%s\n' "$test_user ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/90-blockuntu-phase0
chmod 0440 /etc/sudoers.d/90-blockuntu-phase0
visudo -cf /etc/sudoers.d/90-blockuntu-phase0

if test -d /etc/gdm3; then
  config=/etc/gdm3/custom.conf
elif test -d /etc/gdm; then
  config=/etc/gdm/custom.conf
else
  config=
fi
if test -n "$config"; then
  # These clean test clones use GDM's normal desktop session with auto-login.
  sed -i '/^[[:space:]]*AutomaticLoginEnable[[:space:]]*=/d; /^[[:space:]]*AutomaticLogin[[:space:]]*=/d' "$config"
  sed -i "/^\[daemon\]/a AutomaticLoginEnable=True\nAutomaticLogin=$test_user" "$config"
elif test -f /usr/lib/systemd/system/plasmalogin.service; then
  # Current CachyOS uses Plasma Login Manager, not SDDM.
  test -f /usr/share/wayland-sessions/plasma.desktop
  sed -i '/^[[:space:]]*User[[:space:]]*=/d' /etc/plasmalogin.conf
  sed -i "/^\[Autologin\]/a User=$test_user" /etc/plasmalogin.conf
elif test -d /etc/sddm.conf.d || test -f /usr/lib/systemd/system/sddm.service; then
  mkdir -p /etc/sddm.conf.d
  if test -f /usr/share/wayland-sessions/plasma.desktop; then
    session=plasma.desktop
  elif test -f /usr/share/wayland-sessions/plasmawayland.desktop; then
    session=plasmawayland.desktop
  else
    echo 'No supported Plasma Wayland session found' >&2
    exit 1
  fi
  printf '[Autologin]\nUser=%s\nSession=%s\nRelogin=true\n' "$test_user" "$session" > /etc/sddm.conf.d/90-blockuntu-phase0.conf
else
  echo 'No supported display manager found' >&2
  exit 1
fi
systemctl set-default graphical.target
