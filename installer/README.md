# Windows installer

`installer/build.sh` cross-compiles the desktop app and KodAct CLI for Windows
(MinGW, `x86_64-pc-windows-gnu`) and packs both with NSIS (`kodact.nsi`) into
`target/installer/KodAct-Setup-<version>.exe`, a 64-bit per-user installer
(no administrator rights, no UAC prompt):

| Installed | Where |
|---|---|
| The desktop app | `%LOCALAPPDATA%\Programs\KodAct\KodAct.exe` |
| KodAct CLI | `%LOCALAPPDATA%\Programs\KodAct\bin\kodact.exe` (the app finds it there) |
| Design library | `%LOCALAPPDATA%\Programs\KodAct\design-library\` (`design-library/`, its media fetched by `scripts/sync_open_design.py media`) |
| Document tools | `%LOCALAPPDATA%\Programs\KodAct\python\` (python.org's embeddable Python, pinned by hash, with the document libraries `src/doc_tools.rs` pins; staged by `installer/stage_python.sh`) |
| LibreOffice | `%LOCALAPPDATA%\Programs\KodAct\libreoffice\` (The Document Foundation's 26.8.3 MSI, pinned by hash, unpacked by `installer/stage_libreoffice.sh`, trimmed to English, French, Arabic, German and Spanish), so Word, PowerPoint and Excel files show as they print; `KODACT_NO_LIBREOFFICE=1` leaves it out |
| Shortcuts | Start menu; desktop (optional) |
| `kodact` on PATH | the user's PATH (optional, on by default) |
| Uninstaller | `Uninstall.exe`, listed in Apps & features |

`KodAct-Setup.exe /S` installs silently (all options on); `Uninstall.exe /S`
removes everything, including the PATH entry it added. An entry that was
already on PATH stays.

Needs `rustup target add x86_64-pc-windows-gnu`, `gcc-mingw-w64-x86-64`,
`nsis`, `msitools` (LibreOffice) and Python 3.11+ with Pillow (the installer's pictures) (the design library's media is fetched once, then
cached). `KODACT_DIR` points at a KodAct CLI checkout (default `../kodact`; its `main`
has the mid-turn steering protocol the app uses); the script stops on a
checkout without it.
