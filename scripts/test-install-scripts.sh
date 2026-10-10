#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
temporary_home="$(mktemp -d)"
trap 'rm -rf "$temporary_home"' EXIT

mkdir -p "$temporary_home/Applications/Vibecodium.app" "$temporary_home/Notes"
printf 'keep\n' > "$temporary_home/Notes/Keep.md"
HOME="$temporary_home" bash "$script_dir/uninstall.sh" >/dev/null

[[ ! -e "$temporary_home/Applications/Vibecodium.app" ]]
[[ "$(cat "$temporary_home/Notes/Keep.md")" == keep ]]
