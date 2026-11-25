; The KodAct installer for Windows: the desktop app and KodAct CLI.
;
;   makensis -DAPP_EXE=<KodAct.exe> -DCLI_EXE=<kodact.exe> -DVERSION=0.1.0 \
;            -DOUT=<KodAct-Setup.exe> installer/kodact.nsi
;
; Built by installer/build.sh. A per-user install: no administrator rights,
; no UAC prompt. Lays out:
;   %LOCALAPPDATA%\Programs\KodAct\KodAct.exe        the desktop app
;   %LOCALAPPDATA%\Programs\KodAct\bin\kodact.exe    KodAct CLI (the app finds it in bin\)
;   %LOCALAPPDATA%\Programs\KodAct\bin\linux-x86_64\kodact  KodAct CLI for Linux, put on
;                                     WSL distros, SSH hosts and containers
;   %LOCALAPPDATA%\Programs\KodAct\design-library\  the design library
;   %LOCALAPPDATA%\Programs\KodAct\python\          Python with the document libraries
;   %LOCALAPPDATA%\Programs\KodAct\libreoffice\     LibreOffice, so Office files show as they print
; plus the user's Start menu and desktop shortcuts, an optional PATH entry for
; `kodact` (the user's), and an uninstaller in Apps & features (the user's).

; A 64-bit installer: KodAct runs on 64-bit Windows only.
Target amd64-unicode
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"
!include "x64.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef OUT
  !define OUT "KodAct-Setup-${VERSION}.exe"
!endif

!define NAME "KodAct"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\KodAct"
!define ENV_KEY "Environment"

Name "${NAME}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\KodAct"
InstallDirRegKey HKCU "Software\KodAct" "InstallDir"
RequestExecutionLevel user
ManifestDPIAware true
; Windows' own UI face, so the pages read like the app.
SetFont "Segoe UI" 9
ShowInstDetails nevershow
ShowUninstDetails nevershow
BrandingText "KodAct ${VERSION}  ·  From code to impact"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "KodAct"
VIAddVersionKey "FileDescription" "KodAct installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "KodAct"

!define MUI_ICON "..\assets\kodact.ico"
!define MUI_UNICON "..\assets\kodact.ico"
!define MUI_ABORTWARNING
!define MUI_ABORTWARNING_TEXT "Stop installing KodAct? Nothing is left half installed."
!define MUI_COMPONENTSPAGE_SMALLDESC

; KodAct's look: the burgundy side panel on the welcome and finish pages,
; the icon in the white header, a flat burgundy progress bar, and one status
; line in plain words instead of a list of files. The pictures are drawn by
; scripts/make_installer_art.py at twice their size, for high-DPI screens.
!ifdef ART_DIR
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${ART_DIR}\welcome.bmp"
  !define MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH AspectFitHeight
  !define MUI_UNWELCOMEFINISHPAGE_BITMAP "${ART_DIR}\unwelcome.bmp"
  !define MUI_UNWELCOMEFINISHPAGE_BITMAP_STRETCH AspectFitHeight
  !define MUI_HEADERIMAGE
  !define MUI_HEADERIMAGE_RIGHT
  !define MUI_HEADERIMAGE_BITMAP "${ART_DIR}\header.bmp"
  !define MUI_HEADERIMAGE_BITMAP_STRETCH AspectFitHeight
  !define MUI_HEADERIMAGE_UNBITMAP "${ART_DIR}\header.bmp"
  !define MUI_HEADERIMAGE_UNBITMAP_STRETCH AspectFitHeight
!endif
!define MUI_BGCOLOR "FFFFFF"
!define MUI_TEXTCOLOR "1A1816"
!define MUI_INSTFILESPAGE_PROGRESSBAR "colored"
!define MUI_INSTFILESPAGE_COLORS "7A1F3D FFFFFF"

!define MUI_WELCOMEPAGE_TITLE "Welcome to KodAct"
!define MUI_WELCOMEPAGE_TITLE_3LINES
!define MUI_WELCOMEPAGE_TEXT "KodAct turns what you ask into working code and finished documents, and asks before it changes anything.$\r$\n$\r$\nThis installs the KodAct desktop app and KodAct CLI for you alone: no administrator rights needed. It also brings everything KodAct works with: Python with the document libraries, and LibreOffice, so Word, Excel and PowerPoint files show exactly as they print.$\r$\n$\r$\nClick Next to continue."
!define MUI_COMPONENTSPAGE_TEXT_TOP "Everything is ticked for the full experience. Untick what you would rather not have."
!define MUI_INSTFILESPAGE_FINISHHEADER_TEXT "KodAct is installed"
!define MUI_INSTFILESPAGE_FINISHHEADER_SUBTEXT "Everything is in place."
!define MUI_INSTFILESPAGE_ABORTHEADER_TEXT "Installation stopped"
!define MUI_INSTFILESPAGE_ABORTHEADER_SUBTEXT "KodAct was not installed."
!define MUI_FINISHPAGE_TITLE "KodAct is ready"
!define MUI_FINISHPAGE_TITLE_3LINES
!define MUI_FINISHPAGE_TEXT "Open KodAct from the Start menu, or type kodact in any terminal.$\r$\n$\r$\nFrom code to impact."
!define MUI_FINISHPAGE_RUN "$INSTDIR\KodAct.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start KodAct now"
!define MUI_FINISHPAGE_NOREBOOTSUPPORT

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"


; One status line for a step, its file list hidden.
!macro Step TEXT
  SetDetailsPrint textonly
  DetailPrint "${TEXT}"
  SetDetailsPrint none
!macroend

; ------------------------------------------------------------------ install

Section "KodAct desktop app" SecApp
  SectionIn RO
  !insertmacro Step "Installing the KodAct desktop app…"
  SetOutPath "$INSTDIR"
  File "/oname=KodAct.exe" "${APP_EXE}"
  File "/oname=kodact.ico" "..\assets\kodact.ico"
!ifdef FONTS_DIR
  ; A personal build's fonts (installer/private-fonts), read at start.
  SetOutPath "$INSTDIR\fonts"
  File "${FONTS_DIR}/*.*"
!endif

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\KodAct" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "KodAct"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "KodAct"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\KodAct.exe,0"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  SetShellVarContext current
  CreateShortcut "$SMPROGRAMS\KodAct.lnk" "$INSTDIR\KodAct.exe" "" "$INSTDIR\KodAct.exe" 0
SectionEnd

Section "Document tools" SecPython
  SectionIn RO
  ; Python with the libraries KodAct edits Word, Excel, PowerPoint and PDF
  ; files with (staged by stage_python.sh), so nothing has to be installed
  ; by hand. Replaced whole: it is KodAct's own, and an update's libraries
  ; must not mix with the last one's.
  !insertmacro Step "Setting up Python and the document libraries…"
  RMDir /r "$INSTDIR\python"
  SetOutPath "$INSTDIR\python"
  File /r "${PYTHON_DIR}\*.*"
SectionEnd

Section "Design library" SecDesign
  SectionIn RO
  ; Open Design's skills, templates, design systems and craft rules, which
  ; KodAct's design chats read (design-library/, staged by build.sh with its
  ; media fetched). Replaced whole, so an update leaves no stale files; only
  ; a library KodAct put there (it has UPSTREAM.json) is ever removed.
  !insertmacro Step "Unpacking the design library…"
  IfFileExists "$INSTDIR\design-library\UPSTREAM.json" 0 +2
    RMDir /r "$INSTDIR\design-library"
  SetOutPath "$INSTDIR\design-library"
  File /r "${DESIGN_DIR}\*.*"
SectionEnd

Section "KodAct CLI (kodact)" SecCli
  SectionIn RO
  !insertmacro Step "Installing KodAct CLI…"
  SetOutPath "$INSTDIR\bin"
  File "/oname=kodact.exe" "${CLI_EXE}"
  ; Remote development: the app copies this to a remote the first time.
  SetOutPath "$INSTDIR\bin\linux-x86_64"
  File "/oname=kodact" "${LINUX_CLI}"
SectionEnd

!ifdef LIBREOFFICE_DIR
Section "LibreOffice (documents as they print)" SecOffice
  ; LibreOffice, unpacked beside the app (staged by stage_libreoffice.sh):
  ; KodAct draws Word, PowerPoint and Excel files through it as they print,
  ; exports them to PDF and recalculates workbooks. KodAct finds it in
  ; libreoffice\program\soffice.exe; nothing is registered with Windows, and
  ; a LibreOffice installed separately is left alone. Replaced whole.
  !insertmacro Step "Unpacking LibreOffice for documents as they print. This takes a minute…"
  RMDir /r "$INSTDIR\libreoffice"
  SetOutPath "$INSTDIR\libreoffice"
  File /r "${LIBREOFFICE_DIR}\*.*"
SectionEnd
!endif

Section "Add kodact to PATH" SecPath
  !insertmacro Step "Adding kodact to your PATH…"
  Push "$INSTDIR\bin"
  Call AddToPath
SectionEnd

Section "Turn on KodAct nudges" SecNudges
  ; KODACT_NUDGES in the user's environment: the 10 fact-based nudges
  ; (shellEdit, constraints, namedChecks, passingSnapshots, limitVerdicts,
  ; staleDeliverables, thinkingTimeBox, todoTracking, clock, unpinnedInstalls),
  ; excluding adaptiveEffort so Auto effort never sends unrequested reasoning_effort.
  ReadRegStr $0 HKCU "${ENV_KEY}" "KODACT_NUDGES"
  ReadRegStr $1 HKCU "Software\KodAct" "SetNudges"
  ${If} $0 == ""
  ${OrIf} $0 == "all"
    ${If} $0 == ""
    ${OrIf} $1 == "1"
      WriteRegStr HKCU "${ENV_KEY}" "KODACT_NUDGES" "shellEdit,constraints,namedChecks,passingSnapshots,limitVerdicts,staleDeliverables,thinkingTimeBox,todoTracking,clock,unpinnedInstalls"
      WriteRegStr HKCU "Software\KodAct" "SetNudges" "1"
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000
    ${Else}
      DetailPrint "KODACT_NUDGES is already set to $0; left as it is."
    ${EndIf}
  ${Else}
    DetailPrint "KODACT_NUDGES is already set to $0; left as it is."
  ${EndIf}
SectionEnd

Section "Desktop shortcut" SecDesktop
  !insertmacro Step "Adding the desktop shortcut…"
  SetShellVarContext current
  CreateShortcut "$DESKTOP\KodAct.lnk" "$INSTDIR\KodAct.exe" "" "$INSTDIR\KodAct.exe" 0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} "The KodAct desktop chat app."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecPython} "Python with the libraries KodAct edits Word, Excel, PowerPoint and PDF files with."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesign} "Skills, templates and design systems for design chats."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCli} "KodAct CLI, the engine behind every chat. The app needs it."
!ifdef LIBREOFFICE_DIR
  !insertmacro MUI_DESCRIPTION_TEXT ${SecOffice} "Shows Word, Excel and PowerPoint files exactly as they print, exports them to PDF, recalculates workbooks. About 860 MB."
!endif
  !insertmacro MUI_DESCRIPTION_TEXT ${SecNudges} "Fact-based KodAct CLI nudges on in your user's environment."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecPath} "Run kodact from any terminal (your user's PATH)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "A KodAct icon on the desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "KodAct needs 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
FunctionEnd

Function un.onInit
  SetRegView 64
FunctionEnd

; ------------------------------------------------------------------ PATH
; The user's PATH (HKCU\Environment), edited as a whole string. NSIS strings
; hold NSIS_MAX_STRLEN characters, so a PATH near that length is left alone
; rather than truncated.

Function AddToPath
  Exch $0 ; the directory
  Push $1
  Push $2
  ReadRegStr $1 HKCU "${ENV_KEY}" "Path"
  StrLen $2 $1
  IntOp $2 $2 + 300
  ${If} $2 >= ${NSIS_MAX_STRLEN}
    DetailPrint "PATH is too long to edit safely; add $0 yourself."
    Goto done
  ${EndIf}
  ; Already there?
  Push "$1;"
  Push "$0;"
  Call StrContains
  Pop $2
  ${If} $2 == 1
    Goto done
  ${EndIf}
  ${If} $1 == ""
    StrCpy $1 "$0"
  ${Else}
    StrCpy $2 $1 1 -1
    ${If} $2 == ";"
      StrCpy $1 "$1$0"
    ${Else}
      StrCpy $1 "$1;$0"
    ${EndIf}
  ${EndIf}
  WriteRegExpandStr HKCU "${ENV_KEY}" "Path" $1
  WriteRegStr HKCU "Software\KodAct" "AddedToPath" "$0"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000
  done:
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; Push haystack, push needle; pops 1 when the needle is in the haystack.
Function StrContains
  Exch $1 ; needle
  Exch
  Exch $0 ; haystack
  Push $2
  Push $3
  Push $4
  StrLen $3 $1
  StrCpy $4 0
  loop:
    StrCpy $2 $0 $3 $4
    ${If} $2 == ""
      StrCpy $0 0
      Goto out
    ${EndIf}
    ${If} $2 == $1
      StrCpy $0 1
      Goto out
    ${EndIf}
    IntOp $4 $4 + 1
    Goto loop
  out:
  Pop $4
  Pop $3
  Pop $2
  ; The stack holds the caller's $1, then $0 on top.
  Exch $0
  Exch
  Pop $1
FunctionEnd

; Removes every ";dir" (or a leading "dir;", or the whole value "dir").
Function un.RemoveFromPath
  Exch $0 ; the directory
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  ReadRegStr $1 HKCU "${ENV_KEY}" "Path"
  StrCpy $1 ";$1;"
  StrCpy $5 ";$0;"
  StrLen $3 $5
  StrCpy $4 0
  loop:
    StrCpy $2 $1 $3 $4
    ${If} $2 == ""
      Goto finish
    ${EndIf}
    ${If} $2 == $5
      StrCpy $2 ""             ; before (StrCpy with length 0 copies all)
      ${If} $4 > 0
        StrCpy $2 $1 $4
      ${EndIf}
      IntOp $4 $4 + $3
      StrCpy $1 $1 "" $4       ; after
      StrCpy $1 "$2;$1"
      StrCpy $4 0
      Goto loop
    ${EndIf}
    IntOp $4 $4 + 1
    Goto loop
  finish:
  ; Drop the guards added above.
  StrCpy $1 $1 "" 1
  StrCpy $1 $1 -1
  WriteRegExpandStr HKCU "${ENV_KEY}" "Path" $1
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

; ------------------------------------------------------------------ uninstall

Section "Uninstall"
  ReadRegStr $0 HKCU "Software\KodAct" "AddedToPath"
  ${If} $0 != ""
    Push $0
    Call un.RemoveFromPath
  ${EndIf}

  ReadRegStr $0 HKCU "Software\KodAct" "SetNudges"
  ${If} $0 == "1"
    DeleteRegValue HKCU "${ENV_KEY}" "KODACT_NUDGES"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=2000
  ${EndIf}

  SetShellVarContext current
  Delete "$SMPROGRAMS\KodAct.lnk"
  Delete "$DESKTOP\KodAct.lnk"

  Delete "$INSTDIR\bin\linux-x86_64\kodact"
  RMDir "$INSTDIR\bin\linux-x86_64"
  Delete "$INSTDIR\bin\kodact.exe"
  RMDir "$INSTDIR\bin"
  Delete "$INSTDIR\KodAct.exe"
  Delete "$INSTDIR\kodact.ico"
  RMDir /r "$INSTDIR\fonts"
  RMDir /r "$INSTDIR\python"
  RMDir /r "$INSTDIR\libreoffice"
  IfFileExists "$INSTDIR\design-library\UPSTREAM.json" 0 +2
    RMDir /r "$INSTDIR\design-library"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\KodAct"
SectionEnd
