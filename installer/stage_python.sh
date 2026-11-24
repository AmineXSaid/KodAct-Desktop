#!/usr/bin/env bash
# Stages the Python the installer puts beside the app (`python\python.exe`),
# so the office-documents skill works on a PC that has no Python: python.org's
# embeddable build for Windows x64, pinned by SHA-256, with the document
# libraries `src/doc_tools.rs` pins (and what they need) as Windows wheels in
# its `Lib\site-packages`. Nothing is run from it here: a Linux pip fetches
# the Windows wheels.
#
#   installer/stage_python.sh DEST
#
# Needs: python3 with pip, curl, unzip, sha256sum. The download is cached in
# ~/.cache/kodact-installer.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dest="${1:?usage: stage_python.sh DEST}"

version=3.12.8
tag=312
sha=8d3f33be9eb810f23c102f08475af2854e50484b8e4e06275e937be61ce3d2fb
url="https://www.python.org/ftp/python/$version/python-$version-embed-amd64.zip"

cache="${KODACT_INSTALLER_CACHE:-$HOME/.cache/kodact-installer}"
zip="$cache/python-$version-embed-amd64.zip"
mkdir -p "$cache"
if [ ! -f "$zip" ]; then
  curl -fsSL -o "$zip.part" "$url"
  mv "$zip.part" "$zip"
fi
echo "$sha  $zip" | sha256sum -c - >/dev/null || {
  echo "error: $zip is not the pinned Python $version (SHA-256 differs)" >&2
  exit 1
}

rm -rf "$dest" && mkdir -p "$dest"
unzip -q "$zip" -d "$dest"

# The embeddable build ignores site-packages until its ._pth says otherwise.
printf 'python%s.zip\r\n.\r\nLib\\site-packages\r\nimport site\r\n' "$tag" > "$dest/python$tag._pth"

# The pins, from the one place they are written.
mapfile -t libs < <(sed -n 's/^ *("\([a-zA-Z0-9_.-]*==[0-9][^"]*\)", "[a-z0-9_]*"),$/\1/p' "$here/src/doc_tools.rs")
[ "${#libs[@]}" -ge 6 ] || { echo "error: no pins found in src/doc_tools.rs" >&2; exit 1; }

python3 -m pip install --quiet --disable-pip-version-check --no-input \
  --target "$dest/Lib/site-packages" \
  --platform win_amd64 --python-version "$version" --implementation cp \
  --only-binary=:all: --no-compile \
  "${libs[@]}"

# What a PC never needs from the wheels.
find "$dest/Lib/site-packages" -type d \( -name __pycache__ -o -name tests \) -prune -exec rm -rf {} +
rm -rf "$dest/Lib/site-packages/bin"
echo "$dest: Python $version, ${libs[*]}"
