#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TESTING_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
CONFIG="${1:-${TESTING_ROOT}/config/vm-templates.json}"
LOCAL_CONFIG="${2:-${TESTING_ROOT}/config/vm-templates.local.json}"

command -v python3 >/dev/null 2>&1 || {
  printf 'python3 is required\n' >&2
  exit 1
}
command -v virsh >/dev/null 2>&1 || {
  printf 'virsh is required\n' >&2
  exit 1
}
[[ -r "${CONFIG}" ]] || {
  printf 'VM template configuration is not readable: %s\n' "${CONFIG}" >&2
  exit 1
}

python3 -c '
import json
import sys

config_path, local_path = sys.argv[1:]
with open(config_path, encoding="utf-8") as file:
    config = json.load(file)

if config.get("schema_version") != 1:
    raise SystemExit("VM template schema_version must be 1")
templates = config.get("templates")
if not isinstance(templates, dict) or not templates:
    raise SystemExit("VM template configuration has no templates")

profiles = {}
try:
    with open(local_path, encoding="utf-8") as file:
        local = json.load(file)
except FileNotFoundError:
    local = None
if local is not None:
    if local.get("schema_version") != 1:
        raise SystemExit("local VM template schema_version must be 1")
    profiles = local.get("ssh_profiles", {})

for key, item in sorted(templates.items()):
    if item.get("immutable_base") is not True:
        raise SystemExit(f"{key}: immutable_base must be true")
    if not item.get("guest_user"):
        raise SystemExit(f"{key}: guest_user is required")
    transport = item.get("transport", {})
    if transport.get("kind") != "ssh" or transport.get("port") != 22:
        raise SystemExit(f"{key}: SSH transport on port 22 is required")
    discovery = transport.get("address_discovery", {})
    if discovery.get("kind") != "libvirt_dhcp_lease":
        raise SystemExit(f"{key}: libvirt DHCP lease discovery is required")
    if not discovery.get("network"):
        raise SystemExit(f"{key}: address discovery network is required")
    profile = transport.get("identity_profile")
    if not profile:
        raise SystemExit(f"{key}: SSH identity_profile is required")
    if local is not None and profile not in profiles:
        raise SystemExit(f"{key}: SSH profile {profile!r} is missing locally")
    clone_identity = item.get("clone_identity", {})
    if clone_identity.get("requires_powered_off") is not True:
        raise SystemExit(f"{key}: clone identity reset must require power off")
    if clone_identity.get("operations") != ["machine-id", "ssh-hostkeys"]:
        raise SystemExit(f"{key}: unexpected clone identity operations")
' "${CONFIG}" "${LOCAL_CONFIG}"

mapfile -t templates < <(
  python3 -c '
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    config = json.load(file)
for key, item in sorted(config["templates"].items()):
    domain = item["domain"]
    snapshot = item.get("libvirt_snapshot") or "-"
    immutable = "true" if item.get("immutable_base") is True else "false"
    print(f"{key}\t{domain}\t{immutable}\t{snapshot}")
' "${CONFIG}"
)

[[ "${#templates[@]}" -gt 0 ]] || {
  printf 'no VM templates are configured\n' >&2
  exit 1
}

for row in "${templates[@]}"; do
  IFS=$'\t' read -r key domain immutable snapshot <<<"${row}"
  [[ "${immutable}" == "true" ]] || {
    printf '%s: base domain must be marked immutable\n' "${key}" >&2
    exit 1
  }
  state="$(virsh domstate "${domain}" | tr -d '\r')"
  [[ "${state}" == "shut off" ]] || {
    printf '%s: expected base domain %s to be shut off, got %s\n' \
      "${key}" "${domain}" "${state}" >&2
    exit 1
  }
  if [[ "${snapshot}" != "-" ]]; then
    virsh snapshot-info "${domain}" "${snapshot}" >/dev/null
  fi
  printf '%s: %s is present, immutable, and shut off' "${key}" "${domain}"
  if [[ "${snapshot}" == "-" ]]; then
    printf '; clone the full base because no snapshot is configured\n'
  else
    printf '; snapshot %s exists\n' "${snapshot}"
  fi
done

if [[ -e "${LOCAL_CONFIG}" ]]; then
  [[ -r "${LOCAL_CONFIG}" ]] || {
    printf 'local VM configuration is not readable: %s\n' "${LOCAL_CONFIG}" >&2
    exit 1
  }
  mapfile -t identity_files < <(
    python3 -c '
import json
import sys

with open(sys.argv[1], encoding="utf-8") as file:
    config = json.load(file)
for profile in config["ssh_profiles"].values():
    print(profile["identity_file"])
' "${LOCAL_CONFIG}"
  )
  for identity_file in "${identity_files[@]}"; do
    [[ "${identity_file}" == /* ]] || {
      printf 'SSH identity path must be absolute: %s\n' "${identity_file}" >&2
      exit 1
    }
    [[ -r "${identity_file}" ]] || {
      printf 'SSH identity is not readable: %s\n' "${identity_file}" >&2
      exit 1
    }
    python3 -c '
import os
import sys

path = sys.argv[1]
if os.stat(path).st_mode & 0o077:
    raise SystemExit(f"SSH identity permissions are too broad: {path}")
' "${identity_file}"
    printf 'SSH identity is readable and owner-only: %s\n' "${identity_file}"
  done
else
  printf 'local SSH configuration not found; copy %s.local.example.json\n' \
    "${CONFIG%.json}"
fi
