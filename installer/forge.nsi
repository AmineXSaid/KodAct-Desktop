; The Forge installer for Windows: the desktop app and ForgeCLI.
;
;   makensis -DAPP_EXE=<ForgeDesktop.exe> -DCLI_EXE=<forge.exe> -DVERSION=0.1.0 \
;            -DOUT=<Forge-Setup.exe> installer/forge.nsi
;
; Built by installer/build.sh. A per-user install: no administrator rights,
; no UAC prompt. Lays out:
;   %LOCALAPPDATA%\Programs\Forge\Forge.exe        the desktop app
;   %LOCALAPPDATA%\Programs\Forge\bin\forge.exe    ForgeCLI (the app finds it in bin\)
; plus the user's Start menu and desktop shortcuts, an optional PATH entry for
; `forge` (the user's), and an uninstaller in Apps & features (the user's).

; A 64-bit installer: Forge runs on 64-bit Windows only.
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
  !define OUT "Forge-Setup-${VERSION}.exe"
!endif

!define NAME "Forge"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Forge"
!define ENV_KEY "Environment"

Name "${NAME}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\Forge"
InstallDirRegKey HKCU "Software\Forge" "InstallDir"
RequestExecutionLevel user
BrandingText "Forge ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Forge"
VIAddVersionKey "FileDescription" "Forge installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Forge"

!define MUI_ICON "..\assets\forge.ico"
!define MUI_UNICON "..\assets\forge.ico"
!define MUI_ABORTWARNING
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_FINISHPAGE_RUN "$INSTDIR\Forge.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start Forge"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ------------------------------------------------------------------ install

Section "Forge desktop app" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "/oname=Forge.exe" "${APP_EXE}"
  File "/oname=forge.ico" "..\assets\forge.ico"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Forge" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Forge"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "Forge"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\Forge.exe,0"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  SetShellVarContext current
  CreateShortcut "$SMPROGRAMS\Forge.lnk" "$INSTDIR\Forge.exe" "" "$INSTDIR\Forge.exe" 0
SectionEnd

Section "ForgeCLI (forge)" SecCli
  SectionIn RO
  SetOutPath "$INSTDIR\bin"
  File "/oname=forge.exe" "${CLI_EXE}"
SectionEnd

Section "Add forge to PATH" SecPath
  Push "$INSTDIR\bin"
  Call AddToPath
SectionEnd

Section "Desktop shortcut" SecDesktop
  SetShellVarContext current
  CreateShortcut "$DESKTOP\Forge.lnk" "$INSTDIR\Forge.exe" "" "$INSTDIR\Forge.exe" 0
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} "The Forge desktop chat app."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCli} "ForgeCLI, the engine behind every chat. The app needs it."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecPath} "Run forge from any terminal (your user's PATH)."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "A Forge icon on the desktop."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "Forge needs 64-bit Windows."
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
  WriteRegStr HKCU "Software\Forge" "AddedToPath" "$0"
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
  ReadRegStr $0 HKCU "Software\Forge" "AddedToPath"
  ${If} $0 != ""
    Push $0
    Call un.RemoveFromPath
  ${EndIf}

  SetShellVarContext current
  Delete "$SMPROGRAMS\Forge.lnk"
  Delete "$DESKTOP\Forge.lnk"

  Delete "$INSTDIR\bin\forge.exe"
  RMDir "$INSTDIR\bin"
  Delete "$INSTDIR\Forge.exe"
  Delete "$INSTDIR\forge.ico"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Forge"
SectionEnd
