param(
  [int]$Port = 1420,
  [string]$CargoTargetDir = ""
)

$ErrorActionPreference = "Stop"

function Write-Step {
  param([string]$Message)
  Write-Host "[desktop-dev] $Message"
}

function Get-ProcessCommandLine {
  param([int]$ProcessId)
  try {
    return (Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId").CommandLine
  } catch {
    return ""
  }
}

function Stop-IfOwnedPort {
  param([int]$Port)
  $connections = @(Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue | Where-Object { $_.State -eq "Listen" })
  foreach ($connection in $connections) {
    $commandLine = Get-ProcessCommandLine -ProcessId $connection.OwningProcess
    $owned = $commandLine -match "vite.*--port $Port" -or
      $commandLine -match "verity[\\/]+desktop"
    if (-not $owned) {
      throw "Port $Port is already owned by PID $($connection.OwningProcess): $commandLine"
    }
    Write-Step "Stopping stale desktop dev port owner PID $($connection.OwningProcess)"
    Stop-Process -Id $connection.OwningProcess -Force -ErrorAction SilentlyContinue
  }
}

function Stop-StaleDesktopDevProcesses {
  param([int]$Port)
  $staleProcesses = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object {
      $_.Name -eq "node.exe" -and (
        $_.CommandLine -match "vite[\\/]+bin[\\/]+vite\.js.*--port $Port" -or
        $_.CommandLine -match "@tauri-apps[\\/]+cli[\\/]+tauri\.js dev"
      )
    }
  foreach ($process in $staleProcesses) {
    Write-Step "Stopping stale desktop dev process PID $($process.ProcessId)"
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
  }
}

function Wait-ForHttp {
  param(
    [string]$Url,
    [int]$TimeoutSeconds = 600
  )
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    try {
      $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 3
      if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 500) {
        return
      }
    } catch {
      Start-Sleep -Seconds 5
    }
  } while ((Get-Date) -lt $deadline)
  throw "Timed out waiting for $Url"
}

function Wait-ForWindow {
  param([int]$TimeoutSeconds = 240)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  do {
    $window = Get-Process -Name "verity-desktop" -ErrorAction SilentlyContinue |
      Where-Object { $_.MainWindowHandle -ne 0 } |
      Select-Object -First 1
    if ($window) {
      return $window
    }
    Start-Sleep -Seconds 2
  } while ((Get-Date) -lt $deadline)
  throw "Timed out waiting for the Verity window"
}

function Convert-UncToMappedPath {
  param([string]$Path)
  if (-not $Path.StartsWith("\\")) {
    return $Path
  }

  $target = $Path.TrimEnd("\")
  $mappedDrives = Get-PSDrive -PSProvider FileSystem -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayRoot -and ([string]$_.DisplayRoot).StartsWith("\\") } |
    Sort-Object @{ Expression = { if ($_.Name -eq "Z") { 0 } else { 1 } } }, Name
  foreach ($existing in $mappedDrives) {
    $remoteRoot = ([string]$existing.DisplayRoot).TrimEnd("\")
    if ($target -ieq $remoteRoot) {
      $mapped = "$($existing.Name):\"
      Write-Step "Reusing mapped drive $mapped for $Path"
      return $mapped
    }
    if ($target.StartsWith("$remoteRoot\", [System.StringComparison]::OrdinalIgnoreCase)) {
      $relative = $target.Substring($remoteRoot.Length).TrimStart("\")
      $mapped = Join-Path "$($existing.Name):\" $relative
      if (Test-Path -LiteralPath $mapped) {
        Write-Step "Reusing mapped path $mapped for $Path"
        return $mapped
      }
    }
  }

  $letters = @("X", "Y", "W", "V", "U", "T")
  foreach ($letter in $letters) {
    $drive = "${letter}:"
    $existing = Get-PSDrive -Name $letter -ErrorAction SilentlyContinue
    if ($existing) {
      $root = [string]$existing.Root
      if ($root.TrimEnd("\") -ieq $Path.TrimEnd("\")) {
        Write-Step "Reusing mapped drive $drive for $Path"
        return "$drive\"
      }
      continue
    }

    Write-Step "Mapping $drive to $Path because Node/Vite cannot reliably load config from UNC file URLs"
    $result = & net use $drive $Path /persistent:no 2>&1
    if ($LASTEXITCODE -eq 0) {
      return "$drive\"
    }
    Write-Step "Could not map ${drive}: $result"
  }

  throw "Could not map UNC path to a drive letter: $Path"
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$desktopRoot = (Resolve-Path -LiteralPath (Join-Path $scriptRoot "..")).ProviderPath
$desktopRoot = [System.IO.Path]::GetFullPath($desktopRoot)
if ($desktopRoot.StartsWith("\\")) {
  $repoRoot = [System.IO.Path]::GetFullPath((Join-Path $desktopRoot ".."))
  $mappedRepoRoot = Convert-UncToMappedPath -Path $repoRoot
  $desktopRoot = Join-Path $mappedRepoRoot "desktop"
} else {
  $desktopRoot = Convert-UncToMappedPath -Path $desktopRoot
}
$desktopRoot = [System.IO.Path]::GetFullPath($desktopRoot)
Set-Location -LiteralPath $desktopRoot

$nodeExe = (Get-Command node.exe -ErrorAction Stop).Source
$viteCli = (Resolve-Path -LiteralPath (Join-Path $desktopRoot "node_modules/vite/bin/vite.js")).ProviderPath
$tauriCli = (Resolve-Path -LiteralPath (Join-Path $desktopRoot "node_modules/@tauri-apps/cli/tauri.js")).ProviderPath

if (-not $CargoTargetDir) {
  $preferredTargetRoot = if (Test-Path "D:\") { "D:\Verity" } else { Join-Path $env:LOCALAPPDATA "Verity" }
  $CargoTargetDir = Join-Path $preferredTargetRoot "tauri-target"
}
New-Item -ItemType Directory -Force -Path $CargoTargetDir | Out-Null

$env:VERITY_DESKTOP_USE_POLLING = "1"
$env:CHOKIDAR_USEPOLLING = "1"
$env:CHOKIDAR_INTERVAL = "1000"
$env:WATCHPACK_POLLING = "true"
$env:CARGO_TARGET_DIR = $CargoTargetDir

$logDir = Join-Path $env:TEMP "verity-desktop"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$viteOut = Join-Path $logDir "vite.out.log"
$viteErr = Join-Path $logDir "vite.err.log"
$tauriOut = Join-Path $logDir "tauri.out.log"
$tauriErr = Join-Path $logDir "tauri.err.log"
$tauriConfig = Join-Path $logDir "tauri.dev.override.json"

@{
  build = @{
    devUrl = "http://127.0.0.1:$Port"
    beforeDevCommand = $null
  }
} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $tauriConfig -Encoding UTF8

Write-Step "Desktop root: $desktopRoot"
Write-Step "Cargo target: $CargoTargetDir"
Write-Step "Logs: $logDir"

Stop-IfOwnedPort -Port $Port
Stop-StaleDesktopDevProcesses -Port $Port
Get-Process -Name "verity-desktop" -ErrorAction SilentlyContinue |
  Stop-Process -Force -ErrorAction SilentlyContinue

Remove-Item -LiteralPath $viteOut, $viteErr, $tauriOut, $tauriErr -Force -ErrorAction SilentlyContinue

Write-Step "Starting Vite on http://127.0.0.1:$Port with polling"
$viteArgs = @(
  $viteCli,
  "--host", "127.0.0.1",
  "--port", [string]$Port,
  "--strictPort",
  "--clearScreen", "false"
)
$viteProcess = Start-Process -FilePath $nodeExe -ArgumentList $viteArgs -WorkingDirectory $desktopRoot -RedirectStandardOutput $viteOut -RedirectStandardError $viteErr -WindowStyle Hidden -PassThru
Wait-ForHttp -Url "http://127.0.0.1:$Port" -TimeoutSeconds 600
Write-Step "Vite is ready (PID $($viteProcess.Id))"

Write-Step "Starting Tauri with --no-watch and external dev server"
$tauriArgs = @(
  $tauriCli,
  "dev",
  "--no-watch",
  "--no-dev-server-wait",
  "--config", $tauriConfig
)
$tauriProcess = Start-Process -FilePath $nodeExe -ArgumentList $tauriArgs -WorkingDirectory $desktopRoot -RedirectStandardOutput $tauriOut -RedirectStandardError $tauriErr -WindowStyle Hidden -PassThru

$window = Wait-ForWindow -TimeoutSeconds 300
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class VerityWindow {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@ -ErrorAction SilentlyContinue
[VerityWindow]::ShowWindow($window.MainWindowHandle, 9) | Out-Null
[VerityWindow]::SetForegroundWindow($window.MainWindowHandle) | Out-Null

Write-Step "Opened Verity (PID $($window.Id), launcher PID $($tauriProcess.Id))"
Write-Step "If it closes, inspect $tauriErr and $viteErr"
