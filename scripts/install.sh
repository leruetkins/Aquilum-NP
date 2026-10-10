#!/usr/bin/env bash
set -euo pipefail

REPO="Freaction/Vibecodium"
APP_DIR="$HOME/Applications"
APP_PATH="$APP_DIR/Vibecodium.app"

if [[ "$(uname -s)" != "Darwin" ]]; then
  printf 'Vibecodium curl installer currently supports macOS only.\n' >&2
  exit 1
fi

case "$(uname -m)" in
  arm64) target="aarch64" ;;
  x86_64) target="x64" ;;
  *) printf 'Vibecodium DMG installer supports Apple Silicon and Intel Macs only.\n' >&2; exit 1 ;;
esac

work_dir="$(mktemp -d)"
mount_dir="$work_dir/mount"
mkdir "$mount_dir"
mounted=false
cleanup() {
  if [[ "$mounted" == true ]]; then
    hdiutil detach "$mount_dir" -quiet || true
  fi
  rm -rf "$work_dir"
}
trap cleanup EXIT

curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" -o "$work_dir/release.json"
download_url="$(sed -n 's/^[[:space:]]*"browser_download_url":[[:space:]]*"\(.*\)",\{0,1\}$/\1/p' "$work_dir/release.json" | grep "_${target}\.dmg$" | head -n 1 || true)"
if [[ -z "$download_url" ]]; then
  printf 'Latest GitHub release has no macOS %s DMG.\n' "$target" >&2
  exit 1
fi

curl -fL --proto '=https' --tlsv1.2 "$download_url" -o "$work_dir/Vibecodium.dmg"
hdiutil attach -nobrowse -readonly -mountpoint "$mount_dir" "$work_dir/Vibecodium.dmg" >/dev/null
mounted=true
source_app="$(find "$mount_dir" -maxdepth 3 -type d -name 'Vibecodium.app' -print -quit)"
if [[ -z "$source_app" ]]; then
  printf 'Vibecodium.app was not found in the downloaded DMG.\n' >&2
  exit 1
fi

mkdir -p "$APP_DIR"
rm -rf "$APP_PATH"
ditto "$source_app" "$APP_PATH"
printf 'Installed Vibecodium to %s\n' "$APP_PATH"
