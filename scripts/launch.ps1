# Start a single Android-x86 VM with explicit WHPX or TCG. See README.md for prerequisites.
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
    [string]$UefiVarsPath = '',
    [ValidateSet('whpx','tcg')][string]$Acceleration = 'whpx',
    [ValidateSet('std','virtio')][string]$Graphics = 'std'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if ($env:OS -ne 'Windows_NT') { throw 'This launcher requires Windows.' }
    if ($Acceleration -eq 'whpx') {
        $feature = Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform
        if ($feature.State -ne 'Enabled') { throw 'Windows Hypervisor Platform is not enabled. Enable it and restart Windows, or explicitly select Software compatibility (TCG) in Image Studio.' }
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
        $roms = @('bios-256k.bin', 'vgabios-stdvga.bin', 'kvmvapic.bin')
        if ($Graphics -eq 'virtio') { $roms += 'vgabios-virtio.bin' }
        foreach ($rom in $roms) {
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
    # Studio has no manual resource controls: allocate automatically when a value
    # was not explicitly supplied. Keep this policy aligned with nexus_resource_budget.
    $hostMemoryMB = 0L
    $hostCpu = @(); $hostGpu = @(); $hostOs = $null
    try {
        $computer = Get-CimInstance Win32_ComputerSystem -ErrorAction Stop
        $hostMemoryMB = [long]([math]::Floor($computer.TotalPhysicalMemory / 1MB))
        $hostCpu = @(Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors)
        $hostGpu = @(Get-CimInstance Win32_VideoController -ErrorAction Stop | Select-Object Name, DriverVersion, Status)
        $hostOs = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop | Select-Object Caption, Version, BuildNumber
    } catch { Write-Warning 'Some host inventory is unavailable; conservative defaults will be used for unknown resources.' }
    if (-not $PSBoundParameters.ContainsKey('Cpus')) {
        $Cpus = [int][math]::Max(1, [math]::Min(8, [math]::Floor([Environment]::ProcessorCount / 2)))
    }
    if (-not $PSBoundParameters.ContainsKey('MemoryMB')) {
        $MemoryMB = if ($hostMemoryMB -gt 0) {
            [int][math]::Min([math]::Max(512, [math]::Min(8192, [math]::Floor($hostMemoryMB / 4))), [math]::Max(512, [math]::Floor($hostMemoryMB / 2)))
        } else { 2048 }
    }
    Write-Host "Next launch: $Cpus CPUs / $MemoryMB MB RAM. Automatic values reserve host headroom; they are not benchmark-optimal presets."
    if ($MemoryMB -lt 2048) { Write-Warning 'Allocated RAM is below 2 GiB. This Android image may not boot reliably; the host may be unsuitable.' }
    $order = if ($Boot -eq 'iso') { 'd' } else { 'c' }
    $net = 'user,id=net0'
    if ($ForwardAdb) { $net += ",hostfwd=tcp:127.0.0.1:${AdbPort}-:5555" }
    $accelerator = if ($Acceleration -eq 'tcg') { 'tcg,thread=multi' } else { 'whpx,kernel-irqchip=off' }
    $qemuArgs = @('-accel', $accelerator, '-m', "$MemoryMB", '-smp', "$Cpus",
        '-vga', $(if ($Graphics -eq 'virtio') { 'none' } else { 'std' }), '-usb', '-device', 'usb-tablet', '-hda', $diskFull, '-boot', "order=$order",
        '-name', 'Project Nexus', '-netdev', $net, '-device', 'virtio-net-pci,netdev=net0,romfile=')
    if (Test-Path -LiteralPath (Join-Path $firmware 'bios-256k.bin') -PathType Leaf) { $qemuArgs += @('-L', $firmware) }
    if ($UefiCodePath) {
        # QEMU's key=value syntax escapes literal commas by doubling them.
        $codeArg = $codeFull.Replace(',', ',,'); $varsArg = $varsFull.Replace(',', ',,')
        $qemuArgs += @('-machine', 'q35,vmport=off', '-drive', "if=pflash,format=raw,unit=0,readonly=on,file=$codeArg", '-drive', "if=pflash,format=raw,unit=1,file=$varsArg")
    }
    if ($Graphics -eq 'virtio') { $qemuArgs += @('-device', 'virtio-vga,id=nexus_display') }
    Write-Host "Display device: $Graphics (no host GPU acceleration)."
    if ($Acceleration -eq 'tcg') { $qemuArgs += @('-cpu', 'max') }
    if ($IsoPath) { $qemuArgs += @('-cdrom', [IO.Path]::GetFullPath($IsoPath)) }
    Write-Host "Starting QEMU with $Acceleration. Shut down Android before closing QEMU."
    if ($Acceleration -eq 'tcg') { Write-Warning 'Software CPU emulation selected explicitly. Installation/boot can be very slow; this bypasses WHPX but does not guarantee Bliss compatibility.' }
    # Unique report per attempt, never overwrite the installed disk or past reports.
    # Contains local paths and host identifiers: review before sharing.
    $reportDirectory = Join-Path $diskParent 'diagnostics'
    try {
        New-Item -ItemType Directory -Force -Path $reportDirectory | Out-Null
        $reportPath = Join-Path $reportDirectory (('launch-{0}-{1}.json' -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'), [guid]::NewGuid().ToString('N')))
        $report = [ordered]@{
            schema = 1; timestampUtc = [DateTime]::UtcNow.ToString('o'); state = 'starting'
            acceleration = $Acceleration; graphics = $Graphics; acceleratedGuestGpu = $false
            cpus = $Cpus; memoryMB = $MemoryMB; hostMemoryMB = $hostMemoryMB
            automaticCpus = (-not $PSBoundParameters.ContainsKey('Cpus'))
            automaticMemory = (-not $PSBoundParameters.ContainsKey('MemoryMB'))
            processors = $hostCpu; graphicsAdapters = $hostGpu; windows = $hostOs
            arguments = $qemuArgs
            note = 'Inventory is not GPU activation or a successful Android boot test. Review local paths before sharing.'
        }
        $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding UTF8
        Write-Host "Launch diagnostics: $reportPath"
    } catch { $reportPath = $null; Write-Warning 'Could not save launch diagnostics; VM launch will continue.' }
    & $qemu @qemuArgs
    $qemuExitCode = $LASTEXITCODE
    if ($reportPath) {
        try {
            $report.state = 'exited'; $report['exitCode'] = $qemuExitCode
            $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding UTF8
        } catch { Write-Warning 'Could not update diagnostics with the QEMU exit code.' }
    }
    $global:LASTEXITCODE = $qemuExitCode
    if ($LASTEXITCODE -ne 0) { throw "QEMU exited with code $LASTEXITCODE. If WHPX reports MMIO/virtual-processor emulation failures, preserve this log and report Windows build, CPU model and Bliss ISO version. Hardware acceleration being operational does not guarantee guest compatibility. Try Software compatibility (TCG) explicitly in a fresh installation workspace; no automatic retry or fallback was performed." }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
