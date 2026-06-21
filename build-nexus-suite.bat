@echo off
setlocal enabledelayedexpansion

echo ============================================================
echo   🚀 AGILE UP - NEXUS SUITE BUILDER (v2026.4)
echo ============================================================
echo.

set DIST_DIR=dist_nexus_suite
if not exist %DIST_DIR% mkdir %DIST_DIR%

:: 1. Build Code Explorer CLI
echo [1/3] Building Code Explorer CLI...
cargo build --release -p code-explorer-cli
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to build Code Explorer CLI
    exit /b %ERRORLEVEL%
)
copy target\release\code-explorer.exe %DIST_DIR%\code-explorer.exe > nul
echo ✅ Code Explorer CLI ready in %DIST_DIR%\code-explorer.exe

:: 2. Build Code Explorer Desktop (Tauri)
echo.
echo [2/3] Building Code Explorer Desktop Application...
cd crates\code-explorer-desktop\ui
call npm install --silent
call npm run build
cd ..\..\..
cargo tauri build --project crates/code-explorer-desktop
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to build Code Explorer Desktop
    exit /b %ERRORLEVEL%
)
:: Move the installer/binary to dist
echo ✅ Code Explorer Desktop ready.

:: 3. Build NexusBrain (Tauri)
echo.
echo [3/3] Building NexusBrain (Knowledge IDE)...
cd nexus-brain
call npm install --silent
call npm run build
cd ..
cargo tauri build --project nexus-brain/src-tauri
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to build NexusBrain
    exit /b %ERRORLEVEL%
)
echo ✅ NexusBrain ready.

echo.
echo ============================================================
echo   🎉 NEXUS SUITE BUILT SUCCESSFULLY
echo   Target folder: %DIST_DIR%
echo ============================================================
pause
