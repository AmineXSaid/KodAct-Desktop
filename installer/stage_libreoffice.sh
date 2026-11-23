#!/usr/bin/env bash
# Stages the LibreOffice the installer puts beside the app (`libreoffice\`),
# so Word, PowerPoint and Excel files show as they print in the side panel
# (`src/office_render.rs`), PDF export works and workbooks recalculate, on a
# PC that has no LibreOffice. The Document Foundation's Windows x64 MSI,
# pinned by SHA-256, is unpacked here (msiextract) into a folder that runs
# as it is: no MSI install, no administrator rights.
#
# What a headless renderer never uses is left out: the UI translations and
# language packs past a few languages, spelling dictionaries past those, and
# the Wiki Publisher and Solver extensions. Its fonts go to
# `share\fonts\truetype`, where LibreOffice reads its own, and the Visual C++
# runtime it ships for System32 sits beside `soffice.exe` instead.
#
#   installer/stage_libreoffice.sh DEST
#
# Needs: curl, sha256sum, msiextract (msitools). The download (~360 MB) is
# cached in ~/.cache/kodact-installer.
set -euo pipefail
dest="${1:?usage: stage_libreoffice.sh DEST}"

version=26.8.3
sha=0e6918c273eabbdbbd800ee0dacbbf130a14c13c2e4cec861b06dd59e015bf94
url="https://download.documentfoundation.org/libreoffice/stable/$version/win/x86_64/LibreOffice_${version}_Win_x86-64.msi"
# The languages kept (UI, language packs, dictionaries): English always.
keep_langs="en en_US en-US en_GB en-GB fr ar de es"

cache="${KODACT_INSTALLER_CACHE:-$HOME/.cache/kodact-installer}"
msi="$cache/LibreOffice_${version}_Win_x86-64.msi"
mkdir -p "$cache"
if [ ! -f "$msi" ]; then
  curl -fsSL -o "$msi.part" "$url"
  mv "$msi.part" "$msi"
fi
echo "$sha  $msi" | sha256sum -c - >/dev/null || {
  echo "error: $msi is not the pinned LibreOffice $version (SHA-256 differs)" >&2
  exit 1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
(cd "$work" && msiextract "$msi" >/dev/null)
# msiextract names the install folder "LibreOffice:." after the MSI's source dir.
root="$(find "$work" -maxdepth 1 -type d -name 'LibreOffice*' | head -1)"
[ -f "$root/program/soffice.exe" ] || { echo "error: no soffice.exe in $msi" >&2; exit 1; }

rm -rf "$dest" && mkdir -p "$(dirname "$dest")"
mv "$root" "$dest"

kept() { for l in $keep_langs; do [ "$1" = "$l" ] && return 0; done; return 1; }

# UI translations and language packs.
for d in "$dest"/program/resource/*/; do
  kept "$(basename "$d")" || rm -rf "$d"
done
for f in "$dest"/share/registry/Langpack-*.xcd "$dest"/share/registry/res/registry_*.xcd; do
  [ -e "$f" ] || continue
  l="$(basename "$f" .xcd)"; l="${l#Langpack-}"; l="${l#registry_}"
  kept "$l" || rm -f "$f"
done
# Spelling and hyphenation dictionaries, and extensions a renderer never runs.
for d in "$dest"/share/extensions/dict-*/; do
  l="$(basename "$d")"; l="${l#dict-}"
  kept "$l" || rm -rf "$d"
done
rm -rf "$dest/share/extensions/wiki-publisher" "$dest/share/extensions/nlpsolver"
rm -rf "$dest/help" "$dest/readmes"

# Its fonts, where it looks for its own; the VC++ runtime beside the program.
mkdir -p "$dest/share/fonts/truetype"
cp "$work"/Fonts/* "$dest/share/fonts/truetype/" 2>/dev/null || true
cp -n "$work"/System64/*.dll "$dest/program/" 2>/dev/null || true

echo "$dest: LibreOffice $version, $(du -sh "$dest" | cut -f1)"
