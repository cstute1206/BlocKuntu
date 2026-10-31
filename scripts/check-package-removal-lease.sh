#!/bin/sh
# Transaction guards must validate without consuming: the removal scriptlet
# consumes the same daemon-issued lease after the package manager commits.
removal_message='BlocKuntu refuses direct package-manager removal.'
prepare_pacman=0
reject_package_uninstall() {
  printf '%s\n' "${removal_message}" \
    'Open BlocKuntu Settings and use its uninstall action instead.' >&2
  exit 1
}

case "$#:$1" in
  0:) removal_message='BlocKuntu refuses direct Pacman removal.' ;;
  1:--pacman-prepare)
    removal_message='BlocKuntu refuses direct Pacman removal.'
    prepare_pacman=1
    ;;
  2:--dnf-action)
    case "$2" in
      E) ;; # Final erase requires GUI authorization.
      O) exit 0 ;; # Upgrade/reinstall replacement; not final removal.
      *) reject_package_uninstall ;;
    esac
    ;;
  *) reject_package_uninstall ;;
esac

lease_path='/run/blockuntu/package-removal-lease'
lease_token="${BLOCKUNTU_PACKAGE_REMOVAL_LEASE:-}"
[ "${#lease_token}" -eq 64 ] || reject_package_uninstall
case "${lease_token}" in *[!0-9a-f]*) reject_package_uninstall ;; esac
[ -f "${lease_path}" ] && [ ! -L "${lease_path}" ] && [ -r "${lease_path}" ] || reject_package_uninstall
IFS=' ' read -r expected_token expires_at extra <"${lease_path}" || reject_package_uninstall
[ -z "${extra}" ] || reject_package_uninstall
case "${expires_at}" in ''|*[!0-9]*) reject_package_uninstall ;; esac
[ "${#expires_at}" -le 12 ] || reject_package_uninstall
now="$(/usr/bin/date -u +%s)" || reject_package_uninstall
[ "${now}" -le "${expires_at}" ] || reject_package_uninstall
[ "${lease_token}" = "${expected_token}" ] || reject_package_uninstall
if [ "$prepare_pacman" -eq 1 ]; then
  /usr/lib/blockuntu/prepare-package-removal.sh || exit 1
fi
exit 0
