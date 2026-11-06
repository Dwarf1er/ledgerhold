#!/usr/bin/env bash
# Checks what scripts/package.sh produced. Usage: scripts/smoke.sh <staging-dir> <version> <rid>
# Runs the CLI for real (import, balance, export against a throwaway database) wherever the runner can execute it,
# and inspects each installer without installing it.
set -euo pipefail

out=$(cd "${1:?staging directory}" && pwd)   # absolute: later steps change directory
version=${2:?version}
rid=${3:?rid}
root=$(cd "$(dirname "$0")/.." && pwd)

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }

shopt -s nullglob
need() { local f=("$out"/$1); [ "${#f[@]}" -ge 1 ] || fail "missing $1 in $out"; ok "found $(basename "${f[0]}")"; }

# ---- installers present ------------------------------------------------------------------
case "$rid" in
  linux-x64)   need "*.deb"; need "*.rpm"; need "*.AppImage" ;;
  linux-arm64) need "*.deb"; need "*.rpm" ;;
  win-x64)     need "*.msi"; need "*-setup.exe" ;;
  osx-*)       need "*.dmg" ;;
esac

# ---- the CLI runs and works ------------------------------------------------------------------
can_run=true
[ "$rid" = "osx-x64" ] && can_run=false   # cross-compiled on an arm64 runner
if $can_run; then
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  if [ "$rid" = "win-x64" ]; then
    cli="$out/ledgerhold-cli-$version-$rid/ledgerhold.exe"
  else
    tar -C "$work" -xzf "$out/ledgerhold-cli-$version-$rid.tar.gz"
    cli="$work/ledgerhold-cli-$version-$rid/ledgerhold"
  fi
  [ -x "$cli" ] || fail "CLI binary not found at $cli"
  [ "$("$cli" --version)" = "ledgerhold $version" ] || fail "CLI reports '$("$cli" --version)', expected 'ledgerhold $version'"
  ok "CLI reports version $version"

  db="$work/smoke.sqlite"
  "$cli" --db "$db" person add Alex >/dev/null
  "$cli" --db "$db" institution add Desjardins --bank-id 815 >/dev/null
  "$cli" --db "$db" account add --institution Desjardins --name Joint --kind checking --number EOP-12345 --owner Alex >/dev/null
  "$cli" --db "$db" anchor set Joint 2026-01-01 700.00 >/dev/null
  imported=$("$cli" --db "$db" import Joint "$root/fixtures/synthetic/desjardins.csv" --parser desjardins)
  echo "$imported" | grep -q "imported 10 new" || fail "unexpected import output: $imported"
  balance=$("$cli" --db "$db" balance Joint --on 2026-01-31)
  echo "$balance" | grep -q "1961.45" || fail "unexpected balance: $balance"
  "$cli" --db "$db" export Joint --month 2026-01 -o "$work/ofx" >/dev/null
  grep -q "<LEDGERBAL>" "$work"/ofx/*.ofx || fail "exported OFX has no ledger balance"
  ok "CLI imported 10 transactions, balance 1961.45, exported OFX"
fi

# ---- inspect installers ----------------------------------------------------------------------
for deb in "$out"/*.deb; do
  command -v dpkg-deb >/dev/null || { echo "skip: dpkg-deb not available"; break; }
  contents=$(dpkg-deb -c "$deb")
  echo "$contents" | grep -q "usr/bin/ledgerhold-desktop" || fail "$(basename "$deb") lacks the app binary"
  echo "$contents" | grep -q "\.desktop" || fail "$(basename "$deb") lacks a desktop entry"
  echo "$contents" | grep -q "\.png" || fail "$(basename "$deb") lacks an icon"
  dpkg-deb -I "$deb" | grep -q "Version: $version" || fail "$(basename "$deb") has the wrong version"
  ok "$(basename "$deb") has the binary, desktop entry, icon and version $version"
done

for rpm in "$out"/*.rpm; do
  command -v rpm >/dev/null || { echo "skip: rpm not available"; break; }
  rpm -qpl "$rpm" | grep -q "usr/bin/ledgerhold-desktop" || fail "$(basename "$rpm") lacks the app binary"
  ok "$(basename "$rpm") has the app binary"
done

for img in "$out"/*.AppImage; do
  chmod +x "$img"
  (cd "$work" && "$img" --appimage-extract >/dev/null) || fail "cannot unpack $(basename "$img")"
  [ -x "$work/squashfs-root/usr/bin/ledgerhold-desktop" ] || fail "$(basename "$img") lacks the app binary"
  ok "$(basename "$img") unpacks and contains the app binary"
done
echo "smoke tests passed for $rid"
