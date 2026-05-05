# Cerberus AI — one-liner bootstrapper
# Usage (PowerShell / Windows Terminal):
#   irm https://cerberusai.dev/get | iex
#
# Or from cmd.exe (Win10+, curl is built-in):
#   curl -sSL https://cerberusai.dev/get -o "%TEMP%\get-cerberus.ps1" && powershell -ep bypass -File "%TEMP%\get-cerberus.ps1"

$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"

Write-Host ""
Write-Host "  CERBERUS AI" -ForegroundColor Red
Write-Host "  Bootstrapping installer..." -ForegroundColor DarkGray
Write-Host ""

$api   = "https://api.github.com/repos/tjcrims0nx/CerberusAI-Desktop/releases/latest"
$tmp   = $env:TEMP

try {
    $rel   = Invoke-RestMethod -Uri $api -Headers @{"User-Agent"="CerberusBootstrap"} -TimeoutSec 15
    $asset = $rel.assets | Where-Object { $_.name -match "(?i)(cerberus.*setup\.exe|cerberus.*-windows.*\.exe)$" } |
             Select-Object -First 1

    if (-not $asset) {
        # Fallback: any .exe in the release
        $asset = $rel.assets | Where-Object { $_.name -match "\.exe$" } | Select-Object -First 1
    }

    if (-not $asset) {
        throw "No installer found in release $($rel.tag_name). Visit https://github.com/tjcrims0nx/CerberusAI-Desktop/releases"
    }

    $dest = Join-Path $tmp $asset.name
    Write-Host "  Downloading $($asset.name) from $($rel.tag_name)..." -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -UseBasicParsing

    Write-Host "  Running installer..." -ForegroundColor DarkGray
    Start-Process -FilePath $dest -Wait

    Write-Host ""
    Write-Host "  Done. Launch Cerberus from the Start Menu." -ForegroundColor Green
    Write-Host "  Get your API key at https://access.cerberusai.dev" -ForegroundColor DarkGray
    Write-Host ""
    Start-Sleep -Seconds 2
    exit 0

} catch {
    Write-Host "  ERROR: $_" -ForegroundColor Red
    Write-Host "  Manual download: https://github.com/tjcrims0nx/CerberusAI-Desktop/releases" -ForegroundColor Yellow
    exit 1
}
