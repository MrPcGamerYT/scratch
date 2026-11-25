param([Parameter(Mandatory=$true)][string]$Stub, [string]$Case = '')
$ErrorActionPreference = 'Stop'
function Get-WindowsOptionalFeature { param([switch]$Online, [string]$FeatureName) [pscustomobject]@{ State = 'Enabled' } }
function Read-Host { param([string]$Prompt) return 'YES' }
$backend = Join-Path $PSScriptRoot '../scripts/image-studio.ps1'
if ($Case) {
    $root = $env:NEXUS_ARCHIVE_ROOT
    & $backend -Mode Install -Source (Join-Path $root "$Case.zip") -Workspace (Join-Path $root "work-$Case") -PlayerFolder (Join-Path $root 'player') -QemuDir $root
    exit $LASTEXITCODE
}
$root = Join-Path $env:RUNNER_TEMP ('nexus-archives-' + [guid]::NewGuid().ToString('N'))
$env:NEXUS_ARCHIVE_ROOT = $root
$env:NEXUS_TEST_LOG = Join-Path $root 'args.log'
New-Item -ItemType Directory -Path $root, (Join-Path $root 'player') | Out-Null
try {
    Copy-Item $Stub (Join-Path $root 'qemu-system-x86_64.exe')
    Copy-Item $Stub (Join-Path $root 'qemu-img.exe')
    Set-Content (Join-Path $root 'player/ProjectNexus.exe') 'stub player'
    foreach ($name in @('archive','payload')) {
        Set-Content (Join-Path $root "$name.zip") 'not an ISO'
        & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -Stub $Stub -Case $name
        if ($LASTEXITCODE -ne 1 -or (Test-Path (Join-Path $root "work-$name/android-install.qcow2"))) { throw "Non-ISO source was accepted: $name" }
    }
    Set-Content (Join-Path $root 'bliss.iso') 'synthetic ISO'
    & $backend -Mode Install -Acceleration tcg -Graphics virtio -Source (Join-Path $root 'bliss.iso') -Workspace (Join-Path $root 'work-iso') -PlayerFolder (Join-Path $root 'player') -QemuDir $root
    if ($LASTEXITCODE -ne 0) { throw 'ISO installation contract failed.' }
    if (-not (Test-Path (Join-Path $root 'player/images/android-base.qcow2'))) { throw 'Fixed output name was not published.' }
    if ((Get-Content (Join-Path $root 'player/images/acceleration.txt') -Raw).Trim() -ne 'tcg') { throw 'Prepared player did not retain software mode.' }
    if (@(Get-Content $env:NEXUS_TEST_LOG | Where-Object { $_ -ceq 'tcg,thread=multi' }).Count -ne 2) { throw 'ISO installer and disk verification must both use TCG.' }
    if ((Get-Content (Join-Path $root 'player/images/graphics.txt') -Raw).Trim() -ne 'virtio') { throw 'Prepared graphics choice was lost.' }
    if (@(Get-Content $env:NEXUS_TEST_LOG | Where-Object { $_ -ceq 'virtio-vga,id=nexus_display' }).Count -ne 2) { throw 'Installer and verification must both use VirtIO display.' }
    $global:LASTEXITCODE = 0
    Write-Host 'ISO-only validation, non-ISO rejection and fixed-name staging passed (stubbed, no Android).'
} finally { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
