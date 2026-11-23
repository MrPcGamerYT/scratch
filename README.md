# Project Nexus

A first Windows prototype for running an Android-x86-compatible guest in QEMU with Windows Hypervisor Platform (WHPX). It provides a standalone PowerShell launcher and an optional Qt 6 desktop shell that downloads a stock Android-x86 image, starts QEMU and attempts to embed its native window. This is **not a finished BlueStacks alternative**: it does not include an Android image, multi-instance management, FPS controls, ARM translation, Google Play, or a GPU acceleration stack. Its keymapper supports single-pointer taps, tap sequences and repeat-while-held controls; true multitouch is not yet supported.

## Requirements

- Windows 10/11 x64, hardware virtualization enabled in BIOS/UEFI, and Windows Hypervisor Platform enabled. A reboot may be required. In an **administrator** PowerShell:

  ```powershell
  Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All
  ```

  Check with `Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform`. QEMU reports an error if WHPX cannot initialize; the launcher does not silently fall back to software emulation. Virtual Machine Platform is not required specifically for QEMU WHPX.
- The development bundle includes a source-built, Nexus-patched QEMU x64 engine in `qemu/bin`. Project Nexus automatically selects it. When building the shell yourself without the bundle, provide a compatible QEMU distribution with `qemu-system-x86_64.exe` and `qemu-img.exe` in the same directory.
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

If required, run `windeployqt` from your Qt installation on the built executable before launching it outside the Qt environment. The packaged shell selects its bundled QEMU automatically; select a disk path in the sidebar. Use **Download Android-x86 9.0** to download the stock ISO into a new file, or select an existing ISO. The download is streamed to a temporary file and committed only after SHA-256 verification; cancellation or a mismatch discards it. The image is about 921 MiB and requires internet access. Source: [Android-x86 9.0-r2 release notes](https://www.android-x86.org/releases/releasenote-9-0-r2.html), [SourceForge download](https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download), [published SHA-256](https://osdn.net/projects/android-x86/downloads/71931/android-x86_64-9.0-r2.iso/). Start with **ISO**, install Android if desired, shut down, then select **Disk**. Paths and settings are stored locally using Qt's `QSettings`. Memory, cores, and optional loopback ADB forwarding can be configured before launch. The **Shut down** button sends QEMU's `system_powerdown` monitor command; the guest may ignore it, so shut down Android normally if necessary. Forcibly closing the shell can corrupt the guest disk.

The shell looks for a visible top-level window belonging to its QEMU process for up to 30 seconds and reparents it. QEMU's Windows display backend and DPI behavior can affect embedding, resizing, focus, and input. If embedding fails, QEMU may still be usable in its own window; shut it down normally. The keymapper installs a Windows low-level keyboard hook only while enabled and removes it when disabled or the VM closes. Mapped keys are captured only when the embedded QEMU display has focus; a held click is released when focus is lost.

## Prepared player image (developer workflow)

The development artifact does **not** contain Android or an installed disk. A developer must obtain a compatible ISO with suitable redistribution rights, then use the separate `prepare-android.ps1` tool included in the package. The tool launches the Android installer into a new virtual disk; installation is interactive. After a clean guest shutdown and explicit confirmation it checks the disk and stages it in the player folder. It will not overwrite an existing disk or base image.

```powershell
# Run from the extracted player folder. Both folders must already exist.
.\prepare-android.ps1 -IsoPath 'C:\Android\android.iso' `
  -DiskPath 'C:\Android\new-install.qcow2' -PlayerFolder $PWD
```

Test the staged system before distribution; the tool cannot prove that Android was installed or that redistribution is permitted. With `images/android-base.qcow2` present, the player starts it automatically by default and creates a **separate per-user QCOW2 overlay** under `%APPDATA%\ProjectNexus\Project Nexus\instances`. The base is not modified by normal VM writes. The existing base location, size, and modification time are recorded; changing or moving it causes startup to stop safely rather than replacing the user disk. Back up both the base and overlay. An existing legacy user disk is not migrated automatically.

Without a prepared base, the previous ISO/disk workflow is still available under **Settings → Developer / legacy image setup**, preserving earlier installations. No OS image is downloaded or bundled by the player automatically. A prepared base takes precedence over legacy ISO settings. Disable **Start prepared image automatically** in Settings to start it manually.

## Keymap and mouse

1. Start the VM and open the Android screen or game. Click **Edit keymap**.
2. Choose a key and action; click to add markers and drag them into place. You can place the **same key at several locations**. Select a marker in the list to remove it.
3. In **Hold / tap sequence** mode, one marker holds a click until release; multiple markers click in order once, even for a quick keypress. **Repeat while held** repeats the sequence until the key is released. Set the delay in the editor.
4. Save, check **Enable keymap**, then click the Android display to focus it. The hook is enabled only on request and releases held clicks on focus loss, disable, or shutdown. The default profile is saved in `%APPDATA%\ProjectNexus\Project Nexus\keymaps\default.json`; older profiles are loaded automatically.

These actions use one virtual mouse pointer. Multiple markers are sequential, not simultaneous touches; while a sequence is running other mapped keys are ignored. Timing is best-effort, not a latency guarantee. If the screenshot is blank or positions are inaccurate, bring the display into view and verify coordinates after changing orientation or size.

Use **Lock mouse** to confine the cursor to the emulator window. Press **Ctrl+Alt+L** to unlock; switching to another application or closing the VM also releases it. This is cursor confinement, not a first-person relative-mouse or mouse-look implementation. The Windows hook and clipping behavior need real-PC testing.

## Scope and limitations

- QEMU is started with WHPX, QEMU's default CPU model, and a basic `std` VGA device for compatibility. The `host` CPU model is deliberately not required because support varies between WHPX builds. This is **not** Direct3D passthrough, ANGLE integration, or a gaming-grade accelerated graphics path. Compatibility and performance depend on QEMU and the chosen image; no boot-time, FPS, or input-latency target has been verified.
- ARM-only APKs need an image with a compatible, legally obtained native bridge and matching Android configuration. Copying a library or setting generic properties alone is not sufficient. No bridge is configured here.
- The keymapper uses QEMU's QMP standard input/output connection and an emulated USB tablet; it is **not Android multitouch**. Games requiring simultaneous movement and aiming, swipes, or real touch events may not work. There is no host-to-guest touch daemon, joystick, mouse-look, general-purpose macro recorder, or multi-instance UI yet.
- One process owns one disk at a time. Do not start another VM using the same QCOW2 file. Back up important images and always shut down cleanly.
- On WHPX failure, verify the Windows feature and reboot, BIOS virtualization, host architecture, and your QEMU build. Guest boot problems can also result from incompatible images or installer/boot choices.

## GitHub Actions tests

All automated tests run in `.github/workflows/windows.yml` on pushes, pull requests, and manual workflow dispatch. The Windows jobs apply our patch to pinned QEMU source and build it and the Qt app, test download integrity, keymap serialization, sequence timing and QMP event creation; test launcher arguments with a stub; test managed-disk safeguards with real qemu-img; smoke-test the bundled QEMU, QMP pointer events and SDL update batching with a synthetic VGA workload under TCG, and publish a Windows development artifact. The Linux job uses real QEMU TCG to create a QCOW2 disk and boot a tiny test sector. Neither job proves that Android boots or that Windows WHPX, window embedding, graphics, or Windows hook and Android game input work. The workflow and its artifact are available after this branch is pushed to GitHub.

## Development status

The download, launcher and embedding shell are the first milestones. Android is installed interactively using the stock ISO's installer; this is not an unattended OS installation. They require testing on a real Windows WHPX host with a compatible Android image; no Android boot, graphics, ADB, embedding, or latency guarantees are claimed. The Windows keyboard hook and Android interaction require testing on a real host; later work can add true multi-touch, native translation, and instance management.

## Third-party licensing

QEMU is open source, primarily GPL version 2, and can be redistributed under its terms; separate firmware and libraries have their own licenses. The development bundle contains QEMU and SeaBIOS source, build instructions, and notices in `qemu/licenses/`. Review those notices and the exact dependency, Qt and source-distribution obligations before public release. See [third-party notices](packaging/THIRD-PARTY.md). This is not legal advice.

## QEMU engine development

Project Nexus builds a **modified QEMU 10.2.0**, preserving QEMU's name, attribution and licensing. The first small engine patch is in [`packaging/qemu-patches/0001-sdl2-batch-present.patch`](packaging/qemu-patches/0001-sdl2-batch-present.patch). In the SDL 2D renderer it still uploads each changed rectangle, but batches screen presents that occur during one synchronous hardware refresh. Updates outside that refresh still present immediately. This may reduce redundant rendering work when VGA emits multiple dirty rectangles; it is not a new graphics driver, FPS limiter, or proof of improved Android game performance.

GitHub Actions exercises the patch against a tiny VGA workload on the Windows build with SDL's dummy driver and reports rectangle uploads and screen presents. This is a regression and work-count check—not a real-world CPU, frame-time, boot-time or Android benchmark. A before/after comparison on the same real Windows machine and Android workload is needed before claiming any speedup. The bundled source archive contains the patched source and the separate patch, and the base revision is pinned in the workflow.
