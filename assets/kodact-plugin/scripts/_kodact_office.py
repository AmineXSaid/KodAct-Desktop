#!/usr/bin/env python3
"""Shared Windows-safe LibreOffice / Microsoft Office & pypdfium2 rendering helpers for KodAct skills."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def suppress_windows_error_dialogs() -> None:
    """Prevent child processes (e.g. soffice.exe) from showing modal Windows crash popups (0xc0000142)."""
    if os.name == "nt":
        try:
            import ctypes

            # SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
            ctypes.windll.kernel32.SetErrorMode(0x0001 | 0x0002 | 0x8000)
        except Exception:
            pass


def _prefer_console_soffice(path_str: str) -> str:
    p = Path(path_str)
    if os.name == "nt" and p.name.lower() == "soffice.exe":
        com = p.with_name("soffice.com")
        if com.is_file():
            return str(com)
    return str(p)


def find_soffice() -> str:
    """Locate LibreOffice, honoring KODACT_SOFFICE and KodAct's per-user install folder first."""
    named = os.environ.get("KODACT_SOFFICE")
    if named and Path(named).is_file():
        return _prefer_console_soffice(named)

    local_app = os.environ.get("LOCALAPPDATA")
    bundled = (
        str(Path(local_app) / "Programs" / "KodAct" / "libreoffice" / "program" / "soffice.exe")
        if local_app
        else None
    )

    candidates = [
        bundled,
        shutil.which("soffice.com") if os.name == "nt" else None,
        shutil.which("soffice"),
        shutil.which("libreoffice"),
        r"C:\Program Files\LibreOffice\program\soffice.com",
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        "/usr/lib/libreoffice/program/soffice",
        "/opt/libreoffice/program/soffice",
    ]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return _prefer_console_soffice(candidate)
    raise FileNotFoundError(
        "LibreOffice (soffice) not found. Install LibreOffice or set KODACT_SOFFICE."
    )


def soffice_env_and_kwargs(soffice_bin: str) -> dict:
    """Build isolated environment & subprocess kwargs so soffice never hits DLL collisions on Windows."""
    suppress_windows_error_dialogs()
    env = os.environ.copy()
    env["SAL_DISABLESKIA"] = "1"
    env["SAL_DISABLE_OPENCL"] = "1"
    env["SAL_USE_VCLPLUGIN"] = "svp"

    kwargs: dict = {"env": env}
    prog_dir = Path(soffice_bin).resolve().parent
    if prog_dir.is_dir():
        kwargs["cwd"] = str(prog_dir)

    if os.name == "nt":
        sys_root = env.get("SystemRoot") or env.get("WINDIR") or r"C:\Windows"
        clean_dirs = [
            str(prog_dir),
            str(Path(sys_root) / "System32"),
            sys_root,
            str(Path(sys_root) / "System32" / "Wbem"),
        ]
        env["PATH"] = os.pathsep.join(clean_dirs)
        kwargs["creationflags"] = getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)

    return kwargs


def run_soffice(
    args: list[str], *, timeout: int = 120, check: bool = True
) -> subprocess.CompletedProcess[str]:
    soffice = find_soffice()
    kw = soffice_env_and_kwargs(soffice)
    return subprocess.run(
        [soffice, *args],
        check=check,
        capture_output=True,
        text=True,
        timeout=timeout,
        **kw,
    )


def _office_ps1_helper() -> Path | None:
    here = Path(__file__).resolve().parent / "office_to_pdf.ps1"
    return here if here.is_file() else None


def convert_office_to_pdf(input_path: Path, out_dir: Path, timeout: int = 120) -> Path:
    """Convert .docx / .pptx / .xlsx to PDF in out_dir via Microsoft Office (when configured) or LibreOffice."""
    out_dir.mkdir(parents=True, exist_ok=True)
    pdf_path = out_dir / f"{input_path.stem}.pdf"
    route = (os.environ.get("KODACT_OFFICE_RENDER") or "").strip().lower()

    helper = _office_ps1_helper()
    ps = shutil.which("powershell") or shutil.which("pwsh")
    if os.name == "nt" and helper and ps and route in ("office", "powerpoint", "word", "excel", ""):
        if route in ("office", "powerpoint", "word", "excel"):
            try_office = True
        else:
            probe = subprocess.run(
                [ps, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(helper), "-Probe"],
                capture_output=True,
                text=True,
                timeout=25,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000),
            )
            try_office = probe.returncode == 0 and bool(probe.stdout.strip())
        if try_office:
            r = subprocess.run(
                [
                    ps,
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(helper),
                    str(input_path.resolve()),
                    str(pdf_path.resolve()),
                ],
                capture_output=True,
                text=True,
                timeout=timeout,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000),
            )
            if r.returncode == 0 and pdf_path.is_file():
                return pdf_path
            if route in ("office", "powerpoint", "word", "excel"):
                try:
                    find_soffice()
                except FileNotFoundError:
                    raise RuntimeError(
                        f"Microsoft Office could not convert {input_path.name}: {r.stderr.strip() or r.returncode}"
                    )

    if route == "none":
        raise RuntimeError("Office rendering is disabled (KODACT_OFFICE_RENDER=none).")

    with tempfile.TemporaryDirectory() as lo_profile:
        profile_uri = Path(lo_profile).resolve().as_uri()
        run_soffice(
            [
                f"-env:UserInstallation={profile_uri}",
                "--headless",
                "--norestore",
                "--nolockcheck",
                "--convert-to",
                "pdf",
                "--outdir",
                str(out_dir.resolve()),
                str(input_path.resolve()),
            ],
            timeout=timeout,
            check=True,
        )

    if not pdf_path.is_file():
        raise RuntimeError(f"Expected PDF was not generated: {pdf_path}")
    return pdf_path


def render_pdf_to_pngs(
    pdf_path: Path, out_dir: Path, prefix: str = "page", dpi: int = 150
) -> list[Path]:
    """Render PDF pages to <prefix>-<N>.png using Poppler pdftoppm or pypdfium2 fallback."""
    out_dir.mkdir(parents=True, exist_ok=True)
    for stale in out_dir.glob(f"{prefix}-*.png"):
        stale.unlink(missing_ok=True)

    pdftoppm = shutil.which("pdftoppm")
    if pdftoppm:
        kw = {}
        if os.name == "nt":
            kw["creationflags"] = getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000)
        subprocess.run(
            [pdftoppm, "-png", "-r", str(dpi), str(pdf_path), str(out_dir / prefix)],
            check=True,
            capture_output=True,
            text=True,
            timeout=120,
            **kw,
        )
        pngs = sorted(out_dir.glob(f"{prefix}-*.png"))
        if pngs:
            return pngs

    try:
        import pypdfium2 as pdfium
    except ImportError as exc:
        raise FileNotFoundError(
            "Neither Poppler 'pdftoppm' nor Python 'pypdfium2' is available to render PNG previews."
        ) from exc

    doc = pdfium.PdfDocument(str(pdf_path))
    pngs: list[Path] = []
    try:
        total = len(doc)
        pad_w = len(str(max(1, total)))
        scale = dpi / 72.0
        for idx in range(total):
            page = doc[idx]
            try:
                bitmap = page.render(scale=scale)
                pil_img = bitmap.to_pil()
                out_file = out_dir / f"{prefix}-{idx + 1:0{pad_w}d}.png"
                pil_img.save(out_file)
                pngs.append(out_file)
            finally:
                page.close()
    finally:
        doc.close()

    if not pngs:
        raise RuntimeError("PDF contained 0 renderable pages.")
    return pngs


def _excel_recalc(src: Path, out: Path, timeout: int) -> bool:
    """Windows with Excel: Excel itself recalculates and saves (office_to_pdf.ps1 -Recalc). False
    when Excel isn't there or failed, so LibreOffice is tried next."""
    helper = _office_ps1_helper()
    ps = shutil.which("powershell") or shutil.which("pwsh")
    route = (os.environ.get("KODACT_OFFICE_RENDER") or "").strip().lower()
    if os.name != "nt" or not helper or not ps or route in ("libreoffice", "none"):
        return False
    r = subprocess.run(
        [ps, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(helper),
         str(src.resolve()), str(out.resolve()), "-Recalc"],
        capture_output=True, text=True, timeout=timeout,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0x08000000),
    )
    return r.returncode == 0 and out.is_file()


def recalc_xlsx(src: Path, out: Path, timeout: int = 120) -> Path:
    """Recalculate every formula of an .xlsx and write it, with its values, to `out` (which may be
    `src`). Excel on Windows when it is installed, else LibreOffice with a throwaway profile.

    openpyxl saves formulas without values, so a workbook it wrote shows empty cells in viewers and
    can't be checked for #DIV/0! until something computes it: this is that step."""
    src, out = Path(src), Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        work = tmp_path / "in.xlsx"
        shutil.copyfile(src, work)
        result = tmp_path / "recalculated.xlsx"
        if not _excel_recalc(work, result, timeout):
            profile = (tmp_path / "profile").resolve().as_uri()
            conv = tmp_path / "out"
            run_soffice(
                [f"-env:UserInstallation={profile}", "--headless", "--norestore", "--nolockcheck",
                 "--calc", "--convert-to", "xlsx:Calc MS Excel 2007 XML", "--outdir", str(conv.resolve()),
                 str(work.resolve())],
                timeout=timeout, check=True,
            )
            if not (conv / "in.xlsx").is_file():
                raise RuntimeError("LibreOffice wrote no recalculated workbook")
            shutil.move(str(conv / "in.xlsx"), result)
        shutil.copyfile(result, out)
    return out
