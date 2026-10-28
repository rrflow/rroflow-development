param([Parameter(Mandatory=$true)][string]$Manifest)
$ErrorActionPreference = 'Stop'
try {
    $install = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
    $binary = Join-Path (Split-Path -Parent $Manifest) 'rroflow.exe'
    if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $install.sha256) { throw 'Pinned collector changed.' }
    $binding = Join-Path $install.store 'source.json'
    if ((Get-FileHash -LiteralPath $binding -Algorithm SHA256).Hash.ToLowerInvariant() -ne $install.binding_sha256) { throw 'Source binding changed.' }
    & $binary sources managed-run --manifest $Manifest
    exit $LASTEXITCODE
} catch { Write-Error 'RROFLOW startup failed. Inspect sources service status and the local source journal.'; exit 1 }
