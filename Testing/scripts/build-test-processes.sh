#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TESTING_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
SOURCE="${TESTING_ROOT}/fixtures/test-processes/test_process.c"
OUTPUT_DIR="${TESTING_ROOT}/artifacts/fixtures/bin"

if [[ "${1:-}" == "--output-dir" ]]; then
  [[ $# -eq 2 ]] || {
    printf 'usage: %s [--output-dir DIR]\n' "$0" >&2
    exit 2
  }
  OUTPUT_DIR="$2"
elif [[ $# -ne 0 ]]; then
  printf 'usage: %s [--output-dir DIR]\n' "$0" >&2
  exit 2
fi

COMPILER="${CC:-cc}"
command -v "${COMPILER}" >/dev/null 2>&1 || {
  printf 'missing C compiler: %s\n' "${COMPILER}" >&2
  exit 1
}

mkdir -p "${OUTPUT_DIR}"
for executable in \
  blockuntu-test-app-a \
  blockuntu-test-app-b \
  blockuntu-test-block \
  blockuntu-test-parent \
  blockuntu-test-helper
do
  "${COMPILER}" \
    -std=c11 \
    -O2 \
    -Wall \
    -Wextra \
    -Werror \
    -pedantic \
    "${SOURCE}" \
    -o "${OUTPUT_DIR}/${executable}"
done

printf 'Built test processes in %s\n' "${OUTPUT_DIR}"
