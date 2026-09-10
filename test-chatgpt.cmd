@echo off
powershell -NoProfile -ExecutionPolicy Bypass -NoExit -File "%~dp0scripts\code-explorer.ps1" test-chatgpt %*
