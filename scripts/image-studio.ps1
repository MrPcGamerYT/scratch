# ISO-only desktop backend. Published image name matches the player loader.
[CmdletBinding()]
param(
    [ValidateSet('Install','Resume')][string]$Mode = 'Install',
    [string]$Source = '',
    [Parameter(Mandatory=$true)][string]$Workspace,
    [Parameter(Mandatory=$true)][string]$PlayerFolder,
    [string]$QemuDir = (Join-Path $PSScriptRoot 'qemu/bin'),
    [switch]$Bliss,
    [string]$ExpectedSha256 = '',
    [string]$ApprovalDirectory = '',
    [ValidateSet('whpx','tcg')][string]$Acceleration = 'whpx',
    [ValidateSet('std','virtio')][string]$Graphics = 'std'
)
$ErrorActionPreference = 'Stop'
try {
    if ($Mode -eq 'Install') {
        if (-not (Test-Path -LiteralPath $Source -PathType Leaf) -or [IO.Path]::GetExtension($Source) -ine '.iso') {
            throw 'Select an existing Bliss OS x86-64 PC installer ISO. ZIP, payload.bin and disk imports are not supported.'
        }
        if ($ExpectedSha256) {
            if ($ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'SHA-256 must contain 64 hexadecimal characters.' }
            if ((Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash -ine $ExpectedSha256) { throw 'ISO SHA-256 mismatch. No disk was created.' }
        }
    }
    $workspaceFull = [IO.Path]::GetFullPath($Workspace)
    $target = Join-Path ([IO.Path]::GetFullPath($PlayerFolder)) 'images/android-base.qcow2'
    if (-not (Test-Path -LiteralPath (Join-Path $PlayerFolder 'ProjectNexus.exe') -PathType Leaf)) { throw 'Choose the player folder containing ProjectNexus.exe.' }
    if (Test-Path -LiteralPath $target) { throw 'The player already has images/android-base.qcow2. Back it up; it will not be overwritten.' }
    New-Item -ItemType Directory -Force -Path $workspaceFull | Out-Null
    $disk = Join-Path $workspaceFull 'android-install.qcow2'
    $arguments = @{ DiskPath = $disk; PlayerFolder = $PlayerFolder; QemuDir = $QemuDir; Bliss = $Bliss; ApprovalDirectory = $ApprovalDirectory; Acceleration = $Acceleration; Graphics = $Graphics }
    if ($Mode -eq 'Resume') { $arguments.Resume = $true }
    else {
        if (Test-Path -LiteralPath $disk) { throw 'Workspace already contains android-install.qcow2. Select Resume or an empty workspace.' }
        $arguments.IsoPath = [IO.Path]::GetFullPath($Source)
    }
    Write-Output "Fixed player output: $target"
    & (Join-Path $PSScriptRoot 'prepare-android.ps1') @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Preparation stopped. Review the log; keep the log and workspace. Resume only if installation completed; otherwise select Install with a fresh workspace.' }
    Write-Output "SUCCESS: $target"
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
