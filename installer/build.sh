#!/usr/bin/env bash
# Builds Forge-Setup-<version>.exe: the desktop app and ForgeCLI for Windows,
# cross-compiled with MinGW, packed by NSIS (installer/forge.nsi).
#
#   FORGECLI_DIR=../forgecli installer/build.sh [out-dir]
#
# Needs: rustup targets x86_64-pc-windows-gnu and x86_64-unknown-linux-musl,
# x86_64-w64-mingw32-gcc, musl-gcc (musl-tools), makensis.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cli="${FORGECLI_DIR:-$here/../forgecli}"
out="${1:-$here/target/installer}"
target=x86_64-pc-windows-gnu
version="$(grep -m1 '^version' "$here/Cargo.toml" | cut -d'"' -f2)"

(cd "$cli" && cargo build --release --target "$target" -p forge-cli)
# The ForgeCLI this app puts on remotes (WSL, SSH hosts, containers): static Linux.
(cd "$cli" && CC_x86_64_unknown_linux_musl=musl-gcc cargo build --release --target x86_64-unknown-linux-musl -p forge-cli)
(cd "$here" && cargo build --release --target "$target")

# Symbols out: the app's .exe drops from ~100 MB.
mkdir -p "$out/stage/app" "$out/stage/cli" "$out/stage/linux"  # apart: NSIS matches names case-blind
x86_64-w64-mingw32-strip -o "$out/stage/app/Forge.exe" "$here/target/$target/release/forge-gpui.exe"
x86_64-w64-mingw32-strip -o "$out/stage/cli/forge.exe" "$cli/target/$target/release/forge.exe"
cp "$cli/target/x86_64-unknown-linux-musl/release/forge" "$out/stage/linux/forge"

# Fonts for a personal build (installer/private-fonts, never committed):
# installed beside the app, where Forge reads them at start.
fonts=()
if compgen -G "$here/installer/private-fonts/*.[oOtT][tT][fF]" >/dev/null; then
  rm -rf "$out/stage/fonts" && mkdir -p "$out/stage/fonts"
  cp "$here"/installer/private-fonts/*.[oOtT][tT][fF] "$out/stage/fonts/"
  fonts=(-DFONTS_DIR="$out/stage/fonts")
fi

makensis -V2 -WX \
  "${fonts[@]}" \
  -DAPP_EXE="$out/stage/app/Forge.exe" \
  -DCLI_EXE="$out/stage/cli/forge.exe" \
  -DLINUX_CLI="$out/stage/linux/forge" \
  -DVERSION="$version" \
  -DOUT="$out/Forge-Setup-$version.exe" \
  "$here/installer/forge.nsi"
echo "$out/Forge-Setup-$version.exe"
