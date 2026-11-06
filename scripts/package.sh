#!/usr/bin/env bash
# Builds the desktop installers and the CLI for one platform into a staging directory.
# Usage: scripts/package.sh <rid> <version> <staging-dir>
#   rid: linux-x64 | linux-arm64 | win-x64 | osx-arm64 | osx-x64
# Run it on the target OS (macOS x64 is cross-compiled on an Apple silicon runner). Needs Rust, Node and, on Linux,
# the Tauri system libraries (see README). The version is stamped into the sources, so run it on a clean checkout.
set -euo pipefail

rid=${1:?runtime id, e.g. linux-x64}
version=${2:?version, e.g. 1.2.3}
out=${3:?staging directory}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

triple=""
exe=""
case "$rid" in
  linux-x64)   bundles="deb,rpm,appimage" ;;
  # AppImage tooling is x86-first; deb and rpm cover arm64 Linux.
  linux-arm64) bundles="deb,rpm" ;;
  win-x64)     bundles="msi,nsis"; exe=".exe" ;;
  osx-arm64)   bundles="dmg"; triple="aarch64-apple-darwin" ;;
  osx-x64)     bundles="dmg"; triple="x86_64-apple-darwin" ;;
  *) echo "error: unknown rid '$rid'" >&2; exit 1 ;;
esac

rm -rf "$out"
mkdir -p "$out"

echo "==> stamping version $version"
scripts/set-version.sh "$version"

echo "==> installing JavaScript dependencies"
npm ci --prefix ui
npm ci

target_args=()
target_dir=target
if [ -n "$triple" ]; then
  target_args=(--target "$triple")
  target_dir="target/$triple"
fi

echo "==> building the CLI ($rid)"
cargo build --release -p ledgerhold-cli "${target_args[@]}"

echo "==> building the desktop app: $bundles"
# beforeBuildCommand in tauri.conf.json builds the UI.
npm run tauri -- build --bundles "$bundles" "${target_args[@]}"

bundle_dir="$target_dir/release/bundle"
copied=0
while IFS= read -r -d '' f; do
  cp "$f" "$out/"
  copied=$((copied + 1))
done < <(find "$bundle_dir" -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*.msi' -o -name '*-setup.exe' -o -name '*.dmg' \) -print0)
if [ "$copied" -eq 0 ]; then
  echo "error: no installers found under $bundle_dir" >&2
  exit 1
fi

# The CLI: a folder with the binary, licence and readme (zipped by the workflow on Windows, tarred here elsewhere).
name="ledgerhold-cli-$version-$rid"
stage="$out/$name"
mkdir -p "$stage"
cp "$target_dir/release/ledgerhold$exe" "$stage/"
cp LICENSE README.md "$stage/"
if [ "$rid" != "win-x64" ]; then
  tar -C "$out" -czf "$out/$name.tar.gz" "$name"
  rm -r "$stage"
fi

echo "==> staged in $out:"
ls -l "$out"
