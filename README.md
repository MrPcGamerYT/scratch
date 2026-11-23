# Project Nexus

A first Windows prototype for running an Android-x86-compatible guest in QEMU with Windows Hypervisor Platform (WHPX). It provides a standalone PowerShell launcher and an optional Qt 6 desktop shell that downloads a stock Android-x86 image, starts QEMU and attempts to embed its native window. This is **not a finished BlueStacks alternative**: it does not include an Android image, keymapping, multi-instance management, FPS controls, ARM translation, Google Play, or a GPU acceleration stack.

## Requirements

- Windows 10/11 x64, hardware virtualization enabled in BIOS/UEFI, and Windows Hypervisor Platform enabled. A reboot may be required. In an **administrator** PowerShell:

  ```powershell
  Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All
  ```

  Check with `Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform`. QEMU reports an error if WHPX cannot initialize; the launcher does not silently fall back to software emulation. Virtual Machine Platform is not required specifically for QEMU WHPX.
- A compatible Windows x64 QEMU distribution with `qemu-system-x86_64.exe` and `qemu-img.exe` in the same directory. Install or extract it yourself from a trusted distributor.
- An **x86_64** Android-x86 or compatible ISO. The Qt shell can download the unmodified Android-x86 9.0-r2 image, or you can supply your own. Android-x86 is an independently maintained x86 port of Android, not an official Google emulator image; Nexus does not modify it. Images differ in hardware, installer, licensing and native-bridge support; no particular image is guaranteed to boot or run games. This project does not redistribute Android or proprietary translation libraries. Android 9 is old and may be unpatched: do not use it for sensitive accounts or data.
- Enough free storage for the guest, and a folder for the disk. The new 32 GiB QCOW2 disk is sparse but can grow toward its maximum size.

## Quick start: PowerShell

From this repository in a normal Windows PowerShell, with paths adjusted:

```powershell
.\scripts\launch.ps1 -QemuDir 'C:\Program Files\qemu' `
  -IsoPath 'C:\AndroidEmulator\android.iso' `
  -DiskPath 'C:\AndroidEmulator\android.qcow2' -Boot iso
```

The folder containing the disk must already exist. A missing disk is created only when booting from ISO. Boot the live system or use the image's installer to install to the virtual disk. **Installing to the virtual disk is separate from simply booting a live ISO.** When installation is complete, shut down the guest, then boot the disk:

```powershell
.\scripts\launch.ps1 -QemuDir 'C:\Program Files\qemu' `
  -DiskPath 'C:\AndroidEmulator\android.qcow2' -Boot disk
```

Options include `-MemoryMB 4096`, `-Cpus 4`, and `-ForwardAdb -AdbPort 5555`. The optional forwarding binds only to host loopback and forwards TCP to guest port 5555; **it does not enable ADB in Android**. The image must support and enable TCP ADB separately. Protect debugging access and only use trusted guest images.

## Build and use the Qt shell

Install CMake 3.21+, a Windows C++17 compiler (e.g. Visual Studio 2022), and Qt 6.2+ Widgets matching that compiler. In a developer PowerShell, point CMake to the Qt installation:

```powershell
cmake -S . -B build -DCMAKE_PREFIX_PATH='C:\Qt\6.8.3\msvc2022_64'
cmake --build build --config Release
.\build\Release\ProjectNexus.exe
```

If required, run `windeployqt` from your Qt installation on the built executable before launching it outside the Qt environment. Select QEMU and a disk path in the sidebar. Use **Download Android-x86 9.0** to download the stock ISO into a new file, or select an existing ISO. The download is streamed to a temporary file and committed only after SHA-256 verification; cancellation or a mismatch discards it. The image is about 921 MiB and requires internet access. Source: [Android-x86 9.0-r2 release notes](https://www.android-x86.org/releases/releasenote-9-0-r2.html), [SourceForge download](https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download), [published SHA-256](https://osdn.net/projects/android-x86/downloads/71931/android-x86_64-9.0-r2.iso/). Start with **ISO**, install Android if desired, shut down, then select **Disk**. Paths and settings are stored locally using Qt's `QSettings`. Memory, cores, and optional loopback ADB forwarding can be configured before launch. The **Request shutdown** button sends QEMU's `system_powerdown` monitor command; the guest may ignore it, so shut down Android normally if necessary. Forcibly closing the shell can corrupt the guest disk.

The shell looks for a visible top-level window belonging to its QEMU process for up to 30 seconds and reparents it. QEMU's Windows display backend and DPI behavior can affect embedding, resizing, focus, and input. If embedding fails, QEMU may still be usable in its own window; shut it down normally. No global input hook or touch injection is installed.

## Scope and limitations

- QEMU is started with WHPX, QEMU's default CPU model, and a basic `std` VGA device for compatibility. The `host` CPU model is deliberately not required because support varies between WHPX builds. This is **not** Direct3D passthrough, ANGLE integration, or a gaming-grade accelerated graphics path. Compatibility and performance depend on QEMU and the chosen image; no boot-time, FPS, or input-latency target has been verified.
- ARM-only APKs need an image with a compatible, legally obtained native bridge and matching Android configuration. Copying a library or setting generic properties alone is not sufficient. No bridge is configured here.
- There is no host-to-guest touch daemon, keymapper, profiles, or multi-instance UI yet. In particular, don't expose an unauthenticated input socket to a network or write directly to guest input devices without a security design.
- One process owns one disk at a time. Do not start another VM using the same QCOW2 file. Back up important images and always shut down cleanly.
- On WHPX failure, verify the Windows feature and reboot, BIOS virtualization, host architecture, and your QEMU build. Guest boot problems can also result from incompatible images or installer/boot choices.

## GitHub Actions tests

All automated tests run in `.github/workflows/windows.yml` on pushes, pull requests, and manual workflow dispatch. The Windows job builds the Qt app, tests download integrity against a local HTTP fixture, tests launcher arguments and disk handling with a stub (WHPX is mocked), and publishes a Windows shell artifact. The Linux job uses real QEMU TCG to create a QCOW2 disk and boot a tiny test sector. Neither job proves that Android boots or that Windows WHPX, window embedding, graphics, or input work. The workflow and its artifact are available after this branch is pushed to GitHub.

## Development status

The download, launcher and embedding shell are the first milestones. Android is installed interactively using the stock ISO's installer; this is not an unattended OS installation. They require testing on a real Windows WHPX host with a compatible Android image; no Android boot, graphics, ADB, embedding, or latency guarantees are claimed. Later work should validate that foundation before adding input, native translation, or instance management.
