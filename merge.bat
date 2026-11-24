@echo off
rem Joins the parts back into KodAct-Setup.exe, checks it, and offers to run it.
setlocal
cd /d "%~dp0"
if not exist KodAct-Setup.exe.part01 (
  echo The parts aren't here. Put merge.bat in the folder with KodAct-Setup.exe.part01, .part02 ...
  pause & exit /b 1
)
echo Joining the parts...
copy /b KodAct-Setup.exe.part* KodAct-Setup.exe >nul || (echo Joining failed. & pause & exit /b 1)
echo Checking the file...
for /f "tokens=* delims=" %%h in ('certutil -hashfile KodAct-Setup.exe SHA256 ^| findstr /v ":"') do (set got=%%h & goto :have)
:have
set got=%got: =%
set /p want=<KodAct-Setup.exe.sha256
if /i not "%got%"=="%want%" (
  echo The joined file doesn't match its checksum: download the parts again.
  echo expected %want%
  echo got      %got%
  pause & exit /b 1
)
echo KodAct-Setup.exe is ready and checked.
choice /m "Run the installer now"
if errorlevel 2 exit /b 0
start "" KodAct-Setup.exe
