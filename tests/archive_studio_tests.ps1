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
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    function Make-Zip([string]$Name, [string[]]$Entries) {
        $zip = [IO.Compression.ZipFile]::Open((Join-Path $root "$Name.zip"), [IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($entryName in $Entries) {
                $entry = $zip.CreateEntry($entryName)
                $writer = [IO.StreamWriter]::new($entry.Open())
                try { $writer.Write('test payload') } finally { $writer.Dispose() }
            }
        } finally { $zip.Dispose() }
    }
    Make-Zip 'ambiguous' @('a.iso','b.iso')
    Make-Zip 'loose' @('kernel','initrd.img','system.img')
    foreach ($name in @('ambiguous','loose')) {
        & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $PSCommandPath -Stub $Stub -Case $name
        if ($LASTEXITCODE -ne 1 -or (Test-Path (Join-Path $root "work-$name/android-install.qcow2"))) { throw "Unsafe ZIP was accepted: $name" }
    }
    # Traversal-shaped paths are not used as extraction destinations.
    Make-Zip 'nested' @('../../outside.iso','notes/readme.txt')
    & $backend -Mode Install -Source (Join-Path $root 'nested.zip') -Workspace (Join-Path $root 'work-nested') -PlayerFolder (Join-Path $root 'player') -QemuDir $root
    if ($LASTEXITCODE -ne 0) { throw 'ZIP install contract failed.' }
    if (-not (Test-Path (Join-Path $root 'player/images/android-base.qcow2'))) { throw 'Fixed output name was not published.' }
    if (Test-Path (Join-Path $root 'outside.iso')) { throw 'Archive path traversal escaped extraction directory.' }
    if (@(Get-ChildItem (Join-Path $root 'work-nested') -Directory -Filter '.unpack-*').Count -ne 0) { throw 'Temporary extraction was not cleaned up.' }
    $global:LASTEXITCODE = 0
    Write-Host 'Archive selection, loose/ambiguous rejection, safe extraction and fixed-name staging passed (stubbed, no Android).'
} finally { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
