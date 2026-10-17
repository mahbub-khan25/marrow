@echo off
rem Runs Marrow's safety checks on Windows, saves the results to
rem windows-check.log in the project folder, then starts Marrow.
rem Double-click it, or run:  scripts\windows-check.cmd
cd /d "%~dp0.."
set LOG=windows-check.log
echo Marrow Windows check, %DATE% %TIME% > %LOG%
echo Running the tests... (this takes a few minutes the first time)

echo. >> %LOG%
echo ===== cargo test -p marrow-core ===== >> %LOG%
call cargo test -p marrow-core >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== crash test ===== >> %LOG%
call cargo run --release --example crashtest -p marrow-core -- run 200 >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== DONE ===== >> %LOG%
echo Done. Results are in %CD%\%LOG%
echo Starting Marrow...
start "Marrow" cmd /k npm run tauri dev
