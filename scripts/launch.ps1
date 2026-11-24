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
    [ValidateRange(1024, 65535)][int]$AdbPort = 5555,
    [string]$UefiCodePath = '',
    [string]$UefiVarsPath = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if ($env:OS -ne 'Windows_NT') { throw 'This launcher requires Windows.' }
    $feature = Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform
    if ($feature.State -ne 'Enabled') {
        throw 'Windows Hypervisor Platform is not enabled. Enable it as administrator and restart Windows (see README).'
    }
    if ([bool]$UefiCodePath -ne [bool]$UefiVarsPath) { throw 'Provide both UEFI code and a writable per-VM vars file.' }
    if ($UefiCodePath) {
        if (-not (Test-Path -LiteralPath $UefiCodePath -PathType Leaf) -or -not (Test-Path -LiteralPath $UefiVarsPath -PathType Leaf)) { throw 'UEFI firmware code or variables file not found.' }
        $codeFull = [IO.Path]::GetFullPath($UefiCodePath); $varsFull = [IO.Path]::GetFullPath($UefiVarsPath)
        if ($codeFull -ieq $varsFull -or $varsFull -ieq [IO.Path]::GetFullPath($DiskPath) -or ($IsoPath -and $varsFull -ieq [IO.Path]::GetFullPath($IsoPath))) { throw 'UEFI variables must be a separate file from code, disk and ISO.' }
    }
    $qemu = Join-Path $QemuDir 'qemu-system-x86_64.exe'
    $img = Join-Path $QemuDir 'qemu-img.exe'
    if (-not (Test-Path -LiteralPath $qemu -PathType Leaf)) { throw "QEMU not found: $qemu" }
    if ($Boot -eq 'iso' -and -not $IsoPath) { throw 'Specify -IsoPath when booting from ISO.' }
    if ($IsoPath -and -not (Test-Path -LiteralPath $IsoPath -PathType Leaf)) { throw "ISO not found: $IsoPath" }
    $firmware = [IO.Path]::GetFullPath((Join-Path $QemuDir '../share/qemu'))
    if (Test-Path -LiteralPath (Join-Path $firmware 'bios-256k.bin') -PathType Leaf) {
        foreach ($rom in @('bios-256k.bin', 'vgabios-stdvga.bin', 'kvmvapic.bin')) {
            $romPath = Join-Path $firmware $rom
            if (-not (Test-Path -LiteralPath $romPath -PathType Leaf) -or (Get-Item -LiteralPath $romPath).Length -le 0) {
                throw "Incomplete QEMU firmware bundle: $rom is missing or empty in $firmware. Extract the complete updated package; no VM was started."
            }
        }
    }
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
        '-name', 'Project Nexus', '-netdev', $net, '-device', 'virtio-net-pci,netdev=net0,romfile=')
    if (Test-Path -LiteralPath (Join-Path $firmware 'bios-256k.bin') -PathType Leaf) { $qemuArgs += @('-L', $firmware) }
    if ($UefiCodePath) {
        # QEMU's key=value syntax escapes literal commas by doubling them.
        $codeArg = $codeFull.Replace(',', ',,'); $varsArg = $varsFull.Replace(',', ',,')
        $qemuArgs += @('-machine', 'q35,vmport=off', '-drive', "if=pflash,format=raw,unit=0,readonly=on,file=$codeArg", '-drive', "if=pflash,format=raw,unit=1,file=$varsArg")
    }
    if ($IsoPath) { $qemuArgs += @('-cdrom', [IO.Path]::GetFullPath($IsoPath)) }
    Write-Host 'Starting QEMU with WHPX. Shut down Android before closing QEMU.'
    & $qemu @qemuArgs
    if ($LASTEXITCODE -ne 0) { throw "QEMU exited with code $LASTEXITCODE. If WHPX reports MMIO/virtual-processor emulation failures, preserve this log and report Windows build, CPU model and Bliss ISO version. Hardware acceleration being operational does not guarantee guest compatibility." }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
