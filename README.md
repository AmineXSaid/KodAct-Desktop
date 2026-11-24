# KodAct for Windows: installer download

GitHub takes files up to 100 MB, so the installer is split into parts.

1. Download every `KodAct-Setup.exe.partNN`, `KodAct-Setup.exe.sha256` and `merge.bat` into one folder.
2. Double-click `merge.bat`. It joins the parts into `KodAct-Setup.exe`, checks the SHA-256, and offers to run it.

Or join them by hand: `copy /b KodAct-Setup.exe.part* KodAct-Setup.exe`.

The installer holds the KodAct desktop app, KodAct CLI (the engine), LibreOffice and a Python with the document libraries. To update only the engine later: Settings → About → Replace engine…, and pick the new `kodact.exe`.

## Engine only

`engine/kodact.exe` is KodAct CLI on its own (main, 96cac78). In the app: Settings → About → Replace engine…, then pick it.
