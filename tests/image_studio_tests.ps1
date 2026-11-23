param([Parameter(Mandatory=$true)][string]$Stub, [string]$Case = 'all')
$ErrorActionPreference = 'Stop'
function Get-WindowsOptionalFeature { param([switch]$Online, [string]$FeatureName) [pscustomobject]@{ State = 'Enabled' } }
function Read-Host { param([string]$Prompt) return 'YES' }
$studio = Join-Path $PSScriptRoot '../scripts/prepare-android.ps1'
$root = $env:NEXUS_STUDIO_ROOT
if ($Case -eq 'bad-hash') {
    & $studio -Bliss -QemuDir (Join-Path $root 'engine/bin') -IsoPath (Join-Path $root 'stock.iso') -DiskPath (Join-Path $root 'bad.qcow2') -PlayerFolder (Join-Path $root 'bliss-player') -ExpectedSha256 ('0' * 64)
    exit $LASTEXITCODE
}
if ($Case -eq 'refuse-existing') {
    & $studio -QemuDir $root -IsoPath (Join-Path $root 'stock.iso') -DiskPath (Join-Path $root 'new.qcow2') -PlayerFolder (Join-Path $root 'player')
    exit $LASTEXITCODE
}
$root = Join-Path $env:RUNNER_TEMP ('nexus-studio-' + [guid]::NewGuid().ToString('N'))
$env:NEXUS_STUDIO_ROOT = $root
$env:NEXUS_TEST_LOG = Join-Path $root 'args.log'
New-Item -ItemType Directory -Path $root, (Join-Path $root 'player') | Out-Null
try {
    Copy-Item $Stub (Join-Path $root 'qemu-system-x86_64.exe')
    Copy-Item $Stub (Join-Path $root 'qemu-img.exe')
    Set-Content (Join-Path $root 'player/ProjectNexus.exe') 'player'
    Set-Content (Join-Path $root 'stock.iso') 'iso'
    $disk = Join-Path $root 'new.qcow2'; $player = Join-Path $root 'player'; $iso = Join-Path $root 'stock.iso'
    & $studio -QemuDir $root -IsoPath $iso -DiskPath $disk -PlayerFolder $player
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $player 'images/android-base.qcow2'))) { throw 'Image installation and staging contract failed.' }
    $argsLog = Get-Content $env:NEXUS_TEST_LOG
    foreach ($item in @('order=d','order=c','check','info')) { if ($argsLog -cnotcontains $item) { throw "Missing $item" } }
    & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -Stub $Stub -Case refuse-existing
    if ($LASTEXITCODE -ne 1) { throw 'Existing staged base was not protected.' }
    Remove-Item (Join-Path $player 'images/android-base.qcow2')
    $copy = Join-Path $root 'copy.qcow2'
    & $studio -QemuDir $root -CopyFrom $disk -DiskPath $copy -PlayerFolder $player
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $copy) -or -not (Test-Path (Join-Path $player 'images/android-base.qcow2'))) { throw 'Copy and staging contract failed.' }
    Remove-Item (Join-Path $player 'images/android-base.qcow2')
    & $studio -QemuDir $root -Resume -DiskPath $copy -PlayerFolder $player
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $player 'images/android-base.qcow2'))) { throw 'Resume contract failed.' }
    $engine = Join-Path $root 'engine/bin'; $share = Join-Path $root 'engine/share/qemu'; $blissPlayer = Join-Path $root 'bliss-player'
    New-Item -ItemType Directory -Path $engine, $share, $blissPlayer | Out-Null
    Copy-Item $Stub (Join-Path $engine 'qemu-system-x86_64.exe'); Copy-Item $Stub (Join-Path $engine 'qemu-img.exe')
    Set-Content (Join-Path $share 'edk2-x86_64-code.fd') 'UEFI code'; Set-Content (Join-Path $share 'edk2-i386-vars.fd') 'UEFI variables'
    Set-Content (Join-Path $blissPlayer 'ProjectNexus.exe') 'player'
    & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -Stub $Stub -Case bad-hash
    if ($LASTEXITCODE -ne 1 -or (Test-Path (Join-Path $root 'bad.qcow2')) -or (Test-Path (Join-Path $root 'bad.qcow2.uefi-vars.fd'))) { throw 'Wrong Bliss checksum must not create disk or variables.' }
    $blissDisk = Join-Path $root 'bliss image.qcow2'; $hash = (Get-FileHash -Algorithm SHA256 $iso).Hash
    & $studio -Bliss -QemuDir $engine -IsoPath $iso -DiskPath $blissDisk -PlayerFolder $blissPlayer -ExpectedSha256 $hash
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $blissPlayer 'images/android-base.qcow2')) -or -not (Test-Path (Join-Path $blissPlayer 'images/android-uefi-code.fd')) -or -not (Test-Path (Join-Path $blissPlayer 'images/android-uefi-vars.fd'))) { throw 'Bliss UEFI staging failed.' }
    $argsLog = Get-Content $env:NEXUS_TEST_LOG
    foreach ($item in @('q35,vmport=off', ('if=pflash,format=raw,unit=0,readonly=on,file=' + (Join-Path $share 'edk2-x86_64-code.fd')), ('if=pflash,format=raw,unit=1,file=' + ($blissDisk + '.uefi-vars.fd')))) {
        if ($argsLog -cnotcontains $item) { throw "Missing Bliss argument: $item" }
    }
    $global:LASTEXITCODE = 0
    Write-Host 'Image studio contract tests passed (stubbed, no Android).'
} finally { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
