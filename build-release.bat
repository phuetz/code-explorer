@echo off
REM ──────────────────────────────────────────────────────────────────
REM  Code Explorer — Release Build Script (Windows)
REM
REM  Produces:
REM    1. code-explorer CLI binary        -> target\release\code-explorer.exe
REM    2. Code Explorer Desktop installer -> target\release\bundle\
REM
REM  Usage:
REM    build-release.bat              Build both CLI + Desktop
REM    build-release.bat cli          CLI only
REM    build-release.bat desktop      Desktop app only
REM ──────────────────────────────────────────────────────────────────
setlocal enabledelayedexpansion

set "ROOT=%~dp0"
set "UI_DIR=%ROOT%crates\code-explorer-desktop\ui"
set "DESKTOP_DIR=%ROOT%crates\code-explorer-desktop"

cd /d "%ROOT%"

set "MODE=%~1"
if "%MODE%"=="" set "MODE=all"

REM ─── Pre-flight checks ────────────────────────────────────────────

echo [INFO]  Checking prerequisites...

where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERR]   cargo not found. Install Rust: https://rustup.rs
    exit /b 1
)

if "%MODE%"=="all" goto :check_npm
if "%MODE%"=="desktop" goto :check_npm
goto :prereq_ok

:check_npm
where npm >nul 2>&1
if errorlevel 1 (
    echo [ERR]   npm not found. Install Node.js: https://nodejs.org
    exit /b 1
)
where cargo-tauri >nul 2>&1
if errorlevel 1 (
    echo [WARN]  cargo-tauri not found, installing...
    cargo install tauri-cli --locked
    if errorlevel 1 (
        echo [ERR]   Failed to install tauri-cli
        exit /b 1
    )
)

:prereq_ok
echo [OK]    Prerequisites OK
echo.

REM ─── Step 1: Build CLI ────────────────────────────────────────────

if "%MODE%"=="desktop" goto :skip_cli

echo [INFO]  Building CLI (release)...
cargo build --release -p code-explorer-cli
if errorlevel 1 (
    echo [ERR]   CLI build failed
    exit /b 1
)

for %%F in (target\release\code-explorer.exe) do set "CLI_SIZE=%%~zF"
set /a CLI_SIZE_MB=!CLI_SIZE! / 1048576
echo [OK]    CLI binary: target\release\code-explorer.exe (!CLI_SIZE_MB! MB)
echo.

:skip_cli

REM ─── Step 2: Build Desktop ────────────────────────────────────────

if "%MODE%"=="cli" goto :summary

REM 2a. Install frontend dependencies
echo [INFO]  Installing frontend dependencies...
pushd "%UI_DIR%"
call npm install
if not exist "node_modules" (
    echo [ERR]   npm install failed
    popd
    exit /b 1
)
echo [OK]    Frontend dependencies installed

REM 2b. Build frontend (we do it here so tauri can skip beforeBuildCommand)
echo [INFO]  Building frontend...
call npm run build
if errorlevel 1 (
    echo [ERR]   Frontend build failed
    popd
    exit /b 1
)
echo [OK]    Frontend built
popd

REM 2c. Build Tauri app (skip beforeBuildCommand since we already built frontend)
echo [INFO]  Building Desktop app (release)...
cd /d "%DESKTOP_DIR%"
cargo tauri build --config "{\"build\":{\"beforeBuildCommand\":\"\"}}"
if errorlevel 1 (
    echo [ERR]   Desktop build failed
    exit /b 1
)

cd /d "%ROOT%"
echo [OK]    Desktop build complete
echo.

REM List installer files
echo [INFO]  Installers generated:
if exist "target\release\bundle\msi" (
    for %%F in (target\release\bundle\msi\*.msi) do (
        set "F_SIZE=%%~zF"
        set /a F_SIZE_MB=!F_SIZE! / 1048576
        echo          %%F (!F_SIZE_MB! MB^)
    )
)
if exist "target\release\bundle\nsis" (
    for %%F in (target\release\bundle\nsis\*.exe) do (
        set "F_SIZE=%%~zF"
        set /a F_SIZE_MB=!F_SIZE! / 1048576
        echo          %%F (!F_SIZE_MB! MB^)
    )
)
echo.

REM ─── Summary ──────────────────────────────────────────────────────

:summary
echo ========================================
echo   Build complete!
echo ========================================
if not "%MODE%"=="desktop" (
    echo   CLI:     target\release\code-explorer.exe
)
if not "%MODE%"=="cli" (
    echo   Desktop: target\release\bundle\
)
echo.

endlocal
