param(
    [Parameter(Mandatory=$true)][string]$Stub,
    [string]$Case = 'all'
)
$ErrorActionPreference = 'Stop'
$launcher = Join-Path $PSScriptRoot '../scripts/launch.ps1'
# The CI runner need not have WHPX; test argument and filesystem behavior with a stub.
function Get-WindowsOptionalFeature { param([switch]$Online, [string]$FeatureName) [pscustomobject]@{ State = 'Enabled' } }
if ($Case -eq 'missing-rom') {
    $broken = Join-Path $env:NEXUS_TEST_DIR 'broken-engine'
    New-Item -ItemType Directory -Path (Join-Path $broken 'bin'), (Join-Path $broken 'share/qemu') | Out-Null
    Copy-Item $Stub (Join-Path $broken 'bin/qemu-system-x86_64.exe')
    Copy-Item $Stub (Join-Path $broken 'bin/qemu-img.exe')
    Set-Content (Join-Path $broken 'share/qemu/bios-256k.bin') 'fixture BIOS'
    Set-Content (Join-Path $broken 'share/qemu/vgabios-stdvga.bin') 'fixture VGA'
    & $launcher -QemuDir (Join-Path $broken 'bin') -DiskPath (Join-Path $broken 'must-not-exist.qcow2') -IsoPath $env:NEXUS_TEST_ISO -Boot iso
    exit $LASTEXITCODE
}
if ($Case -eq 'missing-disk') {
    & $launcher -QemuDir $env:NEXUS_TEST_DIR -DiskPath (Join-Path $env:NEXUS_TEST_DIR 'missing.qcow2') -Boot disk
    exit $LASTEXITCODE
}
if ($Case -eq 'same-image') {
    & $launcher -QemuDir $env:NEXUS_TEST_DIR -DiskPath $env:NEXUS_TEST_ISO -IsoPath $env:NEXUS_TEST_ISO -Boot iso
    exit $LASTEXITCODE
}
$root = Join-Path $env:RUNNER_TEMP ('nexus tests ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
try {
    Copy-Item $Stub (Join-Path $root 'qemu-system-x86_64.exe')
    Copy-Item $Stub (Join-Path $root 'qemu-img.exe')
    $env:NEXUS_TEST_DIR = $root
    $env:NEXUS_TEST_ISO = Join-Path $root 'stock image.iso'
    $env:NEXUS_TEST_LOG = Join-Path $root 'args.log'
    $disk = Join-Path $root 'virtual disk.qcow2'
    Set-Content -LiteralPath $env:NEXUS_TEST_ISO -Value 'test ISO'
    & $launcher -QemuDir $root -IsoPath $env:NEXUS_TEST_ISO -DiskPath $disk -Boot iso -MemoryMB 2048 -Cpus 2 -ForwardAdb -AdbPort 5556
    if (-not (Test-Path -LiteralPath $disk)) { throw 'Missing created disk' }
    $argsLog = Get-Content -LiteralPath $env:NEXUS_TEST_LOG
    foreach ($item in @('create', 'qcow2', $disk, '32G', 'whpx,kernel-irqchip=off', '2048', '2', 'order=d', $env:NEXUS_TEST_ISO, 'user,id=net0,hostfwd=tcp:127.0.0.1:5556-:5555')) {
        if ($argsLog -cnotcontains $item) { throw "Missing argument: $item" }
    }
    Remove-Item $env:NEXUS_TEST_LOG
    & $launcher -QemuDir $root -DiskPath $disk -Boot disk
    $argsLog = Get-Content $env:NEXUS_TEST_LOG
    if ($argsLog -cnotcontains 'order=c' -or $argsLog -cnotcontains 'user,id=net0' -or $argsLog -contains '-cdrom' -or $argsLog -contains 'create') { throw 'Disk boot arguments are incorrect' }
    foreach ($testCase in @('missing-disk', 'same-image', 'missing-rom')) {
        & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -Stub $Stub -Case $testCase
        if ($LASTEXITCODE -ne 1) { throw "Expected failure for $testCase, got $LASTEXITCODE" }
    }
    if (Test-Path (Join-Path $root 'broken-engine/must-not-exist.qcow2')) { throw 'Missing ROM must be caught before disk creation.' }
    $global:LASTEXITCODE = 0
    Write-Host 'Launcher contract tests passed.'
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
