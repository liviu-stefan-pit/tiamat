@echo off
REM Windows wrapper for the deterministic fake Cursor CLI.
setlocal EnableExtensions
set "SCRIPT_DIR=%~dp0"
node "%SCRIPT_DIR%fake-agent.mjs" %*
exit /b %ERRORLEVEL%
