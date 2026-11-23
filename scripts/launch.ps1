# Start a single Android-x86 VM with WHPX. See README.md for prerequisites.
[CmdletBinding()]
param(
    [string]$QemuDir = 'C:\Program Files\qemu',
    [string]$IsoPath = '',
    [Parameter(Mandatory = $true)][string]$DiskPath,
    [ValidateSet('iso', 'disk')][string]$Boot = 'iso',
    [ValidateRange(512, 131072)][int]$MemoryMB = 4096,
    [ValidateRange(1, 64)][int]$Cpus = 4,
    [switch]$ForwardAdb,
    [ValidateRange(1024, 65535)][int]$AdbPort = 5555
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if ($env:OS -ne 'Windows_NT') { throw 'This launcher requires Windows.' }
    $feature = Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform
    if ($feature.State -ne 'Enabled') {
        throw 'Windows Hypervisor Platform is not enabled. Enable it as administrator and restart Windows (see README).'
    }
    $qemu = Join-Path $QemuDir 'qemu-system-x86_64.exe'
    $img = Join-Path $QemuDir 'qemu-img.exe'
    if (-not (Test-Path -LiteralPath $qemu -PathType Leaf)) { throw "QEMU not found: $qemu" }
    if ($Boot -eq 'iso' -and -not $IsoPath) { throw 'Specify -IsoPath when booting from ISO.' }
    if ($IsoPath -and -not (Test-Path -LiteralPath $IsoPath -PathType Leaf)) { throw "ISO not found: $IsoPath" }
    $diskFull = [IO.Path]::GetFullPath($DiskPath)
    $diskParent = [IO.Path]::GetDirectoryName($diskFull)
    if (-not (Test-Path -LiteralPath $diskParent -PathType Container)) { throw "Disk directory does not exist: $diskParent" }
    if ($IsoPath -and ([IO.Path]::GetFullPath($IsoPath) -ieq $diskFull)) { throw 'The ISO and virtual disk must be different files.' }
    if ((Test-Path -LiteralPath $diskFull) -and -not (Test-Path -LiteralPath $diskFull -PathType Leaf)) { throw "Disk is not a file: $diskFull" }
    if (-not (Test-Path -LiteralPath $diskFull -PathType Leaf)) {
        if ($Boot -eq 'disk') { throw 'Disk does not exist. Boot from an ISO to create and install it first.' }
        if (-not (Test-Path -LiteralPath $img -PathType Leaf)) { throw "qemu-img not found: $img" }
        Write-Host 'Creating a 32 GiB sparse QCOW2 disk...'
        & $img create -f qcow2 $diskFull 32G
        if ($LASTEXITCODE -ne 0) { throw "qemu-img failed with exit code $LASTEXITCODE" }
    }
    $order = if ($Boot -eq 'iso') { 'd' } else { 'c' }
    $net = 'user,id=net0'
    if ($ForwardAdb) { $net += ",hostfwd=tcp:127.0.0.1:${AdbPort}-:5555" }
    $qemuArgs = @('-accel', 'whpx,kernel-irqchip=off', '-m', "$MemoryMB", '-smp', "$Cpus",
        '-vga', 'std', '-usb', '-device', 'usb-tablet', '-hda', $diskFull, '-boot', "order=$order",
        '-name', 'Project Nexus', '-netdev', $net, '-device', 'virtio-net-pci,netdev=net0')
    if ($IsoPath) { $qemuArgs += @('-cdrom', [IO.Path]::GetFullPath($IsoPath)) }
    Write-Host 'Starting QEMU with WHPX. Shut down Android before closing QEMU.'
    & $qemu @qemuArgs
    if ($LASTEXITCODE -ne 0) { throw "QEMU exited with code $LASTEXITCODE" }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
