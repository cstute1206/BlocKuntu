#!/bin/sh
# Remove only BlocKuntu's per-user Native Messaging artifacts on final uninstall.
# Drop privileges before traversing user-controlled homes, including Snap links.
set -eu

remove_user_files() {
  user_home="$1"
  case "${user_home}" in
    /) echo 'Refusing the filesystem root as a user home' >&2; return 1 ;;
    /*) ;;
    *) echo 'An absolute user home is required' >&2; return 1 ;;
  esac
  [ -d "${user_home}" ] || return 0

  for manifest in \
    .mozilla/native-messaging-hosts/blockuntu_native.json \
    .librewolf/native-messaging-hosts/blockuntu_native.json \
    .waterfox/native-messaging-hosts/blockuntu_native.json \
    .config/google-chrome/NativeMessagingHosts/blockuntu_native.json \
    .config/chromium/NativeMessagingHosts/blockuntu_native.json \
    .config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json \
    .config/opera/NativeMessagingHosts/blockuntu_native.json \
    .config/microsoft-edge/NativeMessagingHosts/blockuntu_native.json \
    .config/vivaldi/NativeMessagingHosts/blockuntu_native.json \
    .var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json \
    snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json \
    snap/chromium/common/chromium/NativeMessagingHosts/blockuntu_native.json; do
    rm -f -- "${user_home}/${manifest}"
  done

  rm -f -- "${user_home}/.var/app/org.mozilla.firefox/data/blockuntu/blockuntu-native"
  for snap_name in firefox chromium brave opera vivaldi; do
    rm -f -- "${user_home}/snap/${snap_name}/common/.local/share/blockuntu/blockuntu-native" \
      "${user_home}/snap/${snap_name}/common/.local/share/blockuntu/blockuntu-native-launcher"
  done

  # current is normally a symlink; also clean artifacts in retained revisions.
  for profile in "${user_home}"/snap/brave/*; do
    [ -d "${profile}" ] || continue
    rm -f -- "${profile}/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json"
  done
  for profile in "${user_home}"/snap/opera/*; do
    [ -d "${profile}" ] || continue
    rm -f -- "${profile}/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json"
  done
  for profile in "${user_home}"/snap/vivaldi/*; do
    [ -d "${profile}" ] || continue
    rm -f -- "${profile}/.config/vivaldi/NativeMessagingHosts/blockuntu_native.json"
  done
}

if [ "${1:-}" = '--user' ] && [ "$#" -eq 2 ]; then
  [ "$(id -u)" -ne 0 ] || { echo 'User-file cleanup must run without root privileges' >&2; exit 1; }
  remove_user_files "$2"
  exit 0
fi

[ "$#" -eq 0 ] && [ "$(id -u)" -eq 0 ] || {
  echo 'Usage: remove-user-native-hosts.sh (root), or --user HOME (unprivileged)' >&2
  exit 1
}

uid_min="$(awk '$1 == "UID_MIN" { print $2; exit }' /etc/login.defs 2>/dev/null || true)"
case "${uid_min}" in ''|*[!0-9]*) uid_min=1000 ;; esac
passwd_records="$(getent passwd)"
printf '%s\n' "${passwd_records}" | while IFS=: read -r user_name password user_uid user_gid gecos user_home user_shell; do
  [ "${user_uid}" -ne 0 ] && [ "${user_uid}" -ne 65534 ] && [ "${user_uid}" -ge "${uid_min}" ] || continue
  [ -d "${user_home}" ] || continue
  runuser -u "${user_name}" -- /bin/sh "$0" --user "${user_home}"
done
