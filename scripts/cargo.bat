@echo off
rem Run cargo with the MSVC environment.
rem
rem Visual Studio 2026 is installed in a non-standard path (D:\raznoe\VS2022),
rem so the MSVC environment is prepared here through vcvars64.bat instead of
rem relying on vswhere.
rem
rem Keep this file pure ASCII with CRLF line endings. Cyrillic text in a .bat
rem breaks cmd parsing, so all comments here are in English.
setlocal
set "VCVARS=D:\raznoe\VS2022\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
  echo [cargo.bat] vcvars64.bat not found: %VCVARS%
  exit /b 1
)
call "%VCVARS%" 1>nul 2>nul
if errorlevel 1 (
  echo [cargo.bat] vcvars64.bat failed
  exit /b 1
)
rem Compiler UI language. cl.exe 14.51 falls with D8050 on sqlite3.c when it
rem prints Russian diagnostics; English messages avoid that path.
set "VSLANG=1033"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cargo %*
exit /b %errorlevel%