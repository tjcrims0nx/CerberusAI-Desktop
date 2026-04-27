<#
.SYNOPSIS
    Cerberus AI — Windows installer / dependency bootstrapper.

.DESCRIPTION
    One-shot installer for Cerberus desktop. Detects and installs:
      1. Microsoft Edge WebView2 Runtime (required by the Tauri webview)
      2. Ollama for Windows (the local inference engine)
      3. A default Cerberus GGUF model (configurable)
      4. The Cerberus desktop app itself (NSIS installer from GitHub Releases)

.PARAMETER Model
    Ollama model tag to pull. Default: qwen2.5:3b (small, runs everywhere).
    Use "skip" to skip the model pull.

.PARAMETER ReleaseTag
    Specific app release tag to install (e.g. v0.1.0). Default: latest.

.PARAMETER Check
    Detection-only mode. Reports what's installed/missing and exits.

.PARAMETER Silent
    Run all sub-installers in silent mode (no UAC popups suppressed; just no UI).

.EXAMPLE
    iwr -useb https://cerberusai.dev/install.ps1 | iex

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Check

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Model "qwen2.5:7b" -Silent
#>
[CmdletBinding()]
param(
    [string]$Model = "qwen2.5:3b",
    [string]$ReleaseTag = "latest",
    [switch]$Check,
    [switch]$Silent
)

$ErrorActionPreference = "Stop"
$ProgressPreference   = "SilentlyContinue"

# ---------- Constants ----------
$RepoOwner    = "tjcrims0nx"
$RepoName     = "CerberusAI-Desktop"
$WebView2Url  = "https://go.microsoft.com/fwlink/p/?LinkId=2124703"   # Evergreen Bootstrapper
$OllamaUrl    = "https://ollama.com/download/OllamaSetup.exe"
$WorkDir      = Join-Path $env:TEMP "CerberusInstall"

# ---------- Brand output ----------
function Write-Brand {
    $banner = @"

   ____           _                          _    ___
  / ___|___ _ __ | |__   ___ _ __ _   _ ___ / \  |_ _|
 | |   / _ \ '__|| '_ \ / _ \ '__| | | / __/ _ \  | |
 | |__|  __/ |   | |_) |  __/ |  | |_| \__ \ ___ \ | |
  \____\___|_|   |_.__/ \___|_|   \__,_|___/_/  \_\___|

           Local-First. Unfiltered. Yours.
"@
    Write-Host $banner -ForegroundColor Red
    Write-Host "   https://cerberusai.dev`n" -ForegroundColor DarkGray
}

function Write-Step($msg)  { Write-Host "==> " -ForegroundColor Red -NoNewline; Write-Host $msg }
function Write-OK($msg)    { Write-Host " OK " -ForegroundColor Green -NoNewline; Write-Host " $msg" -ForegroundColor Gray }
function Write-Skip($msg)  { Write-Host "SKIP" -ForegroundColor DarkGray -NoNewline; Write-Host " $msg" -ForegroundColor DarkGray }
function Write-Warn2($msg) { Write-Host "WARN" -ForegroundColor Yellow -NoNewline; Write-Host " $msg" -ForegroundColor Yellow }
function Write-Err2($msg)  { Write-Host " ERR" -ForegroundColor Red -NoNewline; Write-Host " $msg" -ForegroundColor Red }

# ---------- Helpers ----------
function Test-Admin {
    $id = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    return ([System.Security.Principal.WindowsPrincipal]$id).IsInRole(
        [System.Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Ensure-WorkDir {
    if (-not (Test-Path $WorkDir)) { New-Item -ItemType Directory -Path $WorkDir | Out-Null }
}

function Test-Command($name) {
    return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

function Get-WebView2Version {
    # WebView2 Runtime registers itself under either of these keys depending on per-machine vs per-user install.
    $keys = @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    )
    foreach ($k in $keys) {
        try {
            $v = (Get-ItemProperty -Path $k -ErrorAction Stop).pv
            if ($v -and $v -ne "0.0.0.0") { return $v }
        } catch { }
    }
    return $null
}

function Test-OllamaService {
    try {
        $r = Invoke-WebRequest -Uri "http://127.0.0.1:11434/api/version" -UseBasicParsing -TimeoutSec 3
        return ($r.StatusCode -eq 200)
    } catch { return $false }
}

function Get-OllamaVersion {
    try {
        $r = Invoke-RestMethod -Uri "http://127.0.0.1:11434/api/version" -TimeoutSec 3
        return $r.version
    } catch { return $null }
}

function Get-InstalledOllamaModels {
    try {
        $r = Invoke-RestMethod -Uri "http://127.0.0.1:11434/api/tags" -TimeoutSec 5
        return @($r.models | ForEach-Object { $_.name })
    } catch { return @() }
}

function Get-LatestReleaseAsset {
    param([string]$Tag = "latest")
    $api = if ($Tag -eq "latest") {
        "https://api.github.com/repos/$RepoOwner/$RepoName/releases/latest"
    } else {
        "https://api.github.com/repos/$RepoOwner/$RepoName/releases/tags/$Tag"
    }
    try {
        $rel = Invoke-RestMethod -Uri $api -Headers @{ "User-Agent" = "CerberusInstaller" } -TimeoutSec 15
    } catch {
        throw "Could not reach GitHub releases at $api : $($_.Exception.Message)"
    }
    # Prefer NSIS .exe (smaller, no admin friction); fall back to MSI.
    $asset = $rel.assets | Where-Object { $_.name -match "Cerberus.*-setup\.exe$" } | Select-Object -First 1
    if (-not $asset) {
        $asset = $rel.assets | Where-Object { $_.name -match "Cerberus.*\.msi$" } | Select-Object -First 1
    }
    if (-not $asset) {
        throw "Release '$($rel.tag_name)' has no matching Cerberus installer asset."
    }
    return [pscustomobject]@{
        Name = $asset.name
        Url  = $asset.browser_download_url
        Size = $asset.size
        Tag  = $rel.tag_name
    }
}

function Save-Url {
    param([string]$Url, [string]$Path)
    Write-Host "    downloading $([System.IO.Path]::GetFileName($Path))..." -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $Url -OutFile $Path -UseBasicParsing
    return $Path
}

function Test-WingetReady {
    if (-not (Test-Command "winget")) { return $false }
    try {
        & winget --version *> $null
        return $LASTEXITCODE -eq 0
    } catch { return $false }
}

# ---------- Hardware preflight ----------
# Cerberus 4B v2 Abliterated Q4_K_M (~2.6 GB GGUF) is the smallest model
# offered on cerberusai.dev. These are the absolute minimums to load + run it
# at usable speed via Ollama / llama.cpp on Windows.
$MIN_RAM_GB        = 6
$MIN_FREE_DISK_GB  = 5
$REC_RAM_GB        = 8
$REC_VRAM_GB       = 3
$MODEL_SMALLEST    = "cerberus-4b-v2-abliterated (Q4_K_M, 2.6 GB)"

function Test-CpuAvx2 {
    # Win32 IsProcessorFeaturePresent — feature 40 = PF_AVX2_INSTRUCTIONS_AVAILABLE
    try {
        if (-not ([System.Management.Automation.PSTypeName]'CerbCpu').Type) {
            Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class CerbCpu {
    [DllImport("kernel32.dll")]
    public static extern bool IsProcessorFeaturePresent(uint feature);
}
"@ -ErrorAction SilentlyContinue
        }
        return [CerbCpu]::IsProcessorFeaturePresent(40)
    } catch {
        return $true   # if we can't check, don't block
    }
}

function Get-PrimaryGpu {
    try {
        $gpus = @(Get-CimInstance Win32_VideoController -ErrorAction Stop |
            Where-Object { $_.Name -notmatch "Basic Render|Microsoft Remote|Hyper-V" })
        if ($gpus.Count -eq 0) { return $null }
        # Prefer dedicated NVIDIA / AMD over Intel iGPU
        $dedicated = $gpus | Where-Object { $_.Name -match "NVIDIA|GeForce|RTX|GTX|Radeon|RX |Quadro|Tesla|Arc " } | Select-Object -First 1
        $primary = if ($dedicated) { $dedicated } else { $gpus[0] }
        $vramMB = if ($primary.AdapterRAM -and $primary.AdapterRAM -gt 0) {
            [math]::Round($primary.AdapterRAM / 1MB)
        } else { 0 }
        return [pscustomobject]@{
            Name   = $primary.Name
            VramMB = $vramMB
        }
    } catch { return $null }
}

function Get-FreeDiskGB {
    param([string]$Drive = $env:SystemDrive)
    try {
        $letter = $Drive[0]
        $d = Get-PSDrive -Name $letter -ErrorAction Stop
        return [math]::Round($d.Free / 1GB, 1)
    } catch { return 0 }
}

function Test-MinimumHardware {
    Write-Step "Hardware preflight (must run $MODEL_SMALLEST)"

    $os         = Get-CimInstance Win32_OperatingSystem
    $cpu        = Get-CimInstance Win32_Processor | Select-Object -First 1
    $totalRamGB = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
    $freeDiskGB = Get-FreeDiskGB
    $is64       = [System.Environment]::Is64BitOperatingSystem
    $hasAvx2    = Test-CpuAvx2
    $cpuName    = if ($cpu) { $cpu.Name.Trim() } else { "(unknown)" }
    $cpuCores   = if ($cpu) { $cpu.NumberOfCores } else { 0 }
    $gpu        = Get-PrimaryGpu
    $vramGB     = if ($gpu -and $gpu.VramMB -gt 0) { [math]::Round($gpu.VramMB / 1024, 1) } else { 0 }

    # Report
    Write-Host ""
    Write-Host "  CPU       : " -NoNewline -ForegroundColor DarkGray
    Write-Host "$cpuName ($cpuCores cores, AVX2=$hasAvx2)" -ForegroundColor Gray
    Write-Host "  RAM       : " -NoNewline -ForegroundColor DarkGray
    Write-Host "$totalRamGB GB total" -ForegroundColor $(if ($totalRamGB -ge $REC_RAM_GB) { "Gray" } else { "Yellow" })
    Write-Host "  GPU       : " -NoNewline -ForegroundColor DarkGray
    if ($gpu) {
        Write-Host "$($gpu.Name) ($vramGB GB VRAM)" -ForegroundColor Gray
    } else {
        Write-Host "(none detected — CPU-only inference)" -ForegroundColor Yellow
    }
    Write-Host "  Free disk : " -NoNewline -ForegroundColor DarkGray
    Write-Host "$freeDiskGB GB on $env:SystemDrive" -ForegroundColor $(if ($freeDiskGB -ge $MIN_FREE_DISK_GB) { "Gray" } else { "Red" })
    Write-Host ""

    # Hard blockers
    $blockers = @()
    if (-not $is64)                       { $blockers += "Cerberus requires 64-bit Windows. Current OS is 32-bit." }
    if (-not $hasAvx2)                    { $blockers += "CPU does not report AVX2 support. Cerberus models need AVX2 for usable speed." }
    if ($totalRamGB -lt $MIN_RAM_GB)      { $blockers += "Need at least $MIN_RAM_GB GB system RAM (you have $totalRamGB GB)." }
    if ($freeDiskGB -lt $MIN_FREE_DISK_GB) { $blockers += "Need at least $MIN_FREE_DISK_GB GB free on $env:SystemDrive (you have $freeDiskGB GB)." }

    if ($blockers.Count -gt 0) {
        Write-Err2 "This machine cannot run $MODEL_SMALLEST."
        foreach ($b in $blockers) {
            Write-Host "    - $b" -ForegroundColor Red
        }
        Write-Host ""
        Write-Host "  Install aborted. Free up resources or use a more capable machine." -ForegroundColor Yellow
        Write-Host "  Hardware spec: https://cerberusai.dev/docs#requirements" -ForegroundColor DarkGray
        exit 2
    }

    # Soft warnings
    if ($totalRamGB -lt $REC_RAM_GB) {
        Write-Warn2 "RAM is below $REC_RAM_GB GB recommended. Cerberus will work but may swap during long contexts."
    }
    if (-not $gpu -or $vramGB -lt $REC_VRAM_GB) {
        Write-Warn2 "No GPU with >=$REC_VRAM_GB GB VRAM detected. Inference will run on CPU at ~3-8 tokens/sec."
    } else {
        Write-OK "GPU acceleration available ($($gpu.Name), $vramGB GB VRAM)"
    }
    Write-OK "Hardware passes minimum to run $MODEL_SMALLEST"
}

# ---------- Detect ----------
function Invoke-Detect {
    $report = [ordered]@{}

    $report.OS = "$((Get-CimInstance Win32_OperatingSystem).Caption) ($([System.Environment]::OSVersion.Version))"

    $wv = Get-WebView2Version
    $report.WebView2 = if ($wv) { $wv } else { "MISSING" }

    if (Test-Command "ollama") {
        $report.OllamaCli = (& ollama --version 2>$null | Select-Object -First 1)
    } else {
        $report.OllamaCli = "MISSING"
    }
    $svcVer = Get-OllamaVersion
    $report.OllamaService = if ($svcVer) { "running ($svcVer)" } else { "not running" }

    if ($svcVer) {
        $models = Get-InstalledOllamaModels
        $report.Models = if ($models.Count) { ($models -join ", ") } else { "(none pulled)" }
    } else {
        $report.Models = "(unknown - service offline)"
    }

    $report.Winget = if (Test-WingetReady) { "available" } else { "not available" }
    $report.Admin  = if (Test-Admin) { "yes" } else { "no" }

    try {
        $gpu = Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch "Basic Render" }
        $report.GPU = ($gpu | ForEach-Object {
            $vram = if ($_.AdapterRAM) { " ({0:N0} MB)" -f ($_.AdapterRAM / 1MB) } else { "" }
            "$($_.Name)$vram"
        }) -join " | "
    } catch { $report.GPU = "(detection failed)" }

    Write-Host "`n  Cerberus dependency report" -ForegroundColor Red
    Write-Host "  --------------------------" -ForegroundColor DarkGray
    foreach ($k in $report.Keys) {
        $val = [string]$report[$k]
        $color = if ($val -eq "MISSING" -or $val -match "not (running|available)") { "Red" } else { "Gray" }
        $label = "{0,-15}" -f $k
        Write-Host "  $label : " -NoNewline -ForegroundColor DarkGray
        Write-Host $val -ForegroundColor $color
    }
    Write-Host ""
}

# ---------- Install: WebView2 ----------
function Ensure-WebView2 {
    $v = Get-WebView2Version
    if ($v) { Write-OK "WebView2 Runtime present ($v)"; return }
    Write-Step "Installing Microsoft Edge WebView2 Runtime"
    Ensure-WorkDir
    $exe = Join-Path $WorkDir "MicrosoftEdgeWebview2Setup.exe"
    Save-Url -Url $WebView2Url -Path $exe
    $args = @("/silent", "/install")
    $p = Start-Process -FilePath $exe -ArgumentList $args -Wait -PassThru -Verb RunAs
    if ($p.ExitCode -ne 0) { throw "WebView2 installer exit code $($p.ExitCode)" }
    if (-not (Get-WebView2Version)) { throw "WebView2 install reported success but runtime still not detected." }
    Write-OK "WebView2 installed"
}

# ---------- Install: Ollama ----------
function Ensure-Ollama {
    if (Test-Command "ollama") { Write-OK "Ollama CLI present"; }
    else {
        Write-Step "Installing Ollama for Windows"
        if (Test-WingetReady) {
            $wargs = @("install", "--id", "Ollama.Ollama", "-e", "--accept-source-agreements", "--accept-package-agreements")
            if ($Silent) { $wargs += @("--silent") }
            & winget @wargs
            if ($LASTEXITCODE -ne 0) { throw "winget install Ollama.Ollama failed with $LASTEXITCODE" }
        } else {
            Ensure-WorkDir
            $exe = Join-Path $WorkDir "OllamaSetup.exe"
            Save-Url -Url $OllamaUrl -Path $exe
            $oargs = if ($Silent) { @("/SILENT") } else { @() }
            $p = Start-Process -FilePath $exe -ArgumentList $oargs -Wait -PassThru
            if ($p.ExitCode -ne 0) { throw "Ollama installer exit code $($p.ExitCode)" }
        }
        # Refresh PATH so we can find ollama.exe in this session.
        $env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                    [System.Environment]::GetEnvironmentVariable("Path", "User")
        if (-not (Test-Command "ollama")) {
            Write-Warn2 "ollama installed but not on PATH for this shell. Open a new terminal afterward."
        } else {
            Write-OK "Ollama installed"
        }
    }

    # Make sure the service is up.
    if (-not (Test-OllamaService)) {
        Write-Step "Starting Ollama service"
        try {
            Start-Process -FilePath "ollama" -ArgumentList "serve" -WindowStyle Hidden | Out-Null
        } catch {
            Write-Warn2 "Could not auto-start ollama serve: $_"
        }
        $deadline = (Get-Date).AddSeconds(20)
        while (-not (Test-OllamaService) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
        if (Test-OllamaService) { Write-OK "Ollama service running ($(Get-OllamaVersion))" }
        else { Write-Warn2 "Ollama service did not respond within 20s. Try: ollama serve" }
    } else {
        Write-OK "Ollama service running ($(Get-OllamaVersion))"
    }
}

# ---------- Install: Model ----------
function Ensure-Model {
    if ($Model -eq "skip") { Write-Skip "model pull (--Model skip)"; return }
    if (-not (Test-OllamaService)) {
        Write-Warn2 "Ollama service offline; can't pull $Model. Run 'ollama pull $Model' yourself later."
        return
    }
    $existing = Get-InstalledOllamaModels
    $base = ($Model -split ":")[0]
    if ($existing -contains $Model -or ($existing | Where-Object { $_ -like "${base}:*" })) {
        Write-OK "Model already pulled ($Model)"
        return
    }
    Write-Step "Pulling model $Model (this can take a while)"
    & ollama pull $Model
    if ($LASTEXITCODE -ne 0) { throw "ollama pull $Model failed ($LASTEXITCODE)" }
    Write-OK "Model $Model pulled"
}

# ---------- Install: Cerberus app ----------
function Install-CerberusApp {
    Write-Step "Resolving latest Cerberus release"
    try {
        $asset = Get-LatestReleaseAsset -Tag $ReleaseTag
    } catch {
        Write-Warn2 "$_"
        Write-Warn2 "Skipping app install. You can grab it manually from https://github.com/$RepoOwner/$RepoName/releases"
        return
    }
    Write-OK "Found $($asset.Name) ($([math]::Round($asset.Size / 1MB, 1)) MB) from $($asset.Tag)"

    Ensure-WorkDir
    $dest = Join-Path $WorkDir $asset.Name
    Save-Url -Url $asset.Url -Path $dest

    Write-Step "Running Cerberus installer"
    if ($asset.Name.EndsWith(".msi")) {
        $msiArgs = @("/i", "`"$dest`"")
        if ($Silent) { $msiArgs += @("/qn", "/norestart") }
        $p = Start-Process -FilePath "msiexec.exe" -ArgumentList $msiArgs -Wait -PassThru
    } else {
        $args = if ($Silent) { @("/S") } else { @() }
        $p = Start-Process -FilePath $dest -ArgumentList $args -Wait -PassThru
    }
    if ($p.ExitCode -ne 0) { throw "Cerberus installer exit code $($p.ExitCode)" }
    Write-OK "Cerberus installed"
}

# ---------- Main ----------
Write-Brand

if ($Check) {
    Invoke-Detect
    return
}

Write-Host "  Mode      : install" -ForegroundColor DarkGray
Write-Host "  Model     : $Model" -ForegroundColor DarkGray
Write-Host "  ReleaseTag: $ReleaseTag" -ForegroundColor DarkGray
Write-Host "  Silent    : $Silent`n" -ForegroundColor DarkGray

try {
    Test-MinimumHardware
    Ensure-WebView2
    Ensure-Ollama
    Ensure-Model
    Install-CerberusApp
} catch {
    Write-Err2 $_.Exception.Message
    Write-Host "`nInstall failed. Run the script with -Check to see what's already in place, then retry." -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-OK "All set. Launch Cerberus from the Start Menu, or run:  cerberus"
Write-Host "    Docs:    https://cerberusai.dev/docs" -ForegroundColor DarkGray
Write-Host "    Discord: https://discord.gg/YvfewgZ6re`n" -ForegroundColor DarkGray
