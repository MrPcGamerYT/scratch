# Desktop Image Studio backend. Output names intentionally match MainWindow.
[CmdletBinding()]
param(
    [ValidateSet('Install','Import','Resume')][string]$Mode = 'Install',
    [string]$Source = '',
    [Parameter(Mandatory=$true)][string]$Workspace,
    [Parameter(Mandatory=$true)][string]$PlayerFolder,
    [string]$QemuDir = (Join-Path $PSScriptRoot 'qemu/bin'),
    [switch]$Bliss,
    [string]$SourceVars = '',
    [string]$ExpectedSha256 = '',
    [string]$ApprovalDirectory = ''
)
$ErrorActionPreference = 'Stop'
$temporary = $null
try {
    $workspaceFull = [IO.Path]::GetFullPath($Workspace)
    New-Item -ItemType Directory -Force -Path $workspaceFull | Out-Null
    $disk = Join-Path $workspaceFull 'android-install.qcow2'
    $target = Join-Path ([IO.Path]::GetFullPath($PlayerFolder)) 'images/android-base.qcow2'
    if (-not (Test-Path -LiteralPath (Join-Path $PlayerFolder 'ProjectNexus.exe') -PathType Leaf)) { throw 'Choose the player folder containing ProjectNexus.exe.' }
    if (Test-Path -LiteralPath $target) { throw 'The player already has images/android-base.qcow2. It will not be overwritten. Back up and move the previous installation yourself first.' }
    $arguments = @{ DiskPath = $disk; PlayerFolder = $PlayerFolder; QemuDir = $QemuDir; Bliss = $Bliss; ApprovalDirectory = $ApprovalDirectory }
    if ($Mode -eq 'Resume') {
        $arguments.Resume = $true
    } else {
        if (Test-Path -LiteralPath $disk) { throw 'This workspace already contains android-install.qcow2. Use Resume or choose an empty workspace.' }
        if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) { throw 'Select an existing source file.' }
        if ($ExpectedSha256) {
            if ($ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'SHA-256 must contain 64 hexadecimal characters.' }
            if ((Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash -ine $ExpectedSha256) { throw 'Source SHA-256 mismatch. Nothing installed.' }
        }
        $payload = [IO.Path]::GetFullPath($Source)
        if ([IO.Path]::GetExtension($payload) -ieq '.zip') {
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $temporary = Join-Path $workspaceFull ('.unpack-' + [guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path $temporary | Out-Null
            $archive = [IO.Compression.ZipFile]::OpenRead($payload)
            try {
                $extensions = if ($Mode -eq 'Install') { @('.iso') } else { @('.qcow2','.img','.raw','.vhd','.vhdx') }
                $candidates = @($archive.Entries | Where-Object { $_.Name -and [IO.Path]::GetExtension($_.Name).ToLowerInvariant() -in $extensions })
                if ($candidates.Count -ne 1) {
                    throw "ZIP must contain exactly one $Mode payload ($($extensions -join ', ')); found $($candidates.Count). Loose kernel/initrd/system.img/data.img packages are not bootable whole disks. Select Import only for a preinstalled whole-disk image."
                }
                $entry = $candidates[0]
                if ($entry.Length -gt 128GB -or $entry.Length -le 0 -or ($entry.CompressedLength -gt 0 -and $entry.Length / $entry.CompressedLength -gt 1000)) { throw 'Archive payload exceeds extraction safety limits.' }
                $drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($workspaceFull))
                if ($drive.AvailableFreeSpace -lt ($entry.Length + 1GB)) { throw 'Not enough free workspace space to unpack this image.' }
                # Never use the entry path: extract just one payload to a generated name.
                # This prevents traversal, symlink and Windows alternate-stream filenames.
                $payload = Join-Path $temporary ('payload' + [IO.Path]::GetExtension($entry.Name).ToLowerInvariant())
                Write-Output "Extracting $($entry.Name) ($($entry.Length) bytes)..."
                $inputStream = $entry.Open()
                try {
                    $outputStream = [IO.File]::Open($payload, [IO.FileMode]::CreateNew)
                    try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() }
                } finally { $inputStream.Dispose() }
            } finally { $archive.Dispose() }
        }
        if ($Mode -eq 'Install') {
            if ([IO.Path]::GetExtension($payload) -ine '.iso') { throw 'Install requires an ISO or a ZIP containing one ISO. For installed whole-disk images choose Import.' }
            $arguments.IsoPath = $payload
        } else {
            $img = Join-Path $QemuDir 'qemu-img.exe'
            $json = & $img info --output=json $payload
            if ($LASTEXITCODE -ne 0) { throw 'qemu-img could not inspect source disk.' }
            $info = $json | ConvertFrom-Json
            if ($null -ne $info.'backing-filename' -or $null -ne $info.'data-file' -or $info.format -notin @('qcow2','raw','vpc','vhdx')) { throw 'Only self-contained disks are accepted; external backing/data files are not allowed.' }
            if ($info.format -eq 'raw') {
                $stream = [IO.File]::OpenRead($payload)
                try { $header = New-Object byte[] 512; $read = $stream.Read($header,0,512) } finally { $stream.Dispose() }
                if ($read -ne 512 -or $header[510] -ne 0x55 -or $header[511] -ne 0xAA) { throw 'Raw image has no whole-disk boot signature. A system.img partition cannot be imported as a bootable disk.' }
            }
            if (-not $temporary) { $temporary = Join-Path $workspaceFull ('.convert-' + [guid]::NewGuid().ToString('N')); New-Item -ItemType Directory -Path $temporary | Out-Null }
            $converted = Join-Path $temporary 'standalone.qcow2'
            & $img convert -p -f $info.format -O qcow2 $payload $converted
            if ($LASTEXITCODE -ne 0) { throw 'Disk conversion failed. Original source was not modified.' }
            $arguments.CopyFrom = $converted
            if ($SourceVars) { $arguments.SourceVars = $SourceVars }
        }
    }
    Write-Output "Fixed player output: $target"
    Write-Output 'Only virtual image files are attached. Complete installation in the VM and shut down cleanly before approving each stage.'
    & (Join-Path $PSScriptRoot 'prepare-android.ps1') @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Preparation stopped. Review the log; the installation workspace is retained for Resume.' }
    Write-Output "SUCCESS: $target"
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if ($temporary -and (Test-Path -LiteralPath $temporary)) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
