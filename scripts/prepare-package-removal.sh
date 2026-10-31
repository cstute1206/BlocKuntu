#!/bin/sh
# Invoked by the authorized ALPM PreTransaction hook. Keep the restart barrier
# until post_remove has deleted package files and state, or restore protection
# if preparation fails. No credentials are written to the diagnostic output.
set -eu
umask 077

work='/run/blockuntu-package-removal'
systemd_runtime='/run/systemd/system'
guard='/usr/lib/blockuntu/check-package-removal-lease.sh'
mode="${1:-prepare}"

log() { printf '%s\n' "[blockuntu removal] $*" >&2; }
units() {
  printf '%s\n' blockuntu.socket blockuntu.service blockuntu-watchdog.service \
    blockuntu-hosts.path blockuntu-hosts.service
}
owned_work() {
  [ -d "$work" ] && [ ! -L "$work" ] &&
    [ "$(stat -c %u "$work")" = "$(id -u)" ] &&
    [ "$(stat -c %a "$work")" = 700 ] && [ -f "$work/preparing" ]
}
stopped() {
  for unit in $(units); do
    state="$(systemctl show "$unit" -p Id -p ActiveState -p MainPID -p ControlPID)" || return 1
    case "$state" in
      *'ActiveState=inactive'*|*'ActiveState=failed'*) ;;
      *) return 1 ;;
    esac
    if printf '%s\n' "$state" | grep -Eq '^(MainPID|ControlPID)=[1-9][0-9]*$'; then
      return 1
    fi
    case "$unit" in
      *.service)
        printf '%s\n' "$state" | grep -Fxq 'MainPID=0' || return 1
        printf '%s\n' "$state" | grep -Fxq 'ControlPID=0' || return 1
        ;;
    esac
  done
}
diagnose() {
  for unit in $(units); do
    systemctl show "$unit" -p Id -p LoadState -p ActiveState -p SubState \
      -p MainPID -p ControlPID -p RefuseManualStop -p Restart -p DropInPaths >&2 || true
  done
}
clear_barrier() {
  # Only the override files recorded by this preparation belong to us.
  if [ -f "$work/overrides" ]; then
    while IFS= read -r unit; do
      case "$unit" in
        blockuntu.socket|blockuntu.service|blockuntu-watchdog.service|blockuntu-hosts.path|blockuntu-hosts.service) ;;
        *) log 'invalid override ownership record'; return 1 ;;
      esac
      rm -f "$systemd_runtime/$unit.d/99-blockuntu-removal.conf" \
        "$systemd_runtime/$unit.d/99-blockuntu-removal.conf.tmp"
      rmdir "$systemd_runtime/$unit.d" 2>/dev/null || true
    done < "$work/overrides"
  fi
  rm -f "$work/preparing"
  systemctl daemon-reload
}
restore() {
  owned_work || return 1
  clear_barrier || return 1
  systemctl start blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path || return 1
  rm -rf "$work"
}
failed() {
  code=$?
  trap - EXIT HUP INT TERM
  log 'preparation failed; package removal must be aborted'
  diagnose
  if ! restore; then
    log 'restoring the installed services also failed; preparation diagnostics retained'
  fi
  exit "$code"
}

case "$mode" in
  --verify)
    owned_work && stopped
    exit $?
    ;;
  --finish)
    owned_work && stopped || { log 'services are not quiescent; refusing state cleanup'; exit 1; }
    clear_barrier
    rm -rf "$work"
    exit 0
    ;;
  --abort)
    restore
    exit $?
    ;;
  prepare) "$guard" ;;
  *) exit 1 ;;
esac

if [ -e "$work" ] || [ -L "$work" ]; then
  owned_work || { log 'unrecognized preparation directory'; exit 1; }
  if stopped; then
    "$guard"
    exit 0
  fi
else
  mkdir -m 700 "$work"
  : > "$work/preparing"
  : > "$work/overrides"
fi
trap failed EXIT
trap 'exit 1' HUP INT TERM

# This private copy remains available after Pacman deletes the packaged helper.
install -m 700 "$0" "$work/finish.sh"
for unit in $(units); do
  directory="$systemd_runtime/$unit.d"
  override="$directory/99-blockuntu-removal.conf"
  if ! grep -Fxq "$unit" "$work/overrides"; then
    [ ! -e "$override" ] && [ ! -L "$override" ] || {
      log "unowned removal override for $unit"; exit 1;
    }
    printf '%s\n' "$unit" >> "$work/overrides"
  fi
  mkdir -p "$directory"
  {
    printf '[Unit]\nRefuseManualStop=no\nConditionPathExists=!%s/preparing\n' "$work"
    case "$unit" in
      *.service) printf '\n[Service]\nRestart=no\nRuntimeDirectoryPreserve=yes\n' ;;
    esac
  } > "$override.tmp"
  mv "$override.tmp" "$override"
done
systemctl daemon-reload
# Stop all activation paths in one transaction. Conditions prevent queued
# watchdog starts from reactivating units while the package files disappear.
systemctl --no-block stop $(units)
deadline=$(( $(date -u +%s) + 30 ))
until stopped; do
  [ "$(date -u +%s)" -lt "$deadline" ] || { log 'service shutdown timed out'; exit 1; }
  sleep 0.1
done
# RuntimeDirectoryPreserve=yes must keep the still-unconsumed lease readable.
"$guard"
trap - EXIT HUP INT TERM
log 'services stopped; authorized package removal may proceed'
