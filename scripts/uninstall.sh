#!/usr/bin/env bash
set -euo pipefail

APP_PATH="$HOME/Applications/Vibecodium.app"

if [[ -d "$APP_PATH" ]]; then
  rm -rf "$APP_PATH"
  printf 'Removed Vibecodium from %s\n' "$APP_PATH"
else
  printf 'Vibecodium is not installed in %s\n' "$HOME/Applications"
fi

printf 'Vaults, Markdown files, and Vibecodium settings were left untouched.\n'
