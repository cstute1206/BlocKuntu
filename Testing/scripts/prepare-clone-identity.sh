#!/usr/bin/env bash
# Only runner-owned clones can have their guest identities reset.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [[ "$#" != 3 || "$1" != --apply ]]; then
  printf 'Usage: %s --apply TEMPLATE RUN_ID\nUse phase0-vm.py prepare to create an owned clone first.\n' "${0##*/}" >&2
  exit 2
fi
exec python3 "${script_dir}/phase0-vm.py" identity "$2" --run-id "$3"
