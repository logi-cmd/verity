# SPDX-License-Identifier: MPL-2.0
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][ValidateSet('msi', 'nsis')][string]$InstallerType,
  [Parameter(Mandatory = $true)][string]$InstallerPath,
  [string]$ProductName = 'Verity'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-CheckedProcess {
  param([string]$FilePath, [string[]]$ArgumentList)
  $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -Wait -PassThru -WindowStyle Hidden
  if ($process.ExitCode -ne 0) {
    throw "$FilePath exited with code $($process.ExitCode)"
  }
}

function Resolve-SignTool {
  $roots = @(
    "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
    "${env:ProgramFiles}\Windows Kits\10\bin"
  ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
  $tool = Get-ChildItem -LiteralPath $roots -Filter signtool.exe -Recurse -File |
    Where-Object FullName -Match '\\x64\\' |
    Sort-Object FullName -Descending |
    Select-Object -First 1
  if (-not $tool) { throw 'signtool.exe was not found' }
  return $tool.FullName
}

function Assert-TrustedSignature {
  param([string]$Path, [string]$SignTool)
  & $SignTool verify /pa /all $Path | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for $Path" }
  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  if ($signature.Status -ne 'Valid') { throw "Windows does not trust the signature for $Path" }
  if (-not $signature.TimeStamperCertificate) { throw "The signature for $Path does not have a trusted timestamp" }
  return [pscustomobject]@{
    SignerSubject = $signature.SignerCertificate.Subject
    TimestampSubject = $signature.TimeStamperCertificate.Subject
  }
}

function Get-ProductEntries {
  $roots = @(
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
  )
  return @(Get-ItemProperty -Path $roots -ErrorAction SilentlyContinue |
    Where-Object DisplayName -EQ $ProductName)
}

function Wait-ForProductEntry {
  $deadline = (Get-Date).AddSeconds(30)
  do {
    $entries = @(Get-ProductEntries)
    if ($entries.Count -eq 1) { return $entries[0] }
    if ($entries.Count -gt 1) { throw "Multiple $ProductName installations were found" }
    Start-Sleep -Milliseconds 500
  } while ((Get-Date) -lt $deadline)
  throw "$ProductName did not register an uninstall entry"
}

function Resolve-InstalledExecutable {
  param($Entry)
  $names = @('verity-desktop.exe', 'Verity.exe', 'verity.exe')
  $candidates = [System.Collections.Generic.List[string]]::new()
  if ($Entry.InstallLocation) {
    foreach ($name in $names) { $candidates.Add((Join-Path $Entry.InstallLocation $name)) }
  }
  if ($Entry.DisplayIcon) {
    $candidates.Add(($Entry.DisplayIcon -replace '^"|"(?:,\d+)?$|,\d+$', ''))
  }
  foreach ($root in @($env:LOCALAPPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if (-not $root) { continue }
    foreach ($name in $names) { $candidates.Add((Join-Path (Join-Path $root $ProductName) $name)) }
  }
  $resolved = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
  if (-not $resolved) { throw "The installed $ProductName executable was not found" }
  return (Resolve-Path -LiteralPath $resolved).Path
}

function Assert-DesktopLaunch {
  param([string]$Executable)
  $process = Start-Process -FilePath $Executable -PassThru
  try {
    $deadline = (Get-Date).AddSeconds(30)
    do {
      Start-Sleep -Milliseconds 500
      $process.Refresh()
      if ($process.HasExited) { throw "$ProductName exited before launch smoke completed" }
      if ($process.MainWindowHandle -ne 0) { break }
    } while ((Get-Date) -lt $deadline)
    if ($process.MainWindowHandle -eq 0) { throw "$ProductName did not create a desktop window" }
    Start-Sleep -Seconds 3
    $process.Refresh()
    if ($process.HasExited) { throw "$ProductName did not remain active during launch observation" }
  } finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
  }
}

function Invoke-Install {
  if ($InstallerType -eq 'msi') {
    Invoke-CheckedProcess -FilePath 'msiexec.exe' -ArgumentList @('/i', "`"$InstallerPath`"", '/qn', '/norestart')
  } else {
    Invoke-CheckedProcess -FilePath $InstallerPath -ArgumentList @('/S')
  }
}

function Invoke-Uninstall {
  param($Entry, [string]$Executable)
  if ($InstallerType -eq 'msi') {
    Invoke-CheckedProcess -FilePath 'msiexec.exe' -ArgumentList @('/x', "`"$InstallerPath`"", '/qn', '/norestart')
    return
  }
  $uninstaller = Join-Path (Split-Path -Parent $Executable) 'uninstall.exe'
  if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
    throw 'The NSIS uninstaller was not found beside the installed executable'
  }
  Invoke-CheckedProcess -FilePath $uninstaller -ArgumentList @('/S')
}

$installer = (Resolve-Path -LiteralPath $InstallerPath).Path
if (Get-ProductEntries) { throw "$ProductName must not be installed before acceptance smoke" }
$signTool = Resolve-SignTool
$installerSignature = Assert-TrustedSignature -Path $installer -SignTool $signTool

$installedExecutable = $null
try {
  Invoke-Install
  $entry = Wait-ForProductEntry
  $installedExecutable = Resolve-InstalledExecutable -Entry $entry
  $installedSignature = Assert-TrustedSignature -Path $installedExecutable -SignTool $signTool
  if ($installedSignature.SignerSubject -ne $installerSignature.SignerSubject) {
    throw 'The installer and installed application have different Authenticode publishers'
  }
  Assert-DesktopLaunch -Executable $installedExecutable
  Invoke-Uninstall -Entry $entry -Executable $installedExecutable

  $deadline = (Get-Date).AddSeconds(30)
  do {
    if (-not (Get-ProductEntries) -and -not (Test-Path -LiteralPath $installedExecutable)) { break }
    Start-Sleep -Milliseconds 500
  } while ((Get-Date) -lt $deadline)
  if (Get-ProductEntries) { throw "$ProductName remained registered after uninstall" }
  if (Test-Path -LiteralPath $installedExecutable) { throw "$ProductName executable remained after uninstall" }
} catch {
  if ($installedExecutable -and (Test-Path -LiteralPath $installedExecutable)) {
    try {
      $entry = @(Get-ProductEntries) | Select-Object -First 1
      if ($entry) { Invoke-Uninstall -Entry $entry -Executable $installedExecutable }
    } catch {}
  }
  throw
}

[ordered]@{
  kind = $InstallerType
  name = [IO.Path]::GetFileName($installer)
  sha256 = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
  signerSubject = $installerSignature.SignerSubject
  timestampSubject = $installerSignature.TimestampSubject
  trustedSignature = $true
  installSmoke = $true
  launchSmoke = $true
  uninstallSmoke = $true
} | ConvertTo-Json -Compress
