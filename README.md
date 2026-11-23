# Project Nexus

A Windows Android-player prototype using a source-built QEMU engine and Windows Hypervisor Platform (WHPX). The player opens a developer-prepared system image and does not offer disk creation, ISO selection or Android installation. A separately packaged developer tool prepares the image.

**This is not a finished BlueStacks alternative.** GPU acceleration, resolution/DPI and device profiles are not implemented and appear disabled in settings. The keymapper supports single-pointer taps, sequential markers and repeat-while-held; it does not support true multitouch, mouse-look, or a general macro recorder. No Android image is bundled, and Android compatibility or real-world performance is not guaranteed.

## Player preview

Run `python3 preview/server.py` from the repository root and open port 4173. This is an interactive **design mockup**, not the emulator: controls do not save settings or operate Android. The Windows Qt player implements the corresponding layout and the existing working features described below; the keymap preview is illustrative and differs from the actual Qt editor.

## Requirements

- Windows 10/11 x64, hardware virtualization enabled in BIOS/UEFI, and Windows Hypervisor Platform enabled. A reboot may be required. In an **administrator** PowerShell:

  ```powershell
  Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All
  ```

  Check with `Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform`. The player does not silently fall back from WHPX.
- The player bundle includes a source-built Nexus-patched QEMU in `qemu/bin`. A developer must supply a compatible, legally distributable, preinstalled x86_64 Android image at `images/android-base.qcow2`. The player has no OS image by default.
- Guest compatibility, graphics, native-bridge support and redistribution rights depend on the image. Android-x86 9.0-r2 is old and potentially unpatched; do not use it for sensitive accounts or data. No proprietary translation libraries are provided.

## Separate developer image tool

Download and extract the **ProjectNexus-ImageStudio-Developer** artifact separately from the **ProjectNexus-Windows** player artifact. Obtain a compatible ISO and review its redistribution terms. The developer tool creates a new virtual disk, launches its installer and stages the resulting standalone image in the player package; it does not prove that the installation succeeded. The ISO installer requires a real WHPX-capable Windows host. For example:

```powershell
# Run from the extracted developer tool directory, not the player directory.
.\prepare-android.ps1 -IsoPath 'C:\Android\android.iso' `
  -DiskPath 'C:\Android\new-install.qcow2' -PlayerFolder 'C:\NexusPlayer'
```

The disk's parent folder must already exist. For Android-x86 9.0-r2, select **Advanced options → Auto Installation**, confirm erasing the new virtual disk, wait for installation, and shut down. The tool attaches no physical host disks, then boots the virtual disk without the ISO so you can verify it before staging. Installation and verification still require interaction; boot is not guaranteed. The tool retains a failed or unfinished image; use `-Resume` with the same `-DiskPath` to boot it again. Use `-CopyFrom 'C:\Android\original.qcow2'` with a **new** `-DiskPath` (and no ISO) to copy a standalone image and boot the copy for changes inside Android. Shut down all VMs using the source before copying, and keep backups. This does not provide arbitrary system-image editing or root access; those depend on the guest.

The tool will not overwrite an existing disk or base image. Test boot and verify distribution rights before sharing the prepared player. For stock Android-x86 9.0-r2, see the [release notes](https://www.android-x86.org/releases/releasenote-9-0-r2.html) and [ISO download](https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download). Published SHA-256: `f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e`.

The prepared image starts automatically by default. For persistent user data, the player internally creates a separate per-user QCOW2 overlay under `%APPDATA%\ProjectNexus\Project Nexus\instances`; users are never asked to create or select disks. The base is not modified by normal VM writes. If the base is moved or changed, startup stops rather than replacing the overlay. Back up both. Legacy disks are preserved but not migrated or selectable in the new player.

## Build and use the Qt player

Install CMake 3.21+, a Windows C++17 compiler (e.g. Visual Studio 2022), and Qt 6.2+ Widgets and Network matching that compiler:

```powershell
cmake -S . -B build -DCMAKE_PREFIX_PATH='C:\Qt\6.8.3\msvc2022_64'
cmake --build build --config Release
.\build\Release\ProjectNexus.exe
```

For a standalone build deploy Qt with `windeployqt`, bundle the engine and prepare the image as above. CPU cores, memory, cursor visibility, automatic startup and optional loopback ADB forwarding are configurable in Settings and apply at the next start. Forwarding TCP does not enable ADB in Android. **Shut down** sends QEMU's `system_powerdown`; the guest may ignore it. Shut down inside Android if necessary. Forcibly closing can corrupt the guest disk.

The player tries for 30 seconds to embed QEMU's visible window. QEMU's Windows display backend and DPI behavior can affect embedding, resizing, focus, and input. If embedding fails QEMU may remain in a separate window; shut down Android normally.

## Dock controls and APKs

**Install APK** requires a running image with TCP ADB enabled and authorized, local ADB forwarding enabled before startup, and Android platform-tools supplied by the developer in `platform-tools/adb.exe` beside the player or on PATH. Platform-tools is not bundled. Only install trusted APKs; the command may update an existing package. ADB exposes a debugging interface to local processes and is off by default. This integration is not yet verified against a real Android image.

Volume, RAM cleanup, and the FPS toggle are disabled because they are not implemented. The app does not force-trim Windows processes or claim to clean memory; those actions can harm responsiveness or interrupt work. FPS requires real guest frame instrumentation and a reliable display overlay, not a fabricated counter.

The developer tool is a separate package, but this is not access control: a person who controls their PC and its files can inspect or modify an image with other tools. For stronger integrity, a future signed-image verification design would be needed; it still cannot guarantee control over a hostile host.

## Keymap and mouse

1. Start the VM and open the Android screen or game. Click **Keymap** in the top dock.
2. Choose a key and action; click to add markers and drag them into place. You can place the **same key at several locations**. Select a marker in the list to remove it.
3. In **Hold / tap sequence** mode, one marker holds a click until release; multiple markers click in order once, even for a quick keypress. **Repeat while held** repeats the sequence until the key is released. Set the delay in the editor.
4. Save, check **On** next to Keymap, then click the Android display to focus it. The hook is enabled only on request and releases held clicks on focus loss, disable, or shutdown. The default profile is saved in `%APPDATA%\ProjectNexus\Project Nexus\keymaps\default.json`; older profiles are loaded automatically.

These actions use one virtual mouse pointer. Multiple markers are sequential, not simultaneous touches; while a sequence is running other mapped keys are ignored. Timing is best-effort, not a latency guarantee. If the screenshot is blank or positions are inaccurate, bring the display into view and verify coordinates after changing orientation or size.

Use **Lock** to confine the cursor to the emulator window. Press **Ctrl+Alt+L** to unlock; switching to another application or closing the VM also releases it. This is cursor confinement, not a first-person relative-mouse or mouse-look implementation. The Windows hook and clipping behavior need real-PC testing.

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
