# Diagnostic candidate only. Not a certified Nexus Player image or accelerated gaming configuration.
[CmdletBinding()]
param([string]$QemuExe = '')
$ErrorActionPreference = 'Stop'
try {
    if (-not $QemuExe) {
        Add-Type -AssemblyName System.Windows.Forms
        $picker = New-Object System.Windows.Forms.OpenFileDialog
        $picker.Title = 'Select qemu-system-x86_64.exe from the complete Nexus Developer package'
        $picker.Filter = 'QEMU system executable|qemu-system-x86_64.exe'
        if ($picker.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) { return }
        $QemuExe = $picker.FileName
    }
    if (-not (Test-Path -LiteralPath $QemuExe -PathType Leaf)) { throw 'QEMU executable not found.' }
    $firmware = [IO.Path]::GetFullPath((Join-Path (Split-Path $QemuExe) '../share/qemu'))
    foreach ($rom in @('vgabios-virtio.bin', 'kvmvapic.bin')) {
        if (-not (Test-Path -LiteralPath (Join-Path $firmware $rom) -PathType Leaf)) { throw "Missing QEMU ROM $rom. Extract the complete updated Developer package." }
    }
    $qemuImg = Join-Path (Split-Path $QemuExe) 'qemu-img.exe'
    if (-not (Test-Path -LiteralPath $qemuImg -PathType Leaf)) { throw 'Use the complete Developer package including qemu-img.exe and dependencies.' }
    foreach ($name in @('android-candidate.qcow2', 'uefi-code.fd', 'uefi-vars-template.fd')) {
        if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $name) -PathType Leaf)) { throw "Missing package file: $name" }
    }
    # A new per-launch instance avoids writing the immutable base or mixing firmware states.
    $instance = Join-Path $env:LOCALAPPDATA ('ProjectNexus/experimental/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $instance | Out-Null
    $base = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot 'android-candidate.qcow2')).Path
    $overlay = Join-Path $instance 'user.qcow2'
    $vars = Join-Path $instance 'uefi-vars.fd'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'uefi-vars-template.fd') -Destination $vars
    & $qemuImg create -f qcow2 -F qcow2 -b $base $overlay
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the diagnostic overlay. Base was not modified.' }
    $codeArg = (Join-Path $PSScriptRoot 'uefi-code.fd').Replace(',', ',,')
    $varsArg = $vars.Replace(',', ',,')
    $overlayArg = $overlay.Replace(',', ',,')
    Write-Warning 'Experimental Linux-tested configuration. Windows compatibility and performance are NOT certified. No host GPU acceleration. This launcher does not expose guest ADB to the host.'
    Write-Host "Instance retained at: $instance"
    Write-Host 'Each launch starts a fresh test instance. Do not delete/move the base while using an overlay. Shut Android down cleanly.'
    & $QemuExe -L $firmware -machine 'q35,vmport=off' -accel 'tcg,thread=multi' -cpu 'max,avx=off,avx2=off,f16c=off,fma=off' -smp 2 -m 3072 -vga none -device virtio-vga -usb -device usb-tablet -drive "file=$overlayArg,format=qcow2" -drive "if=pflash,format=raw,readonly=on,file=$codeArg" -drive "if=pflash,format=raw,file=$varsArg" -boot order=c -netdev user,id=net0 -device virtio-net-pci,netdev=net0,romfile= -name 'Nexus experimental Android candidate'
    if ($LASTEXITCODE -ne 0) { throw "QEMU exited with code $LASTEXITCODE. Instance and base retained; do not reinstall." }
} catch {
    Write-Error $_.Exception.Message
    exit 1
}
