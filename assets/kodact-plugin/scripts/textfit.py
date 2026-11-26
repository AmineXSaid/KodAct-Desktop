"""How much room text needs, measured with the font it will be shown in.

    from textfit import lines, height
    lines("Revenue grew 23%", "Segoe UI", 18, width_in=4.0, bold=True)    # → 1
    height(text, "Georgia", 20, width_in=6.0)                              # inches, line spacing 1.18

The fonts are looked up where the operating system keeps them (Windows, macOS, Linux, and the
person's own font folders). A font that isn't installed is measured with a metric-compatible one
when there is one (Calibri ↔ Carlito, Cambria ↔ Caladea, Arial ↔ Liberation Sans, Times New Roman ↔
Liberation Serif, Courier New ↔ Liberation Mono, Georgia ↔ Gelasio); else with an estimate of its
average letter width. `source(family)` says which was used.

Lines wrap the way Office wraps them: at spaces, a word longer than the line broken where it has to
be, each paragraph ("\\n") starting a new line.
"""

from __future__ import annotations

import os
import sys
from functools import lru_cache
from pathlib import Path

# Family → its files: (regular, bold), names as the font packages ship them (case is ignored).
FILES = {
    "segoe ui": ("segoeui.ttf", "segoeuib.ttf"),
    "segoe ui semibold": ("seguisb.ttf", "segoeuib.ttf"),
    "segoe ui black": ("seguibl.ttf", "seguibl.ttf"),
    "segoe ui light": ("segoeuil.ttf", "segoeuib.ttf"),
    "calibri": ("calibri.ttf", "calibrib.ttf"),
    "cambria": ("cambria.ttc", "cambriab.ttf"),
    "arial": ("arial.ttf", "arialbd.ttf"),
    "times new roman": ("times.ttf", "timesbd.ttf"),
    "georgia": ("georgia.ttf", "georgiab.ttf"),
    "century gothic": ("gothic.ttf", "gothicb.ttf"),
    "trebuchet ms": ("trebuc.ttf", "trebucbd.ttf"),
    "verdana": ("verdana.ttf", "verdanab.ttf"),
    "tahoma": ("tahoma.ttf", "tahomabd.ttf"),
    "consolas": ("consola.ttf", "consolab.ttf"),
    "courier new": ("cour.ttf", "courbd.ttf"),
    "aptos": ("aptos.ttf", "aptos-bold.ttf"),
    "inter": ("inter-regular.ttf", "inter-bold.ttf"),
}
# Fonts drawn to the same widths, for machines without the original.
METRIC_TWINS = {
    "calibri": ("carlito-regular.ttf", "carlito-bold.ttf"),
    "cambria": ("caladea-regular.ttf", "caladea-bold.ttf"),
    "arial": ("liberationsans-regular.ttf", "liberationsans-bold.ttf"),
    "times new roman": ("liberationserif-regular.ttf", "liberationserif-bold.ttf"),
    "courier new": ("liberationmono-regular.ttf", "liberationmono-bold.ttf"),
    "consolas": ("liberationmono-regular.ttf", "liberationmono-bold.ttf"),
    "georgia": ("gelasio-regular.ttf", "gelasio-bold.ttf"),
}
# Average advance of a letter, in ems, when no file is at hand (wider faces get more).
ESTIMATE = {"century gothic": 0.56, "verdana": 0.58, "segoe ui black": 0.6, "georgia": 0.53,
            "trebuchet ms": 0.5, "consolas": 0.55, "courier new": 0.6}
DEFAULT_EM = 0.52
SCALE = 10  # measure at 10x the point size: integer rounding stays below 1%


def _font_dirs() -> list[Path]:
    home = Path.home()
    dirs = []
    if os.name == "nt":
        win = Path(os.environ.get("WINDIR", r"C:\Windows"))
        dirs += [win / "Fonts"]
        local = os.environ.get("LOCALAPPDATA")
        if local:
            dirs.append(Path(local) / "Microsoft" / "Windows" / "Fonts")
    elif sys.platform == "darwin":
        dirs += [Path("/System/Library/Fonts"), Path("/System/Library/Fonts/Supplemental"),
                 Path("/Library/Fonts"), home / "Library" / "Fonts",
                 # Office for Mac keeps its own copies of the Windows fonts.
                 Path("/Applications/Microsoft PowerPoint.app/Contents/Resources/DFonts")]
    else:
        dirs += [Path("/usr/share/fonts"), Path("/usr/local/share/fonts"), home / ".fonts",
                 home / ".local" / "share" / "fonts"]
    return [d for d in dirs if d.is_dir()]


@lru_cache(maxsize=1)
def _index() -> dict[str, str]:
    """Every font file the machine has: lower-case file name → path."""
    found: dict[str, str] = {}
    for d in _font_dirs():
        for root, _, names in os.walk(d):
            for n in names:
                if n.lower().endswith((".ttf", ".otf", ".ttc")):
                    found.setdefault(n.lower(), os.path.join(root, n))
    return found


def _file(family: str, bold: bool) -> tuple[str | None, str]:
    fam = (family or "").strip().lower()
    idx = _index()
    for table, how in ((FILES, "the font itself"), (METRIC_TWINS, "a metric-compatible font")):
        names = table.get(fam)
        if names:
            path = idx.get(names[1 if bold else 0]) or idx.get(names[0])
            if path:
                return path, how
    return None, "an estimate"


@lru_cache(maxsize=256)
def _font(family: str, pt: float, bold: bool):
    path, _ = _file(family, bold)
    if path is None:
        return None
    try:
        from PIL import ImageFont

        return ImageFont.truetype(path, size=max(1, int(round(pt * SCALE))))
    except (ImportError, OSError):
        return None


def source(family: str, bold: bool = False) -> str:
    """How `family` is measured here: "the font itself", "a metric-compatible font" or "an estimate"."""
    path, how = _file(family, bold)
    return how if path and _font(family, 12, bold) is not None else "an estimate"


def _width_pt(text: str, family: str, pt: float, bold: bool) -> float:
    """The text's advance width in points."""
    font = _font(family, pt, bold)
    if font is None:
        em = ESTIMATE.get((family or "").strip().lower(), DEFAULT_EM)
        return len(text) * pt * em * (1.06 if bold else 1.0)
    return font.getlength(text) / SCALE


def lines(text: str, family: str, pt: float, width_in: float, bold: bool = False) -> int:
    """How many lines `text` takes in a box `width_in` inches wide."""
    room = width_in * 72
    if room <= 0:
        return 10**6
    space = _width_pt(" ", family, pt, bold)
    total = 0
    for para in str(text).split("\n"):
        words = para.split()
        if not words:
            total += 1
            continue
        n, cur = 1, 0.0
        for word in words:
            w = _width_pt(word, family, pt, bold)
            if w > room:  # a word longer than the line breaks where it must
                if cur:
                    n += 1
                n += int(w // room)
                cur = w % room
                continue
            add = w + (space if cur else 0.0)
            if cur and cur + add > room:
                n += 1
                cur = w
            else:
                cur += add
        total += n
    return total


def height(text: str, family: str, pt: float, width_in: float, bold: bool = False, spacing: float = 1.18) -> float:
    """The height in inches `text` needs at `pt`, wrapped to `width_in`."""
    return lines(text, family, pt, width_in, bold) * pt * spacing / 72


if __name__ == "__main__":
    for fam in sys.argv[1:] or list(FILES):
        print(f"{fam}: {source(fam)}")
