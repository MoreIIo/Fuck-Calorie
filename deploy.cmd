@echo off
rem Lance scripts\deploy.ps1 sans avoir a modifier la politique d'execution de PowerShell.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\deploy.ps1" %*
exit /b %ERRORLEVEL%
