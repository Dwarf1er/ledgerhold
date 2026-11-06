#!/usr/bin/env bash
# Stamps a version into Cargo.toml ([workspace.package]) and src-tauri/tauri.conf.json.
# Usage: scripts/set-version.sh 1.2.3   (or 1.2.3-rc.1)
set -euo pipefail

version=${1:?version, e.g. 1.2.3}
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: '$version' is not a valid version (expected e.g. 1.2.3 or 1.2.3-rc.1)" >&2
  exit 1
fi
cd "$(dirname "$0")/.."

# Node is already required for the UI, so use it instead of juggling GNU vs BSD sed.
node - "$version" <<'JS'
const fs = require("fs");
const version = process.argv[2];

const cargo = fs.readFileSync("Cargo.toml", "utf8");
const re = /(\[workspace\.package\][^\[]*?\nversion = ")[^"]*(")/s;
if (!re.test(cargo)) throw new Error("no version found under [workspace.package] in Cargo.toml");
fs.writeFileSync("Cargo.toml", cargo.replace(re, `$1${version}$2`));

const confPath = "src-tauri/tauri.conf.json";
const conf = JSON.parse(fs.readFileSync(confPath, "utf8"));
conf.version = version;
fs.writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");
console.log(`version set to ${version}`);
JS
