#!/usr/bin/env bash
# Supply a readable appliance kernel without changing host /boot permissions.
set -euo pipefail
testing_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
kernel_release="$(uname -r)"
kernel_dir="${testing_root}/runtime/guestfs-kernel/${kernel_release}"
kernel_path="${kernel_dir}/extracted/boot/vmlinuz-${kernel_release}"
mkdir -p "${kernel_dir}"
if [[ ! -r "${kernel_path}" ]]; then
  package="linux-image-${kernel_release}"
  version="$(dpkg-query -W -f='${Version}' "${package}")"
  (cd -- "${kernel_dir}" && apt-get download "${package}=${version}")
  packages=("${kernel_dir}"/*.deb)
  [[ "${#packages[@]}" -eq 1 && -f "${packages[0]}" ]]
  dpkg-deb --extract "${packages[0]}" "${kernel_dir}/extracted"
  # Package mode is 0600; extraction is owned by the invoking user.
fi
[[ -r "${kernel_path}" ]]
printf 'Guestfs appliance kernel ready: %s\n' "${kernel_path}"
