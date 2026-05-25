# Cerberus AI — one-liner bootstrapper (BETA)
# Usage (PowerShell / Windows Terminal):
#   irm https://cerberusai.dev/get-beta | iex
#
# Or from cmd.exe (Win10+, curl is built-in):
#   curl -sSL https://cerberusai.dev/get-beta -o "%TEMP%\get-cerberus-beta.ps1" && powershell -ep bypass -File "%TEMP%\get-cerberus-beta.ps1"
#
# This bootstrapper hands off to deploy/install.ps1 in the newest GitHub
# prerelease. That script handles Ollama, WebView2, and the Cerberus app itself,
# so a fresh Windows install ends up fully working.

$ErrorActionPreference = "Stop"
$ProgressPreference    = "SilentlyContinue"

Write-Host ""
Write-Host "  CERBERUS AI (BETA)" -ForegroundColor Cyan
Write-Host "  Bootstrapping beta installer..." -ForegroundColor DarkGray
Write-Host ""

$repoOwner = "tjcrims0nx"
$repoName  = "CerberusAI-Desktop"
$api       = "https://api.github.com/repos/$repoOwner/$repoName/releases"
$tmp       = $env:TEMP

try {
    $releases = Invoke-RestMethod -Uri $api -Headers @{ "User-Agent" = "CerberusBootstrap" } -TimeoutSec 15
    $rel = $releases | Where-Object { $_.prerelease -eq $true -and $_.draft -eq $false } | Select-Object -First 1
    if (-not $rel) {
        throw "No public beta prerelease found. Visit https://github.com/$repoOwner/$repoName/releases"
    }
    $tag = $rel.tag_name

    # 1. Pull the deploy/install.ps1 from the same release tag so the
    #    bootstrapper and the script never drift in version.
    $installScriptUrl = "https://raw.githubusercontent.com/$repoOwner/$repoName/$tag/deploy/install.ps1"
    $installScript    = Join-Path $tmp "cerberus-install.ps1"

    Write-Host "  Fetching beta installer ($tag)..." -ForegroundColor DarkGray
    try {
        Invoke-WebRequest -Uri $installScriptUrl -OutFile $installScript -UseBasicParsing
    } catch {
        # Fall back to the raw .exe path if the release tag predates the install.ps1 script.
        Write-Host "  Release $tag has no install.ps1; falling back to direct app installer." -ForegroundColor Yellow
        $asset = $rel.assets | Where-Object { $_.name -match "(?i)cerberus.*beta.*setup\.exe$" } | Select-Object -First 1
        if (-not $asset) {
            $asset = $rel.assets | Where-Object { $_.name -match "(?i)cerberus.*-setup\.exe$" } | Select-Object -First 1
        }
        if (-not $asset) {
            $asset = $rel.assets | Where-Object { $_.name -match "\.exe$" } | Select-Object -First 1
        }
        if (-not $asset) {
            throw "No installer found in release $tag. Visit https://github.com/$repoOwner/$repoName/releases"
        }
        $dest = Join-Path $tmp $asset.name
        Write-Host "  Downloading $($asset.name)..." -ForegroundColor DarkGray
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -UseBasicParsing
        Write-Host "  Running app installer (no Ollama bootstrap available for this release)..." -ForegroundColor DarkGray
        Start-Process -FilePath $dest -Wait
        Write-Host ""
        Write-Host "  Done. If Ollama is not installed, get it from https://ollama.com/download" -ForegroundColor Yellow
        Start-Sleep -Seconds 2
        exit 0
    }

    # 2. Run install.ps1 — it handles WebView2, Ollama (winget or direct .exe),
    #    a default model, and the Cerberus desktop app itself.
    Write-Host "  Running full beta installer (WebView2 + Ollama + Cerberus)..." -ForegroundColor DarkGray
    Write-Host ""
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installScript -ReleaseTag $tag
    $rc = $LASTEXITCODE
    if ($rc -ne 0) {
        throw "Installer exited with code $rc."
    }

    Write-Host ""
    Write-Host "  Done. Launch Cerberus from the Start Menu." -ForegroundColor Green
    Write-Host "  Get your API key at https://access.cerberusai.dev" -ForegroundColor DarkGray
    Write-Host ""
    Start-Sleep -Seconds 2
    exit 0

} catch {
    Write-Host "  ERROR: $_" -ForegroundColor Red
    Write-Host "  Manual download: https://github.com/$repoOwner/$repoName/releases" -ForegroundColor Yellow
    Write-Host "  Or install Ollama yourself:  https://ollama.com/download" -ForegroundColor Yellow
    exit 1
}
