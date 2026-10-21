# Sign one file with Azure Artifact Signing. Tauri calls this for every Windows binary it builds
# (the app .exe, then the NSIS and MSI installers) once the release workflow has set up signing.
# Needs: SIGNTOOL_PATH, ARTIFACT_DLIB_PATH, ARTIFACT_METADATA_PATH (set by release.yml) and an
# Azure sign-in (azure/login) that has the "Artifact Signing Certificate Profile Signer" role.
param([Parameter(Mandatory = $true)][string]$File)
$ErrorActionPreference = 'Stop'
& $env:SIGNTOOL_PATH sign /v /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 `
  /dlib $env:ARTIFACT_DLIB_PATH /dmdf $env:ARTIFACT_METADATA_PATH $File
if ($LASTEXITCODE -ne 0) { Write-Error "signing failed: $File"; exit $LASTEXITCODE }
