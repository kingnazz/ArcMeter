#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <dmg-path> <arm64|x86_64>" >&2
  exit 64
fi

dmg_path="$1"
expected_arch="$2"

case "$expected_arch" in
  arm64 | x86_64) ;;
  *)
    echo "unsupported expected architecture: $expected_arch" >&2
    exit 64
    ;;
esac

if [[ ! -f "$dmg_path" ]]; then
  echo "DMG does not exist: $dmg_path" >&2
  exit 66
fi

mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/arcmeter-dmg.XXXXXX")"
mounted=false

cleanup() {
  if [[ "$mounted" == true ]]; then
    hdiutil detach "$mount_dir" >/dev/null
  fi
  rmdir "$mount_dir"
}
trap cleanup EXIT

hdiutil verify "$dmg_path"
hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "$dmg_path" >/dev/null
mounted=true

app_path="$mount_dir/ArcMeter.app"
executable_path="$app_path/Contents/MacOS/arcmeter"

if [[ ! -d "$app_path" || ! -f "$executable_path" ]]; then
  echo "ArcMeter.app is incomplete inside the final DMG" >&2
  exit 65
fi

codesign --verify --deep --strict --verbose=4 "$app_path"
codesign -dvvv "$app_path"
file "$executable_path"

actual_archs="$(lipo -archs "$executable_path")"
if [[ "$actual_archs" != "$expected_arch" ]]; then
  echo "architecture mismatch: expected $expected_arch, found $actual_archs" >&2
  exit 65
fi

shasum -a 256 "$dmg_path"
echo "Verified final DMG app: strict ad-hoc signature and $expected_arch architecture"
