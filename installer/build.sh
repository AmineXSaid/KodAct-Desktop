#!/usr/bin/env bash
# Builds KodAct-Setup-<version>.exe: the desktop app and KodAct CLI for Windows,
# cross-compiled with MinGW, behind KodAct's own setup window (installer/setup),
# with everything it installs appended to it by installer/pack.py.
#
#   KODACT_DIR=../kodact installer/build.sh [out-dir]
#
# Needs: rustup targets x86_64-pc-windows-gnu and x86_64-unknown-linux-musl,
# x86_64-w64-mingw32-gcc, musl-gcc (musl-tools); python3 with pip, curl and
# unzip (the bundled Python, `stage_python.sh`), and msiextract (msitools,
# the bundled LibreOffice).
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

# Symbols out: the app's .exe drops from ~100 MB. The install folder is
# staged as it will be laid out, then appended to the setup.
root="$out/stage/root"
rm -rf "$root" && mkdir -p "$root/bin/linux-x86_64"
x86_64-w64-mingw32-strip -o "$out/stage/setup.exe" "$here/target/$target/release/kodact-setup.exe"
x86_64-w64-mingw32-strip -o "$root/KodAct.exe" "$here/target/$target/release/kodact-desktop.exe"
cp "$here/assets/kodact.ico" "$root/kodact.ico"
x86_64-w64-mingw32-strip -o "$root/bin/kodact.exe" "$cli/target/$target/release/kodact.exe"
# The KodAct CLI the app copies to a remote the first time.
cp "$cli/target/x86_64-unknown-linux-musl/release/kodact" "$root/bin/linux-x86_64/kodact"

# Fonts for a personal build (installer/private-fonts, never committed):
# installed beside the app, where KodAct reads them at start.
if compgen -G "$here/installer/private-fonts/*.[oOtT][tT][fF]" >/dev/null; then
  mkdir -p "$root/fonts"
  cp "$here"/installer/private-fonts/*.[oOtT][tT][fF] "$root/fonts/"
fi

# The design library: its committed files and the media its lock pins
# (fetched once, cached by hash), checked, and nothing else from the folder.
python3 -I "$here/scripts/sync_open_design.py" media
python3 -I "$here/scripts/sync_open_design.py" check
python3 -I "$here/scripts/sync_open_design.py" stage "$root/design-library"

# Python with the document libraries, for a PC that has none: what the
# office-documents skill's tools run on (`src/doc_tools.rs` finds it).
"$here/installer/stage_python.sh" "$root/python"

# LibreOffice, so Office files show as they print (`src/office_render.rs`):
# a part the setup lets you leave out. KODACT_NO_LIBREOFFICE=1 builds a
# smaller setup without it.
if [ "${KODACT_NO_LIBREOFFICE:-}" != 1 ]; then
  "$here/installer/stage_libreoffice.sh" "$root/libreoffice"
fi

python3 -I "$here/installer/pack.py" "$out/stage/setup.exe" "$root" "$version" "$out/KodAct-Setup-$version.exe"
(cd "$out" && sha256sum "KodAct-Setup-$version.exe" > "KodAct-Setup-$version.exe.sha256")
echo "$out/KodAct-Setup-$version.exe"
