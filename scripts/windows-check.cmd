@echo off
setlocal
rem Runs every Marrow check on Windows and ends with a clear PASS or FAIL.
rem The details go to windows-check.log in the project folder. Marrow starts
rem afterwards only if everything passed.
rem Double-click it, or run:  scripts\windows-check.cmd
cd /d "%~dp0.."
set LOG=windows-check.log
set FAILED=
echo Marrow Windows check, %DATE% %TIME% > %LOG%
echo Running the checks (a few minutes the first time)...

call :step "Install packages" npm install --no-audit --no-fund
call :step "Type check" npx tsc --noEmit
call :step "Window tests" npm test
call :step "Engine and app tests" cargo test --workspace
call :step "Crash test" cargo run --release --example crashtest -p marrow-core -- run 200
call :step "Speed test" cargo run --release --example bench -p marrow-core -- 10000

echo. >> %LOG%
if defined FAILED (
  echo OVERALL: FAIL -%FAILED% >> %LOG%
  echo.
  echo ==================================================
  echo   FAIL:%FAILED%
  echo   Details: %CD%\%LOG%
  echo ==================================================
  pause
  exit /b 1
)
echo OVERALL: PASS >> %LOG%
echo.
echo ==================================================
echo   PASS - every check passed
echo ==================================================
echo Starting Marrow...
start "Marrow" cmd /k npm run tauri dev
exit /b 0

rem Runs one check, logs it, and remembers a failure.
:step
set NAME=%~1
shift
echo. >> %LOG%
echo ===== %NAME% ===== >> %LOG%
echo   %NAME%...
call %1 %2 %3 %4 %5 %6 %7 %8 %9 >> %LOG% 2>&1
if errorlevel 1 (
  echo ----- %NAME%: FAILED ----- >> %LOG%
  echo     FAILED
  set FAILED=%FAILED% [%NAME%]
) else (
  echo ----- %NAME%: passed ----- >> %LOG%
)
exit /b 0
