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
BrandingText "KodAct ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "KodAct"
VIAddVersionKey "FileDescription" "KodAct installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "KodAct"

!define MUI_ICON "..\assets\kodact.ico"
!define MUI_UNICON "..\assets\kodact.ico"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\KodAct.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start KodAct"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ------------------------------------------------------------------ install

Section "KodAct desktop app" SecApp
  SectionIn RO
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

Section "Design library" SecDesign
  SectionIn RO
  ; Open Design's skills, templates, design systems and craft rules, which
  ; KodAct's design chats read (design-library/, staged by build.sh with its
  ; media fetched). Replaced whole, so an update leaves no stale files; only
  ; a library KodAct put there (it has UPSTREAM.json) is ever removed.
  IfFileExists "$INSTDIR\design-library\UPSTREAM.json" 0 +2
    RMDir /r "$INSTDIR\design-library"
  SetOutPath "$INSTDIR\design-library"
  File /r "${DESIGN_DIR}\*.*"
SectionEnd

Section "KodAct CLI (kodact)" SecCli
  SectionIn RO
  SetOutPath "$INSTDIR\bin"
  File "/oname=kodact.exe" "${CLI_EXE}"
  ; Remote development: the app copies this to a remote the first time.
  SetOutPath "$INSTDIR\bin\linux-x86_64"
  File "/oname=kodact" "${LINUX_CLI}"
SectionEnd

Section "Add kodact to PATH" SecPath
  Push "$INSTDIR\bin"
  Call AddToPath
SectionEnd

Section "Desktop shortcut" SecDesktop
  SetShellVarContext current
  CreateShortcut "$DESKTOP\KodAct.lnk" "$INSTDIR\KodAct.exe" "" "$INSTDIR\KodAct.exe" 0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} "The KodAct desktop chat app."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesign} "Skills, templates and design systems for design chats."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCli} "KodAct CLI, the engine behind every chat. The app needs it."
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
  IfFileExists "$INSTDIR\design-library\UPSTREAM.json" 0 +2
    RMDir /r "$INSTDIR\design-library"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\KodAct"
SectionEnd
