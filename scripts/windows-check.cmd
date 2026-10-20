@echo off
rem Runs Marrow's safety checks on Windows, saves the results to
rem windows-check.log in the project folder, then starts Marrow.
rem Double-click it, or run:  scripts\windows-check.cmd
cd /d "%~dp0.."
set LOG=windows-check.log
echo Marrow Windows check, %DATE% %TIME% > %LOG%
echo Running the tests... (this takes a few minutes the first time)

echo. >> %LOG%
echo ===== npm install ===== >> %LOG%
call npm install --no-audit --no-fund >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== window tests ===== >> %LOG%
call npx tsc --noEmit >> %LOG% 2>&1
call npm test >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== cargo test --workspace ===== >> %LOG%
call cargo test --workspace >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== crash test ===== >> %LOG%
call cargo run --release --example crashtest -p marrow-core -- run 200 >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== speed (10,000 generated notes) ===== >> %LOG%
call cargo run --release --example bench -p marrow-core -- 10000 >> %LOG% 2>&1
echo exit code %ERRORLEVEL% >> %LOG%

echo. >> %LOG%
echo ===== DONE ===== >> %LOG%
echo Done. Results are in %CD%\%LOG%
echo Starting Marrow...
start "Marrow" cmd /k npm run tauri dev
