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

# Symbols out: the app's .exe drops from ~100 MB.
mkdir -p "$out/stage/app" "$out/stage/cli"  # apart: NSIS matches names case-blind
x86_64-w64-mingw32-strip -o "$out/stage/app/Forge.exe" "$here/target/$target/release/forge-gpui.exe"
x86_64-w64-mingw32-strip -o "$out/stage/cli/forge.exe" "$cli/target/$target/release/forge.exe"

makensis -V2 -WX \
  -DAPP_EXE="$out/stage/app/Forge.exe" \
  -DCLI_EXE="$out/stage/cli/forge.exe" \
  -DVERSION="$version" \
  -DOUT="$out/Forge-Setup-$version.exe" \
  "$here/installer/forge.nsi"
echo "$out/Forge-Setup-$version.exe"
