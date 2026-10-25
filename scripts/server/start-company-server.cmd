@echo off
rem Bus Company Online: start the main server (every company, the players and the server's
rem clock; docs/omsihub/BUS_COMPANY_ONLINE.md) with a double-click. It goes beside
rem omsi-hub-cli.exe in the Omsi-Hub folder.
rem The first time it asks for the server's folder (its company-server.cfg, companies and
rem players) and keeps it in company-server-folder.txt beside it: from then on it starts at
rem once. Another folder: delete that file, or start-company-server.cmd <folder>.
rem (A folder without a server in it is started only when you say so: an Enter by mistake
rem started a new, empty server, and the companies seemed gone - 25 October 2026.)
setlocal
cd /d "%~dp0"
title Bus Company Online - main server
set "CLI=%~dp0omsi-hub-cli.exe"
if not exist "%CLI%" set "CLI=%~dp0openomsi-launcher.exe"
if not exist "%CLI%" (
  echo omsi-hub-cli.exe is not beside this file: put the file in the Omsi-Hub folder.
  pause
  exit /b 1
)
set "KEPT=%~dp0company-server-folder.txt"
set "DIR=%~1"
if not defined DIR if exist "%KEPT%" set /p DIR=<"%KEPT%"
if defined DIR goto check
:ask
echo The server's folder holds its company-server.cfg, its companies and its players,
echo for example C:\Users\you\Desktop\BusCompanyServer
set "DIR="
set /p "DIR=Type or paste the folder: "
if not defined DIR goto ask
:check
rem (quotes pasted with the folder taken off)
set "DIR=%DIR:"=%"
if exist "%DIR%\company-server.cfg" goto run
echo.
echo There is no server in %DIR% yet (no company-server.cfg there).
choice /c YN /m "Start a new, empty server in it"
if errorlevel 2 goto ask
:run
> "%KEPT%" echo %DIR%
echo Bus Company Online's main server in %DIR%
echo (type help for its commands, quit to stop it)
echo.
"%CLI%" --company-server "%DIR%"
echo.
echo The main server has stopped.
pause
