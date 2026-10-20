# Builds a portable Marrow: a folder (and a .zip of it) that runs on any
# Windows 10/11 PC without installing anything or needing admin rights.
#
#   npm run portable
#   npm run portable -- -WebView2Cab C:\Downloads\Microsoft.WebView2.FixedVersionRuntime.x64.cab
#
# Result: portable\Marrow\ and portable\Marrow-portable.zip. Running it again
# updates the program in portable\Marrow\ but never touches its Notes or Data.
#
#   Marrow\
#     Marrow.exe       the app
#     portable.txt     tells Marrow to keep its settings in Data\ (next to it)
#     Notes\           your vault; opens automatically the first time
#     Data\            created on first run: settings, search cache, window state
#     WebView2\        only if you pass -WebView2Cab (for PCs without WebView2)

param(
  # Microsoft's "Fixed Version" WebView2 runtime (.cab), for PCs that don't have WebView2.
  [string]$WebView2Cab = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host "Building Marrow (release)..." -ForegroundColor Cyan
npm install --no-audit --no-fund
if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "Build failed" }

# Build a fresh copy in a staging folder. Nothing outside it is deleted.
$stage = Join-Path $root "portable\.staging\Marrow"
if (Test-Path (Split-Path $stage)) { Remove-Item -Recurse -Force (Split-Path $stage) }
New-Item -ItemType Directory -Force -Path $stage, (Join-Path $stage "Notes") | Out-Null

Copy-Item (Join-Path $root "target\release\marrow-app.exe") (Join-Path $stage "Marrow.exe")
Set-Content -Encoding UTF8 (Join-Path $stage "portable.txt") @"
This file puts Marrow in portable mode: its settings, search cache and window
state live in the Data folder next to Marrow.exe, and nothing is written to
this computer's user folders. Delete this file to use the normal folders.
"@
Set-Content -Encoding UTF8 (Join-Path $stage "Notes\Welcome.md") @"
# Welcome

This is your portable vault. Everything here is plain Markdown, so the folder
can be copied anywhere, opened in Obsidian, or backed up as it is.

Type [[ to link a note, # to tag it, or / for blocks.
"@

if ($WebView2Cab) {
  Write-Host "Adding the WebView2 runtime..." -ForegroundColor Cyan
  $tmp = Join-Path $env:TEMP ("marrow-webview2-" + [guid]::NewGuid())
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  & expand.exe $WebView2Cab -F:* $tmp | Out-Null
  $exe = Get-ChildItem -Path $tmp -Recurse -Filter "msedgewebview2.exe" | Select-Object -First 1
  if (-not $exe) { throw "That .cab doesn't contain msedgewebview2.exe" }
  Copy-Item -Recurse $exe.Directory.FullName (Join-Path $stage "WebView2")
  Remove-Item -Recurse -Force $tmp
}

# The zip is made from the clean staging copy, so it never contains anyone's notes.
$zip = Join-Path $root "portable\Marrow-portable.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip

# Update portable\Marrow in place. Only the program files are replaced; the
# Notes and Data folders of a copy someone has been using are never touched.
$out = Join-Path $root "portable\Marrow"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item -Force (Join-Path $stage "Marrow.exe") (Join-Path $out "Marrow.exe")
Copy-Item -Force (Join-Path $stage "portable.txt") (Join-Path $out "portable.txt")
if (Test-Path (Join-Path $stage "WebView2")) {
  $wv = Join-Path $out "WebView2"
  if (Test-Path $wv) { Remove-Item -Recurse -Force $wv }
  Copy-Item -Recurse (Join-Path $stage "WebView2") $wv
}
if (-not (Test-Path (Join-Path $out "Notes"))) {
  Copy-Item -Recurse (Join-Path $stage "Notes") (Join-Path $out "Notes")
}
Remove-Item -Recurse -Force (Split-Path $stage)

$size = "{0:N1} MB" -f ((Get-Item $zip).Length / 1MB)
Write-Host ""
Write-Host "Done: $out" -ForegroundColor Green
Write-Host "Zip:  $zip ($size)" -ForegroundColor Green
Write-Host "Copy the Marrow folder (or unzip it) anywhere - a USB stick, Documents, OneDrive - and run Marrow.exe."
