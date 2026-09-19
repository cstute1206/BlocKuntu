#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s DOMAIN [TIMEOUT_SECONDS]\n' "${0##*/}" >&2
}

[[ "${#}" -ge 1 && "${#}" -le 2 ]] || {
  usage
  exit 2
}

domain="${1}"
timeout_seconds="${2:-120}"
[[ "${timeout_seconds}" =~ ^[0-9]+$ ]] || {
  printf 'timeout must be a non-negative integer: %s\n' "${timeout_seconds}" >&2
  exit 2
}

command -v virsh >/dev/null 2>&1 || {
  printf 'virsh is required\n' >&2
  exit 1
}
virsh dominfo "${domain}" >/dev/null

deadline=$((SECONDS + timeout_seconds))
while (( SECONDS <= deadline )); do
  lease_output="$(virsh domifaddr "${domain}" --source lease 2>/dev/null || true)"
  mapfile -t addresses < <(
    awk '$3 == "ipv4" { sub(/\/.*/, "", $4); print $4 }' <<<"${lease_output}"
  )

  if [[ "${#addresses[@]}" -eq 1 ]]; then
    printf '%s\n' "${addresses[0]}"
    exit 0
  fi
  if [[ "${#addresses[@]}" -gt 1 ]]; then
    printf '%s: found multiple IPv4 DHCP addresses; refusing to guess:\n' \
      "${domain}" >&2
    printf '  %s\n' "${addresses[@]}" >&2
    exit 1
  fi

  sleep 2
done

printf '%s: no IPv4 DHCP lease appeared within %s seconds\n' \
  "${domain}" "${timeout_seconds}" >&2
exit 1
