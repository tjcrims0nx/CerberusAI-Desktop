param(
  [switch]$Wait,
  [switch]$Json
)

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$node = "node.exe"
$script = Join-Path $repo "scratch\test-installed-plugins.mjs"
$logDir = Join-Path $repo "scratch\plugin-verification"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$stamp = Get-Date -Format "yyyy-MM-ddTHH-mm-ss"
$stdout = Join-Path $logDir "hidden-$stamp.out.txt"
$stderr = Join-Path $logDir "hidden-$stamp.err.txt"
$arguments = @($script, "--quiet")

if ($Json) {
  $arguments = @($script, "--json")
}

$process = Start-Process `
  -FilePath $node `
  -ArgumentList $arguments `
  -WorkingDirectory $repo `
  -WindowStyle Hidden `
  -RedirectStandardOutput $stdout `
  -RedirectStandardError $stderr `
  -PassThru

if ($Wait) {
  $process.WaitForExit()
  $output = if (Test-Path $stdout) { Get-Content $stdout -Raw } else { "" }
  $errorOutput = if (Test-Path $stderr) { Get-Content $stderr -Raw } else { "" }

  if ($output.Trim()) {
    Write-Output $output.Trim()
  }
  if ($errorOutput.Trim()) {
    Write-Error $errorOutput.Trim()
  }
  exit $process.ExitCode
}

Write-Output "Started hidden plugin verification. PID $($process.Id). Logs: $logDir"
