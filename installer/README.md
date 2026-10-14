# Windows installer

`installer/build.sh` cross-compiles the desktop app and ForgeCLI for Windows
(MinGW, `x86_64-pc-windows-gnu`) and packs both with NSIS (`forge.nsi`) into
`target/installer/Forge-Setup-<version>.exe`, a 64-bit installer:

| Installed | Where |
|---|---|
| The desktop app | `C:\Program Files\Forge\Forge.exe` |
| ForgeCLI | `C:\Program Files\Forge\bin\forge.exe` (the app finds it there) |
| Shortcuts | Start menu; desktop (optional) |
| `forge` on PATH | the user's PATH (optional, on by default) |
| Uninstaller | `Uninstall.exe`, listed in Apps & features |

`Forge-Setup.exe /S` installs silently (all options on); `Uninstall.exe /S`
removes everything, including the PATH entry it added. An entry that was
already on PATH stays.

Needs `rustup target add x86_64-pc-windows-gnu`, `gcc-mingw-w64-x86-64` and
`nsis`. `FORGECLI_DIR` points at a ForgeCLI checkout (default `../forgecli`).
