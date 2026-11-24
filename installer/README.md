# Windows installer

`installer/build.sh` cross-compiles the desktop app, KodAct CLI and KodAct's
setup for Windows (MinGW, `x86_64-pc-windows-gnu`), stages the install folder,
and appends it to the setup (`pack.py`):
`target/installer/KodAct-Setup-<version>.exe`, a 64-bit per-user installer (no
administrator rights, no UAC prompt), with its `.sha256`.

The setup (`installer/setup`, the `kodact-setup` binary) is KodAct's own dark
window, in GPUI like the app: the mark on a burgundy glow and the author's
handwritten signature (`setup/signature.png`, Pinyon Script, OFL), where to install,
four choices (LibreOffice, `kodact` on PATH, every nudge, a desktop shortcut),
then each part as it unpacks with one progress bar, then done or what went
wrong with Try again. What it installs is a zip (LZMA) after the .exe, then its
length and `KODACTPK` (`setup/payload.rs`); the .exe without it is
`Uninstall.exe`. It writes the same registry keys as the NSIS installer did, so
it updates an install that one made. Run without a payload (a development
build) it plays an install back; `KODACT_SETUP_SCREEN=welcome|working|done|failed|uninstall`
holds a screen, for screenshots. `kodact.nsi` is the previous installer, kept
for reference.

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

`KodAct-Setup.exe /S` installs silently (all options on);
`Uninstall.exe --uninstall /S` removes everything, including the PATH entry it
added. An entry that was
already on PATH stays.

Needs `rustup target add x86_64-pc-windows-gnu`, `gcc-mingw-w64-x86-64`,
`msitools` (LibreOffice) and Python 3.11+ (the design library's media is fetched once, then
cached). `KODACT_DIR` points at a KodAct CLI checkout (default `../kodact`; its `main`
has the mid-turn steering protocol the app uses); the script stops on a
checkout without it.
