#!/usr/bin/env bash
# Builds KodAct-Setup-<version>.exe: the desktop app and KodAct CLI for Windows,
# cross-compiled with MinGW, packed by NSIS (installer/kodact.nsi).
#
#   KODACT_DIR=../kodact installer/build.sh [out-dir]
#
# Needs: rustup targets x86_64-pc-windows-gnu and x86_64-unknown-linux-musl,
# x86_64-w64-mingw32-gcc, musl-gcc (musl-tools), makensis; python3 with pip
# and Pillow, curl and unzip (the bundled Python, `stage_python.sh`; the
# pictures), and msiextract (msitools, the bundled LibreOffice).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cli="${KODACT_DIR:-$here/../kodact}"
out="${1:-$here/target/installer}"
target=x86_64-pc-windows-gnu
version="$(grep -m1 '^version' "$here/Cargo.toml" | cut -d'"' -f2)"

# The app speaks the steering protocol (a message sent mid-turn queued with
# priority "next", `cancel_queued`), which KodAct CLI's main has.
# A checkout without it would build an installer whose mid-turn messages fail.
if ! grep -q '"cancel_queued"' "$cli/crates/kodact-cli/src/host.rs" 2>/dev/null; then
  echo "error: $cli has no steering protocol; check out KodAct CLI's main" >&2
  exit 1
fi

(cd "$cli" && cargo build --release --target "$target" -p kodact-cli)
# The KodAct CLI this app puts on remotes (WSL, SSH hosts, containers): static Linux.
(cd "$cli" && CC_x86_64_unknown_linux_musl=musl-gcc cargo build --release --target x86_64-unknown-linux-musl -p kodact-cli)
(cd "$here" && cargo build --release --target "$target")

# Symbols out: the app's .exe drops from ~100 MB.
mkdir -p "$out/stage/app" "$out/stage/cli" "$out/stage/linux"  # apart: NSIS matches names case-blind
x86_64-w64-mingw32-strip -o "$out/stage/app/KodAct.exe" "$here/target/$target/release/kodact-desktop.exe"
x86_64-w64-mingw32-strip -o "$out/stage/cli/kodact.exe" "$cli/target/$target/release/kodact.exe"
cp "$cli/target/x86_64-unknown-linux-musl/release/kodact" "$out/stage/linux/kodact"

# Fonts for a personal build (installer/private-fonts, never committed):
# installed beside the app, where KodAct reads them at start.
fonts=()
if compgen -G "$here/installer/private-fonts/*.[oOtT][tT][fF]" >/dev/null; then
  rm -rf "$out/stage/fonts" && mkdir -p "$out/stage/fonts"
  cp "$here"/installer/private-fonts/*.[oOtT][tT][fF] "$out/stage/fonts/"
  fonts=(-DFONTS_DIR="$out/stage/fonts")
fi

# The design library: its committed files and the media its lock pins
# (fetched once, cached by hash), checked, and nothing else from the folder.
python3 -I "$here/scripts/sync_open_design.py" media
python3 -I "$here/scripts/sync_open_design.py" check
python3 -I "$here/scripts/sync_open_design.py" stage "$out/stage/design-library"

# Python with the document libraries, for a PC that has none: what the
# office-documents skill's tools run on (`src/doc_tools.rs` finds it).
"$here/installer/stage_python.sh" "$out/stage/python"

# LibreOffice, so Office files show as they print (`src/office_render.rs`).
# KODACT_NO_LIBREOFFICE=1 builds a smaller installer without it.
office=()
if [ "${KODACT_NO_LIBREOFFICE:-}" != 1 ]; then
  "$here/installer/stage_libreoffice.sh" "$out/stage/libreoffice"
  office=(-DLIBREOFFICE_DIR="$out/stage/libreoffice")
fi

# The installer's pictures, in KodAct's colours.
python3 -I "$here/scripts/make_installer_art.py" "$out/stage/art"

makensis -V2 -WX \
  "${fonts[@]}" \
  "${office[@]}" \
  -DART_DIR="$out/stage/art" \
  -DPYTHON_DIR="$out/stage/python" \
  -DDESIGN_DIR="$out/stage/design-library" \
  -DAPP_EXE="$out/stage/app/KodAct.exe" \
  -DCLI_EXE="$out/stage/cli/kodact.exe" \
  -DLINUX_CLI="$out/stage/linux/kodact" \
  -DVERSION="$version" \
  -DOUT="$out/KodAct-Setup-$version.exe" \
  "$here/installer/kodact.nsi"
echo "$out/KodAct-Setup-$version.exe"
