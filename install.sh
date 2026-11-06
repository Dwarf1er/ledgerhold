#!/usr/bin/env bash
# Builds Ledgerhold from source and installs it for the current user, no root needed.
#   ./install.sh        build and install the desktop app and the CLI into ~/.local
#   ./install.sh -u     remove what this script installed (never touches your data)
# Needs Rust and Node (`mise install` sets both up) and, on Linux, the Tauri system libraries.
set -euo pipefail

prefix=${PREFIX:-$HOME/.local}
bin=$prefix/bin
apps=$prefix/share/applications
icons=$prefix/share/icons/hicolor
root=$(cd "$(dirname "$0")" && pwd)

uninstall() {
  rm -f "$bin/ledgerhold" "$bin/ledgerhold-desktop" "$apps/ledgerhold.desktop" \
        "$icons/512x512/apps/ledgerhold.png" "$icons/128x128/apps/ledgerhold.png"
  command -v update-desktop-database >/dev/null && update-desktop-database "$apps" 2>/dev/null || true
  echo "Removed Ledgerhold from $prefix. Your data (the .sqlite file) was not touched."
}

case "${1:-}" in
  -u|--uninstall) uninstall; exit 0 ;;
  -h|--help) sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "") ;;
  *) echo "unknown option: $1 (try --help)" >&2; exit 1 ;;
esac

need() { command -v "$1" >/dev/null || { echo "error: '$1' is required. $2" >&2; exit 1; }; }
need cargo "Install Rust (https://rustup.rs) or run 'mise install'."
need node "Install Node.js or run 'mise install'."
need npm "Install Node.js or run 'mise install'."

cd "$root"
echo "==> building the command-line tool"
cargo build --release -p ledgerhold-cli
install -Dm755 target/release/ledgerhold "$bin/ledgerhold"

if [ "$(uname -s)" != "Linux" ]; then
  echo
  echo "Installed the command-line tool to $bin/ledgerhold."
  echo "For the desktop app on this OS, download the installer from the Releases page,"
  echo "or build it with 'npm ci && npm run tauri build' (bundles end up under target/release/bundle)."
  exit 0
fi

if command -v pkg-config >/dev/null && ! pkg-config --exists webkit2gtk-4.1; then
  cat >&2 <<'MSG'
error: the Tauri system libraries are missing. Install them first:
  Arch:    sudo pacman -S webkit2gtk-4.1 gtk3 librsvg libayatana-appindicator
  Debian:  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev
  Fedora:  sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel libappindicator-gtk3-devel
MSG
  exit 1
fi

echo "==> building the desktop app (this takes a few minutes the first time)"
npm ci --prefix ui
npm ci
# --no-bundle builds the release binary with the UI embedded, without making installers.
npm run tauri -- build --no-bundle
install -Dm755 target/release/ledgerhold-desktop "$bin/ledgerhold-desktop"

install -Dm644 src-tauri/icons/icon.png "$icons/512x512/apps/ledgerhold.png"
install -Dm644 src-tauri/icons/128x128.png "$icons/128x128/apps/ledgerhold.png"
mkdir -p "$apps"
cat > "$apps/ledgerhold.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Ledgerhold
Comment=Household bank ledger
Exec=$bin/ledgerhold-desktop
Icon=ledgerhold
Terminal=false
Categories=Office;Finance;
StartupWMClass=ledgerhold-desktop
DESKTOP
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true

echo
echo "Installed to $prefix:"
echo "  ledgerhold          command-line tool"
echo "  ledgerhold-desktop  the app (also in your application menu as Ledgerhold)"
case ":$PATH:" in *":$bin:"*) ;; *) echo "Note: $bin is not on your PATH." ;; esac
