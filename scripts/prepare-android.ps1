# Developer tool: install a legally obtained Android ISO into a new standalone disk.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$IsoPath,
    [Parameter(Mandatory=$true)][string]$DiskPath,
    [Parameter(Mandatory=$true)][string]$PlayerFolder,
    [string]$QemuDir = (Join-Path $PSScriptRoot 'qemu/bin')
)
$ErrorActionPreference = 'Stop'
try {
    $disk = [IO.Path]::GetFullPath($DiskPath)
    $targetDir = Join-Path ([IO.Path]::GetFullPath($PlayerFolder)) 'images'
    $target = Join-Path $targetDir 'android-base.qcow2'
    if (Test-Path -LiteralPath $disk) { throw 'DiskPath already exists. Choose a NEW filename; this tool will not overwrite it.' }
    if (Test-Path -LiteralPath $target) { throw 'A prepared base already exists in the player folder. It will not be overwritten.' }
    if (-not (Test-Path -LiteralPath (Join-Path $PlayerFolder 'ProjectNexus.exe') -PathType Leaf)) { throw 'PlayerFolder must contain ProjectNexus.exe.' }
    Write-Host 'Install Android to the new virtual disk in the guest installer. Shut down the guest when finished.'
    & (Join-Path $PSScriptRoot 'launch.ps1') -QemuDir $QemuDir -IsoPath $IsoPath -DiskPath $disk -Boot iso
    if ($LASTEXITCODE -ne 0) { throw 'Installer VM failed. Disk was retained for inspection.' }
    if ((Read-Host 'Did you install Android to the virtual disk and shut it down cleanly? Type YES to stage it') -cne 'YES') { throw 'Not staged. The disk was retained.' }
    $img = Join-Path $QemuDir 'qemu-img.exe'
    $json = & $img info --output=json $disk
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect disk.' }
    $info = $json | ConvertFrom-Json
    if ($info.format -ne 'qcow2' -or $null -ne $info.'backing-filename') { throw 'Disk must be a standalone QCOW2 image with no backing file.' }
    & $img check $disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk failed qemu-img check. It was not staged.' }
    New-Item -ItemType Directory -Force $targetDir | Out-Null
    [IO.File]::Copy($disk, $target, $false)
    Write-Host "Prepared image staged at $target. Test boot and verify OS redistribution rights before sharing."
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
