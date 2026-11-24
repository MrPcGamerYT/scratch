# Developer-only, interactive image preparation. No host disks are attached.
[CmdletBinding()]
param(
    [string]$IsoPath = '',
    [Parameter(Mandatory=$true)][string]$DiskPath,
    [Parameter(Mandatory=$true)][string]$PlayerFolder,
    [string]$QemuDir = (Join-Path $PSScriptRoot 'qemu/bin'),
    [switch]$Resume,
    [string]$CopyFrom = '',
    [switch]$Bliss,
    [string]$SourceVars = '',
    [string]$ExpectedSha256 = '',
    [string]$ApprovalDirectory = ''
)
$ErrorActionPreference = 'Stop'
function Confirm-InstallationStage([string]$Prompt) {
    if (-not $ApprovalDirectory) { return (Read-Host $Prompt) }
    if (-not (Test-Path -LiteralPath $ApprovalDirectory -PathType Container)) { throw 'Approval directory is missing.' }
    $request = Join-Path $ApprovalDirectory 'request.txt'
    $response = Join-Path $ApprovalDirectory 'approve.txt'
    if (Test-Path -LiteralPath $response) { Remove-Item -LiteralPath $response }
    [IO.File]::WriteAllText($request, $Prompt)
    Write-Output "APPROVAL REQUIRED: $Prompt" | Out-Host
    $deadline = [DateTime]::UtcNow.AddHours(1)
    while (-not (Test-Path -LiteralPath $response)) {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Approval timed out. Workspace retained for Resume.' }
        Start-Sleep -Milliseconds 300
    }
    $answer = [IO.File]::ReadAllText($response).Trim()
    Remove-Item -LiteralPath $request, $response
    return $answer
}
try {
    $disk = [IO.Path]::GetFullPath($DiskPath)
    $targetDir = Join-Path ([IO.Path]::GetFullPath($PlayerFolder)) 'images'
    $target = Join-Path $targetDir 'android-base.qcow2'
    $img = Join-Path $QemuDir 'qemu-img.exe'
    $uefiCode = [IO.Path]::GetFullPath((Join-Path $QemuDir '../share/qemu/edk2-x86_64-code.fd'))
    $uefiTemplate = [IO.Path]::GetFullPath((Join-Path $QemuDir '../share/qemu/edk2-i386-vars.fd'))
    $vars = $disk + '.uefi-vars.fd'
    $stagedCode = Join-Path $targetDir 'android-uefi-code.fd'
    $stagedVars = Join-Path $targetDir 'android-uefi-vars.fd'
    if ($SourceVars -and (-not $Bliss -or -not $CopyFrom)) { throw 'SourceVars is only valid with -Bliss -CopyFrom.' }
    if ($ExpectedSha256 -and $ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'ExpectedSha256 must be a 64-character SHA-256 hash.' }
    if ($Bliss -and (-not (Test-Path -LiteralPath $uefiCode -PathType Leaf) -or -not (Test-Path -LiteralPath $uefiTemplate -PathType Leaf))) { throw 'Bundled UEFI firmware is missing; use the updated Image Studio package.' }
    if ((Test-Path -LiteralPath $stagedCode) -or (Test-Path -LiteralPath $stagedVars)) { throw 'UEFI staging files already exist. No files will be overwritten.' }
    if ($Resume -and $CopyFrom) { throw 'Choose either -Resume or -CopyFrom, not both.' }
    if (Test-Path -LiteralPath $target) { throw 'A prepared base already exists in the player folder. It will not be overwritten.' }
    if (-not (Test-Path -LiteralPath (Join-Path $PlayerFolder 'ProjectNexus.exe') -PathType Leaf)) { throw 'PlayerFolder must contain ProjectNexus.exe.' }
    if (-not (Test-Path -LiteralPath $img -PathType Leaf)) { throw "qemu-img not found: $img" }
    if ($Resume) {
        if (-not (Test-Path -LiteralPath $disk -PathType Leaf)) { throw 'Resume requires an existing disk file.' }
        if ($Bliss -and -not (Test-Path -LiteralPath $vars -PathType Leaf)) { throw 'Resume requires the existing DiskPath.uefi-vars.fd file.' }
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
            if ($Bliss -and (-not $SourceVars -or -not (Test-Path -LiteralPath $SourceVars -PathType Leaf))) { throw 'Copying a Bliss UEFI image requires -SourceVars from that image.' }
            if ($Bliss -and (Test-Path -LiteralPath $vars)) { throw 'Destination UEFI vars already exist; none will be overwritten.' }
            [IO.File]::Copy($source, $disk, $false)
            if ($Bliss) { [IO.File]::Copy([IO.Path]::GetFullPath($SourceVars), $vars, $false) }
        } else {
            if (-not $IsoPath -or -not (Test-Path -LiteralPath $IsoPath -PathType Leaf)) { throw 'Supply an existing -IsoPath for a new installation.' }
            if ($ExpectedSha256 -and (Get-FileHash -Algorithm SHA256 -LiteralPath $IsoPath).Hash -ine $ExpectedSha256) { throw 'ISO SHA-256 mismatch. No disk was created.' }
            if ($Bliss) {
                if (Test-Path -LiteralPath $vars) { throw 'Destination UEFI vars already exist; none will be overwritten.' }
                [IO.File]::Copy($uefiTemplate, $vars, $false)
                Write-Host @'

BLISS OS GUIDED UEFI INSTALL (interactive; menus vary by image):
1. Select the Bliss installation option, not live boot or Auto-Install.
2. Create/choose a partition on the NEW virtual disk. Only the virtual disk is
   attached; formatting erases it. Never select a physical drive.
3. Format the selected virtual partition (ext4 where supported), accept UEFI
   GRUB when offered, and wait for installation to finish. Shut down cleanly.
4. The tool boots the installed disk without the ISO so you can verify it.
Official guidance: https://docs.blissos.org/installation/install-in-a-virtual-machine/
Bliss boot, graphics and game compatibility are not guaranteed.
'@
            } else { Write-Host @'

ANDROID-X86 9.0-R2 GUIDED INSTALL (interactive, not unattended):
1. In the boot menu choose Advanced options, then Auto Installation.
2. The installer erases the selected virtual disk. This VM attaches only the
   new QCOW2 file, not your physical drives. Verify the selected target.
3. Confirm Yes. Wait for installation to finish; choose shutdown, not reboot
   (or close the installer only after it has finished).
4. Return here. The tool will boot the disk without the ISO for you to verify.
Other Android images can have different menus or may not support auto install.
'@
            }
            $fwArgs = @{}; if ($Bliss) { $fwArgs = @{ UefiCodePath = $uefiCode; UefiVarsPath = $vars } }
            & (Join-Path $PSScriptRoot 'launch.ps1') -QemuDir $QemuDir -IsoPath $IsoPath -DiskPath $disk -Boot iso @fwArgs
            if ($LASTEXITCODE -ne 0) { throw 'Installer VM failed. Disk was retained; use -Resume after investigating.' }
            if ((Confirm-InstallationStage 'Did installation finish and the guest shut down cleanly? Approve to boot the disk.') -cne 'YES') { throw 'Not staged. Disk retained; use -Resume to boot it later.' }
        }
    }
    $json = & $img info --output=json $disk
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect disk.' }
    $info = $json | ConvertFrom-Json
    if ($info.format -ne 'qcow2' -or $null -ne $info.'backing-filename') { throw 'Disk must be a standalone QCOW2 image with no backing file.' }
    & $img check $disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk failed qemu-img check. It was not staged.' }
    Write-Host 'Booting the disk without an ISO. Verify Android starts, make permitted changes in the guest, then shut it down cleanly.'
    $fwArgs = @{}; if ($Bliss) { $fwArgs = @{ UefiCodePath = $uefiCode; UefiVarsPath = $vars } }
    & (Join-Path $PSScriptRoot 'launch.ps1') -QemuDir $QemuDir -DiskPath $disk -Boot disk @fwArgs
    if ($LASTEXITCODE -ne 0) { throw 'Disk boot failed. Disk was retained; use -Resume after investigating.' }
    if ((Confirm-InstallationStage 'Did Android boot and shut down cleanly, and are you ready to stage it?') -cne 'YES') { throw 'Not staged. Disk retained.' }
    & $img check $disk
    if ($LASTEXITCODE -ne 0) { throw 'Disk failed qemu-img check after boot. It was not staged.' }
    New-Item -ItemType Directory -Force $targetDir | Out-Null
    if ($Bliss) {
        # Stage firmware before the disk so an incomplete copy cannot be auto-booted.
        [IO.File]::Copy($uefiCode, $stagedCode, $false)
        [IO.File]::Copy($vars, $stagedVars, $false)
    }
    # Publish the base only after a complete copy. A failed copy cannot be booted.
    $temporary = Join-Path $targetDir ('.android-base-' + [guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.File]::Copy($disk, $temporary, $false)
        [IO.File]::Move($temporary, $target)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
    Write-Host "Prepared image staged at $target. Verify OS redistribution rights before sharing."
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
