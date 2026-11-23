param([Parameter(Mandatory=$true)][string]$Stub, [string]$Case = 'all')
$ErrorActionPreference = 'Stop'
function Get-WindowsOptionalFeature { param([switch]$Online, [string]$FeatureName) [pscustomobject]@{ State = 'Enabled' } }
function Read-Host { param([string]$Prompt) return 'YES' }
$studio = Join-Path $PSScriptRoot '../scripts/prepare-android.ps1'
$root = $env:NEXUS_STUDIO_ROOT
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
    $global:LASTEXITCODE = 0
    Write-Host 'Image studio contract tests passed (stubbed, no Android).'
} finally { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
