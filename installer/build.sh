#!/usr/bin/env bash
# Builds Forge-Setup-<version>.exe: the desktop app and ForgeCLI for Windows,
# cross-compiled with MinGW, packed by NSIS (installer/forge.nsi).
#
#   FORGECLI_DIR=../forgecli installer/build.sh [out-dir]
#
# Needs: rustup target x86_64-pc-windows-gnu, x86_64-w64-mingw32-gcc, makensis.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cli="${FORGECLI_DIR:-$here/../forgecli}"
out="${1:-$here/target/installer}"
target=x86_64-pc-windows-gnu
version="$(grep -m1 '^version' "$here/Cargo.toml" | cut -d'"' -f2)"

(cd "$cli" && cargo build --release --target "$target" -p forge-cli)
(cd "$here" && cargo build --release --target "$target")

mkdir -p "$out"
makensis -V2 -WX \
  -DAPP_EXE="$here/target/$target/release/forge-gpui.exe" \
  -DCLI_EXE="$cli/target/$target/release/forge.exe" \
  -DVERSION="$version" \
  -DOUT="$out/Forge-Setup-$version.exe" \
  "$here/installer/forge.nsi"
echo "$out/Forge-Setup-$version.exe"
