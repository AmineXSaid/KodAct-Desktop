# Turn an Office file into a PDF with Microsoft Office itself, through COM,
# with no window and no prompt: PowerPoint for .pptx/.ppt, Word for
# .docx/.doc/.rtf, Excel for .xlsx/.xls. Used by check_tool.py's render and by
# KodAct's side panel, so a deck is drawn as PowerPoint draws it.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File office_to_pdf.ps1 IN OUT.pdf
#   powershell -NoProfile -ExecutionPolicy Bypass -File office_to_pdf.ps1 -Probe
#
# -Probe prints the Office apps this computer has (powerpoint word excel) and
# exits 0, or prints nothing and exits 3. Exit codes: 0 done, 2 bad
# arguments, 3 the app is not installed, 4 Office failed (the reason is on
# stderr). The input is opened read-only and never changed.

param(
    [string]$In,
    [string]$Out,
    [switch]$Probe
)

$ErrorActionPreference = "Stop"

function Test-ComApp([string]$progId) {
    return [bool](Get-Item "Registry::HKEY_CLASSES_ROOT\$progId\CLSID" -ErrorAction SilentlyContinue)
}

if ($Probe) {
    $found = @()
    if (Test-ComApp "PowerPoint.Application") { $found += "powerpoint" }
    if (Test-ComApp "Word.Application") { $found += "word" }
    if (Test-ComApp "Excel.Application") { $found += "excel" }
    if ($found.Count -eq 0) { exit 3 }
    Write-Output ($found -join " ")
    exit 0
}

if (-not $In -or -not $Out) {
    [Console]::Error.WriteLine("usage: office_to_pdf.ps1 IN OUT.pdf | -Probe")
    exit 2
}

$In = (Resolve-Path -LiteralPath $In).Path
$Out = [System.IO.Path]::GetFullPath($Out)
$ext = [System.IO.Path]::GetExtension($In).ToLowerInvariant()

$app = $null
$doc = $null
try {
    switch -Regex ($ext) {
        '^\.(pptx|ppt|pptm|ppsx)$' {
            if (-not (Test-ComApp "PowerPoint.Application")) { exit 3 }
            $app = New-Object -ComObject PowerPoint.Application
            # Open(FileName, ReadOnly, Untitled, WithWindow): read-only, no window.
            $doc = $app.Presentations.Open($In, -1, 0, 0)
            $doc.SaveAs($Out, 32)  # ppSaveAsPDF
        }
        '^\.(docx|doc|docm|rtf)$' {
            if (-not (Test-ComApp "Word.Application")) { exit 3 }
            $app = New-Object -ComObject Word.Application
            $app.Visible = $false
            $app.DisplayAlerts = 0
            # Open(FileName, ConfirmConversions, ReadOnly, AddToRecentFiles)
            $doc = $app.Documents.Open($In, $false, $true, $false)
            $doc.ExportAsFixedFormat($Out, 17)  # wdExportFormatPDF
        }
        '^\.(xlsx|xls|xlsm)$' {
            if (-not (Test-ComApp "Excel.Application")) { exit 3 }
            $app = New-Object -ComObject Excel.Application
            $app.Visible = $false
            $app.DisplayAlerts = $false
            # Open(FileName, UpdateLinks, ReadOnly)
            $doc = $app.Workbooks.Open($In, 0, $true)
            $doc.ExportAsFixedFormat(0, $Out)  # xlTypePDF
        }
        default {
            [Console]::Error.WriteLine("not an Office file: $ext")
            exit 2
        }
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 4
} finally {
    if ($doc) {
        # Close without saving: Word and Excel would otherwise ask.
        try {
            if ($ext -match '^\.(docx|doc|docm|rtf)$') { $doc.Close(0) }
            elseif ($ext -match '^\.(xlsx|xls|xlsm)$') { $doc.Close($false) }
            else { $doc.Close() }
        } catch {}
        [void][Runtime.InteropServices.Marshal]::ReleaseComObject($doc)
    }
    if ($app) { try { $app.Quit() } catch {} ; [void][Runtime.InteropServices.Marshal]::ReleaseComObject($app) }
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
}

if (-not (Test-Path -LiteralPath $Out)) {
    [Console]::Error.WriteLine("Office wrote no PDF")
    exit 4
}
exit 0
