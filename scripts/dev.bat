@echo off
rem Run the app in development mode with the MSVC environment.
rem Pure ASCII, CRLF line endings: Cyrillic in a .bat breaks cmd parsing.
setlocal
set "VCVARS=D:\raznoe\VS2022\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
  echo [dev.bat] vcvars64.bat not found: %VCVARS%
  exit /b 1
)
call "%VCVARS%" 1>nul 2>nul
if errorlevel 1 (
  echo [dev.bat] vcvars64.bat failed
  exit /b 1
)
set "VSLANG=1033"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call npx tauri dev
exit /b %errorlevel%