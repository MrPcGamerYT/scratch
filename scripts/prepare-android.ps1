# Developer-only, interactive image preparation. No host disks are attached.
[CmdletBinding()]
param(
    [string]$IsoPath = '',
    [Parameter(Mandatory=$true)][string]$DiskPath,
    [Parameter(Mandatory=$true)][string]$PlayerFolder,
    [string]$QemuDir = (Join-Path $PSScriptRoot 'qemu/bin'),
    [switch]$Resume,
    [string]$CopyFrom = ''
)
$ErrorActionPreference = 'Stop'
try {
    $disk = [IO.Path]::GetFullPath($DiskPath)
    $targetDir = Join-Path ([IO.Path]::GetFullPath($PlayerFolder)) 'images'
    $target = Join-Path $targetDir 'android-base.qcow2'
    $img = Join-Path $QemuDir 'qemu-img.exe'
    if ($Resume -and $CopyFrom) { throw 'Choose either -Resume or -CopyFrom, not both.' }
    if (Test-Path -LiteralPath $target) { throw 'A prepared base already exists in the player folder. It will not be overwritten.' }
    if (-not (Test-Path -LiteralPath (Join-Path $PlayerFolder 'ProjectNexus.exe') -PathType Leaf)) { throw 'PlayerFolder must contain ProjectNexus.exe.' }
    if (-not (Test-Path -LiteralPath $img -PathType Leaf)) { throw "qemu-img not found: $img" }
    if ($Resume) {
        if (-not (Test-Path -LiteralPath $disk -PathType Leaf)) { throw 'Resume requires an existing disk file.' }
        Write-Host 'Resuming an existing disk. Stop any other VM using it and back it up first.'
    } else {
        if (Test-Path -LiteralPath $disk) { throw 'DiskPath already exists. Choose a NEW filename; this tool will not overwrite it.' }
        if (-not (Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($disk)) -PathType Container)) { throw 'Disk parent directory does not exist.' }
        if ($CopyFrom) {
            $source = [IO.Path]::GetFullPath($CopyFrom)
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'CopyFrom image not found.' }
            $sourceInfo = (& $img info --output=json $source | ConvertFrom-Json)
            if ($LASTEXITCODE -ne 0 -or $sourceInfo.format -ne 'qcow2' -or $null -ne $sourceInfo.'backing-filename') { throw 'CopyFrom must be a standalone QCOW2 image.' }
            Write-Host 'Copying an existing standalone image. Ensure it is shut down and not being written by any VM.'
            [IO.File]::Copy($source, $disk, $false)
        } else {
            if (-not $IsoPath -or -not (Test-Path -LiteralPath $IsoPath -PathType Leaf)) { throw 'Supply an existing -IsoPath for a new installation.' }
            Write-Host @'

ANDROID-X86 9.0-R2 GUIDED INSTALL (interactive, not unattended):
1. In the boot menu choose Advanced options, then Auto Installation.
2. The installer erases the selected virtual disk. This VM attaches only the
   new QCOW2 file, not your physical drives. Verify the selected target.
3. Confirm Yes. Wait for installation to finish; choose shutdown, not reboot
   (or close the installer only after it has finished).
4. Return here. The tool will boot the disk without the ISO for you to verify.
Other Android images can have different menus or may not support auto install.
'@
            & (Join-Path $PSScriptRoot 'launch.ps1') -QemuDir $QemuDir -IsoPath $IsoPath -DiskPath $disk -Boot iso
            if ($LASTEXITCODE -ne 0) { throw 'Installer VM failed. Disk was retained; use -Resume after investigating.' }
            if ((Read-Host 'Did installation finish and the guest shut down cleanly? Type YES to boot the disk') -cne 'YES') { throw 'Not staged. Disk retained; use -Resume to boot it later.' }
        }
    }
    $json = & $img info --output=json $disk
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect disk.' }
    $info = $json | ConvertFrom-Json
    if ($info.format -ne 'qcow2' -or $null -ne $info.'backing-filename') { throw 'Disk must be a standalone QCOW2 image with no backing file.' }
    & $img check $disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk failed qemu-img check. It was not staged.' }
    Write-Host 'Booting the disk without an ISO. Verify Android starts, make permitted changes in the guest, then shut it down cleanly.'
    & (Join-Path $PSScriptRoot 'launch.ps1') -QemuDir $QemuDir -DiskPath $disk -Boot disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk boot failed. Disk was retained; use -Resume after investigating.' }
    if ((Read-Host 'Did Android boot and shut down cleanly, and are you ready to stage it? Type YES') -cne 'YES') { throw 'Not staged. Disk retained.' }
    & $img check $disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk failed qemu-img check after boot. It was not staged.' }
    New-Item -ItemType Directory -Force $targetDir | Out-Null
    [IO.File]::Copy($disk, $target, $false)
    Write-Host "Prepared image staged at $target. Verify OS redistribution rights before sharing."
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
