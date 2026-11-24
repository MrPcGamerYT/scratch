# Project Nexus

A Windows Android-player prototype using a source-built QEMU engine and Windows Hypervisor Platform (WHPX). The player opens a developer-prepared system image and does not offer disk creation, ISO selection or Android installation. A separately packaged developer tool prepares the image.

**This is not a finished BlueStacks alternative.** GPU acceleration, resolution/DPI and device profiles are not implemented and appear disabled in settings. The **Nexus Keymap Studio** implements multi-scheme v3 JSON profiles, tap/burst/turbo-repeat sequences, 8-way WASD D-pads with sprint/walk modifiers, directional swipes (including swipe-and-hold), two-finger symmetric pinch-zoom, MOBA directional skill pads with quick-cast/manual-aim/cancel, multi-step coordinate macro scripts, asymmetric X/Y mouse-aim with Left-Click Fire, Right-Click ADS and Free-Look eye camera, extended keyboard/mouse/wheel bindings, and a translucent click-through Win32 HUD overlay for key hints and live crosshairs. Support in Android and real games remains unverified, and there is no live input-recording macro recorder yet. No Android image is bundled, and Android compatibility or real-world performance is not guaranteed.

## Player preview

Run `python3 preview/server.py` from the repository root and open port 4173. This is an interactive **design mockup** showcasing the Nexus Keymap Studio, control presets, live stage key-hint overlay, macro script validator, conflict checker, and player settings—not the emulator itself: controls do not save settings or operate Android.

## Requirements

- Windows 10/11 x64, hardware virtualization enabled in BIOS/UEFI, and Windows Hypervisor Platform enabled. A reboot may be required. In an **administrator** PowerShell:

  ```powershell
  Enable-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform -All
  ```

  Check with `Get-WindowsOptionalFeature -Online -FeatureName HypervisorPlatform`. The player does not silently fall back from WHPX.
- The player bundle includes a source-built Nexus-patched QEMU in `qemu/bin`. A developer must supply a compatible, legally distributable, preinstalled x86_64 Android image at `images/android-base.qcow2`. The player has no OS image by default.
- Guest compatibility, graphics, native-bridge support and redistribution rights depend on the image. Android-x86 9.0-r2 is old and potentially unpatched; do not use it for sensitive accounts or data. No proprietary translation libraries are provided.

## Separate developer image tool

### Desktop preparation tool — ISO only (Windows)

Download **ProjectNexus-Windows** and **ProjectNexus-ImageStudio-Developer** from the latest successful build. Each GitHub artifact download contains a product ZIP and its `.zip.sha256` checksum. Extract the GitHub download, then extract the enclosed product ZIP into a normal folder. Do not run EXEs inside an archive. Keep all DLLs, Qt plugins, scripts and QEMU files together.

Both product ZIPs are created with ZIP64 support, CRC-tested and every entry is verified against a SHA-256 manifest before upload. `MANIFEST.sha256` is inside each product ZIP. Check the downloaded product ZIP in PowerShell with `Get-FileHash .\ProjectNexus-Windows.zip -Algorithm SHA256` (or the Developer ZIP name) and compare against its sibling `.sha256` file. A mismatch means the download is incomplete or changed: download again before extraction.

Open **NexusImageStudio.exe**. The desktop preparation tool now accepts **only a Bliss OS x86-64 PC installer `.iso`**, not ZIPs, OTA payloads or disk imports. It requires Windows PowerShell or PowerShell 7 and Windows Hypervisor Platform enabled.

1. Choose **Install from Bliss OS PC ISO** and select the ISO. An optional publisher SHA-256 can verify it before disk creation.
2. Choose an empty workspace and the player folder containing `ProjectNexus.exe`.
3. Leave Bliss UEFI enabled for a UEFI-based release. Start preparation, install Bliss in the VM, and shut it down cleanly.
4. Confirm the checkpoint to boot the installed disk without the ISO. Verify Android starts; shut down cleanly and confirm publishing.
5. The fixed output is `images/android-base.qcow2`, with matching UEFI firmware files. No disk renaming is needed. Existing published images are never overwritten.
6. **Resume retained ISO installation** continues the workspace's `android-install.qcow2`; it does not accept another image format.

**Edit offline image files (WSLg)** remains available for editing a new disk copy. Requires Windows 11 WSLg/Ubuntu with `sudo apt install python3-tk python3-guestfs qemu-utils libguestfs-tools`. Shut down all VMs first, choose a new working copy, mount a supported filesystem and edit files. Close the editor to flush changes. Originals stay unchanged; unsupported/verified/encrypted filesystems may need additional tooling. ISO preparation does not edit the ISO itself, and editing root files may break Android verified boot. OTA extraction utilities remain in the source repository for reference but are not included in the ISO-only desktop package.

### Fullscreen and Aim & Fire placement

**F11** toggles fullscreen, including when the embedded guest has focus. The player title bar, action toolbar and status bar hide in fullscreen and return on exit. F11 is reserved and cannot be used as a game binding. The Windows hook ignores F11 when another application or an app modal dialog has focus; other apps are not given a global fullscreen shortcut.

In Keymap Studio choose **Aim & Fire**, then click the game's fire-button location. A **FIRE** marker appears immediately and can be dragged like other controls; there is no coordinate-entry dialog required to place it. The aim anchor starts centered. **Aim options** configures the toggle key, sensitivity, ADS and Free-Look. Once armed in the player, the toggle key locks/unlocks aim: host cursor capture and the visible crosshair stay centered, while the game fire touch uses your draggable FIRE location. The guest camera-touch anchor may be dragged separately for games requiring a different touch region; it does not move the host cursor-lock center. A fire-location hint remains visible while the active crosshair is shown, even with general hints off.

### PowerShell backend (optional)

Download and extract the **ProjectNexus-ImageStudio-Developer** artifact separately from the **ProjectNexus-Windows** player artifact. Download a legally obtained Bliss OS x86-64 ISO from the [Bliss OS project](https://sourceforge.net/projects/blissos-x86/) and check the exact image's redistribution terms. The ISO is **not bundled**. On a Windows host with WHPX enabled, run the separate tool:

```powershell
# Run in the extracted developer tool directory.
.\prepare-android.ps1 -Bliss -IsoPath 'C:\Android\bliss.iso' `
  -DiskPath 'C:\Android\bliss-install.qcow2' -PlayerFolder 'C:\NexusPlayer'
# Optional: add -ExpectedSha256 <trusted published 64-digit SHA-256> to reject a mismatching ISO.
```

The disk's parent folder must already exist. The tool attaches a newly created virtual disk, isolated UEFI variables and the ISO—not physical host disks. Follow the ISO's **installation** menu, create/format the virtual partition as instructed by that Bliss release, install UEFI GRUB if prompted, and shut down cleanly. The tool then boots the disk without the ISO for manual verification. Menus vary; Bliss documentation warns its old Auto-Install option is not supported on newer releases. See [Bliss VM instructions](https://docs.blissos.org/installation/install-in-a-virtual-machine/) and [QEMU notes](https://docs.blissos.org/installation/install-in-a-virtual-machine/install-in-qemu/). The workflow is interactive, not an unattended install, and **has not yet been verified by installing or booting Bliss on Windows**. Basic Q35/UEFI firmware startup is exercised in CI under TCG only.

The tool will not overwrite an existing disk, UEFI variables, or staged image. If staging fails, the disk is retained and any already-staged firmware files must be backed up and manually removed before retrying. A failed installation is retained: use `-Bliss -Resume` with the same disk to boot it. To copy an existing Bliss disk, use `-Bliss -CopyFrom <source.qcow2> -SourceVars <source.qcow2.uefi-vars.fd>` with a new `-DiskPath`. Shut down all VMs and back up the disk and variables before copying. The staged UEFI variables are copied per-user on first launch; never distribute variables containing credentials or personal data. Back up the base, per-user disk and per-user UEFI variables together. A changed base or firmware template blocks startup rather than overwriting data. Legacy BIOS images continue to work without UEFI files.

For Android-x86 9.0-r2 instead, omit `-Bliss`: its legacy BIOS installer offers Auto Installation. No existing legacy disk is migrated or selectable in the player.

The prepared image starts automatically when the player opens, even if an older saved setting disabled automatic startup. If startup fails, use **Retry** after fixing the problem. For persistent user data, the player internally creates a separate per-user QCOW2 overlay under `%APPDATA%\ProjectNexus\Project Nexus\instances`; users are never asked to create or select disks. The base is not modified by normal VM writes. If the base is moved or changed, startup stops rather than replacing the overlay. Back up both. Legacy disks are preserved but not migrated or selectable in the new player.

## Offline developer file editor (no ADB)

The separately packaged Image Studio contains an **offline file browser**, running under a Linux desktop or Windows 11 WSLg. It copies a stopped, standalone QCOW2 image to a new filename, then lets the developer select a filesystem and browse, import/replace, export, create folders and delete files in the **copy**. It never edits the chosen source. This does not need Android to boot or ADB. Stop every VM and back up the source before copying; otherwise even the copy may be inconsistent. Do not open the working copy in a VM while the editor is running. Closing the editor flushes its changes.

For Windows 11 with WSLg and an Ubuntu WSL distribution, install dependencies in Ubuntu (requires WSL setup and network access), then run the editor from the extracted developer package, with its actual path substituted:

```bash
sudo apt update
sudo apt install python3-guestfs python3-tk qemu-utils libguestfs-tools
export LIBGUESTFS_BACKEND=direct
export LIBGUESTFS_BACKEND_SETTINGS=force_tcg
/usr/bin/python3 /mnt/c/path/to/ImageStudio/offline-files/offline_editor.py
```

Libguestfs requires its Linux appliance/kernel and may need a working WSLg setup; this workflow has not yet been verified on a real Windows PC. Copy the working image to a safe location first, shut down the editor, then use `prepare-android.ps1 -Resume -DiskPath 'C:\path\working.qcow2' -PlayerFolder 'C:\NexusPlayer'` to boot-test and stage it. It won't replace an existing player base; changing the base also requires a deliberate user-overlay migration. The browser operates on one selected filesystem at a time and does **not** unpack nested `system.sfs` / `system.img`, decrypt partitions, preserve all Android SELinux metadata for new files, bypass verified boot, or build an Android OS. Edits may prevent booting; keep the original backup.

## Bliss OS and licensing

Bliss OS FOSS is the selected OS family for evaluation. It is Android-based and has Android-x86 lineage; x86-64 is the PC CPU architecture. The exact release, installation, compatibility, and performance have not been validated on this player. A Nexus-branded distribution should honestly credit the upstream OS and comply with each component's licenses, source obligations, trademarks, and any bundled app/Google-service rights; you cannot claim authorship of the upstream OS. Review the precise image and consult a qualified reviewer before redistribution. See the Bliss team's [licensing overview](https://archive.fosdem.org/2024/events/attachments/fosdem-2024-2957-introduction-to-blisslabs-and-bliss-os/slides/22755/BLISS_V1_5_FOSS1_XaqePfP.pdf).

## Build and use the Qt player

Install CMake 3.21+, Visual Studio 2022 C++ build tools (x64), Qt 6.2+ Widgets and Network matching MSVC, and the stable Rust toolchain with the `x86_64-pc-windows-msvc` target (`rustup target add x86_64-pc-windows-msvc`). The current integrated build uses a Release Rust static library; the supported packaged configuration is Release:

```powershell
cmake -S . -B build -DCMAKE_PREFIX_PATH='C:\Qt\6.8.3\msvc2022_64'
cmake --build build --config Release
.\build\Release\ProjectNexus.exe
```

For a standalone build deploy Qt with `windeployqt`, bundle the engine and prepare the image as above. CPU cores, memory, cursor visibility and optional loopback ADB forwarding are configurable in Settings and apply at the next start. Forwarding TCP does not enable ADB in Android. **Shut down** sends QEMU's `system_powerdown`; the guest may ignore it. Shut down inside Android if necessary. Forcibly closing can corrupt the guest disk.

The player tries for 30 seconds to embed QEMU's visible window. QEMU's Windows display backend and DPI behavior can affect embedding, resizing, focus, and input. If embedding fails QEMU may remain in a separate window; shut down Android normally.

## Dock controls and APKs

**Install APK** requires a running image with TCP ADB enabled and authorized, local ADB forwarding enabled before startup, and Android platform-tools supplied by the developer in `platform-tools/adb.exe` beside the player or on PATH. Platform-tools is not bundled. Only install trusted APKs; the command may update an existing package. ADB exposes a debugging interface to local processes and is off by default. This integration is not yet verified against a real Android image.

Volume, RAM cleanup and FPS are disabled because they are not implemented. The app does not force-trim Windows processes or claim to clean memory; those actions can harm responsiveness or interrupt work. FPS requires real guest frame instrumentation and a reliable display overlay, not a fabricated counter.

The developer tool is a separate package, but this is not access control: a person who controls their PC and its files can inspect or modify an image with other tools. For stronger integrity, a future signed-image verification design would be needed; it still cannot guarantee control over a hostile host.

## Nexus Keymap Studio, HUD overlay and mouse aim

1. Click **Keymap** in the top dock (works both while Android is running with a live guest screenshot and offline against a 16:9 alignment grid).
2. Switch or create named **Schemes**, load starter **Templates** (*Battle Royale / FPS*, *MOBA 5v5*, *Action RPG*, or *Blank*), **Import / Export** portable v3 JSON profiles, and use **40-step Undo / Redo**, **2% Snap Grid**, and live **Key Conflict** diagnostics.
3. Choose a tool from the palette and click/drag on the canvas, using direct **Click-to-Bind** capture for keyboard keys (`A–Z`, `0–9`, `F1–F12`, arrows, numpad, punctuation, `Shift`/`Ctrl`/`Alt`/`Tab`/`Enter`/`Space`), mouse buttons (`Right Click`, `Middle Click`, `Mouse 4/5`), and `Wheel Up` / `Wheel Down`:
   - **Tap / Burst / Repeat**: Hold-or-sequential tap chains, **Burst N×** rapid taps on press (`2..50` taps), or **Turbo Repeat** while held (`25..5000 ms`).
   - **WASD 8-Way D-Pad**: Smooth continuous virtual thumbstick drag without dropping contact across cardinal/diagonal transitions, plus optional **Sprint** and **Walk** modifier radius scaling.
   - **Directional Swipe**: Smooth interpolated two-point drag (`25..5000 ms`) with optional **Hold at end** while the key remains held.
   - **Pinch Zoom**: Symmetric two-finger expand/contract gesture (requires multi-touch mode) with optional modifier chord (e.g., `Ctrl + Wheel Up/Down`).
   - **MOBA Skill Pad**: Directional skill-shot pad mapping live host cursor angle around a hero origin in **Quick Cast**, **Manual Aim (cast on release)**, or **Auto Cast** modes, plus an optional **Cancel Skill** key and target coordinate.
   - **Macro Script**: Text-authored multi-step automation (`tap`, `wait`, `swipe`, `down`, `move`, `up`, `multipress`) with live syntax/coordinate validation and interactive canvas coordinate picking.
4. Save, check **On** next to **Keymap**, and toggle **Hints** in the dock to display or hide the frameless click-through Win32 key-hint overlay (`KeymapOverlay`) over the guest viewport. Profiles are saved under `%APPDATA%\ProjectNexus\Project Nexus\keymaps\`; legacy v1/v2 profiles load automatically and upgrade cleanly to v3.

In standard single-pointer mode, actions share one virtual USB tablet pointer (two-finger pinch-zoom requires multi-touch). In experimental multi-touch mode (**Settings → Preferences → Experimental multi-touch keymapping**), `KeymapEngine` dynamically allocates across QEMU's 10-slot `virtio-multitouch-pci` device while reserving slots `0..2` for `AimController` when armed. Timing is best-effort, not a latency guarantee.

### Aim, Pan, Shoot, ADS & Free-Look

In **Keymap → Configure Aim…** (or **Aim & fire** in the dock), configure:
- **Start / Stop key** (default `F8`) and **Hold-to-Suspend key** (default `Tab`, temporarily freeing the host cursor for inventory/looting while releasing on key-up).
- **Camera anchor** (`10%..90%`) and independent **Horizontal (X)** and **Vertical (Y)** mouse sensitivities (`0.05..10.0`) with automatic invisible touch recentering near viewport edges.
- **Left-Click Fire** target coordinates, **Right-Click ADS Scope** (Hold or Tap-Toggle), **Free-Look Eye** camera key (`Alt` by default), and customizable **On-Screen Crosshair** reticle (`Cross + Dot`, `Cross`, `Dot`, or `Circle`).

Click **Aim & fire** in the dock to arm it, focus the Android display, and press your Start/Stop key (`F8`). Losing focus, minimizing/resizing the window, opening a modal dialog, QMP failure, or stopping the VM immediately releases mouse capture and all active touch/pointer contacts. QEMU accepting virtio multitouch and pointer events does not by itself prove Bliss/Android guest or game compatibility; real-PC testing is still required.

**Lock** is separate: it only confines the cursor; **Ctrl+Alt+L** releases it.

## Scope and limitations

- In the default hardware mode, QEMU is started with WHPX, QEMU's default CPU model, and a basic `std` VGA device for compatibility. The `host` CPU model is deliberately not required because support varies between WHPX builds. This is **not** Direct3D passthrough, ANGLE integration, or a gaming-grade accelerated graphics path. Compatibility and performance depend on QEMU and the chosen image; no boot-time, FPS, or input-latency target has been verified.
- ARM-only APKs need an image with a compatible, legally obtained native bridge and matching Android configuration. Copying a library or setting generic properties alone is not sufficient. No bridge is configured here.
- The keymapper uses QMP. Standard mode uses a single USB tablet pointer; optional experimental mode adds a QEMU 10-slot virtio multitouch device (`slots 0..9`). Bliss guest support and real game behavior are **not verified**. There is no live event-recording macro recorder, host-to-guest touch daemon, or multi-instance UI yet.
- One process owns one disk at a time. Do not start another VM using the same QCOW2 file. Back up important images and always shut down cleanly.
- On WHPX failure, verify the Windows feature and reboot, BIOS virtualization, host architecture, and your QEMU build. Guest boot problems can also result from incompatible images or installer/boot choices.

## Rust migration

This is an incremental migration, not a complete Rust player. `rust/nexus-core` is compiled as a static library and called by the existing Qt application through the C ABI in `src/rustcore.h`. **Key/mouse binding validation, tap/burst/repeat invariants, 8-way D-pad vector math, swipe interpolation, symmetric pinch-zoom geometry, MOBA cursor-angle projection, macro script step validation, and asymmetric X/Y aim-coordinate calculations run in Rust**; existing v1/v2/v3 JSON profile versions remain supported. The Qt UI, Windows hooks, QMP transport, disk management, and developer tools are still C++/PowerShell/Python. QEMU remains the bundled source-built C engine. No additional Rust runtime installation is required for end users. The Rust crate has no third-party crate dependencies; the standard library has its own license obligations. Moving this logic to Rust does not itself prove better performance or make single-pointer input multitouch.

## GitHub Actions tests

All automated tests run in `.github/workflows/windows.yml` on pushes, pull requests, and manual workflow dispatch. The Windows jobs apply our patch to pinned QEMU source and build it and the Qt app, run Rust unit tests and test the Rust–C++ integration, download integrity, keymap serialization, simultaneous sequence timing, aim arithmetic and QMP event creation; test launcher arguments with a stub; test managed-disk safeguards with real qemu-img; smoke-test the bundled QEMU, QMP pointer events and SDL update batching with a synthetic VGA workload under TCG, and publish a Windows development artifact. The Linux jobs use real QEMU TCG to boot a tiny test sector and exercise the offline file editor against a test QCOW2 filesystem. None of the jobs proves that Bliss or Android boots or that Windows WHPX, window embedding, graphics, or Windows hook and Android game input work. The workflow and its artifact are available after this branch is pushed to GitHub.

## Development status

The download, launcher and embedding shell are the first milestones. Android is installed interactively using the stock ISO's installer; this is not an unattended OS installation. They require testing on a real Windows WHPX host with a compatible Android image; no Android boot, graphics, ADB, embedding, or latency guarantees are claimed. The Windows keyboard hook and Android interaction require testing on a real host; later work can add true multi-touch, native translation, and instance management.

## Third-party licensing

QEMU is open source, primarily GPL version 2, and can be redistributed under its terms; separate firmware and libraries have their own licenses. The development bundle contains QEMU and SeaBIOS source, build instructions, and notices in `qemu/licenses/`. Review those notices and the exact dependency, Qt and source-distribution obligations before public release. See [third-party notices](packaging/THIRD-PARTY.md). This is not legal advice.

## QEMU engine development

Project Nexus builds a **modified QEMU 10.2.0**, preserving QEMU's name, attribution and licensing. The first small engine patch is in [`packaging/qemu-patches/0001-sdl2-batch-present.patch`](packaging/qemu-patches/0001-sdl2-batch-present.patch). In the SDL 2D renderer it still uploads each changed rectangle, but batches screen presents that occur during one synchronous hardware refresh. Updates outside that refresh still present immediately. This may reduce redundant rendering work when VGA emits multiple dirty rectangles; it is not a new graphics driver, FPS limiter, or proof of improved Android game performance.

GitHub Actions exercises the patch against a tiny VGA workload on the Windows build with SDL's dummy driver and reports rectangle uploads and screen presents. This is a regression and work-count check—not a real-world CPU, frame-time, boot-time or Android benchmark. A before/after comparison on the same real Windows machine and Android workload is needed before claiming any speedup. The bundled source archive contains the patched source and the separate patch, and the base revision is pinned in the workflow.

## Rust runtime migration and performance measurement

The native player now obtains its complete QEMU argument plan from the Rust runtime (`runtime.rs`) over a size-checked C ABI. Rust owns launch-resource validation, loopback-only ADB configuration, multitouch/UEFI device selection and path encoding. On Windows, Rust probes WHPX hypervisor capability before launch; the player refuses to silently fall back to software CPU emulation. Capability presence is not a guarantee that a specific VM starts. QEMU still owns virtual CPU/device execution, Qt/C++ still owns UI/process lifecycle/input hooks, and the file editor is retained. This is a migration step, not a Rust-native replacement for QEMU.

Run `cargo run --locked --release --manifest-path rust/nexus-core/Cargo.toml --bin runtime-bench` for the reproducible launch-planner microbenchmark. CI records this alongside Rust tests and a C++/Rust JSON/ABI integration test. It measures configuration-construction cost only, **not game FPS, end-to-end latency, boot performance or superiority to another emulator**. A fastest-emulator claim requires reproducible real-game measurements on matched hardware/settings; none is made here. GPU acceleration and guest driver compatibility remain major performance work.

## Direct HUD binding and single title bar

In Keymap Studio, click an empty canvas location with Tap or Select active. A dashed **Press a key…** marker waits for the next keyboard key, mouse button (including left click), or wheel direction on the canvas. Escape or loss of canvas focus cancels it. Pending markers are never serialized; only successful valid captures enter the profile and undo history. Existing controls remain draggable. F11 and system-reserved keys are not game bindings. Left-click bindings conflict with Aim Fire and are reported by the scheme diagnostics.

The player's dock tools now live in its single draggable custom title bar, alongside window controls. The secondary header, visible Start/Stop/Retry controls and bottom status bar are removed from the player layout; automatic startup and graceful window-close behavior remain. Errors still use dialogs; fullscreen hides the title bar and F11 restores it. The browser preview demonstrates the title bar and click-to-bind workflow but does not run Android; its fullscreen action is a simulated layout state.

### Missing `kvmvapic.bin` / WHPX installation failures

The player and Developer packages require `qemu/share/qemu/kvmvapic.bin` in addition to the BIOS and VGA ROM. Older bundles omitted the virtual-APIC ROM. New builds supply it from the pinned QEMU revision, including when reusing a cached engine artifact, and ZIP packaging checks the ROM is present and nonempty. The installer checks for missing companion ROMs before disk creation when the bundled BIOS is detected; the player validates all required PC ROMs before startup.

A subsequent `WHPX: Failed to emulate MMIO access` or virtual-processor crash can be a separate host/guest compatibility issue; adding a missing ROM does not prove this is resolved. Preserve Windows build, CPU model, exact ISO version/hash and the full log if it persists. Do not publish a disk from a failed installation. Resume is appropriate only for an installed disk; when installation never started, use Install with a fresh workspace after correcting the engine package. Never overwrite an existing working disk or disable host security protections to guess at a fix.

### Workaround for WHPX MMIO / virtual-processor crashes

`Windows Hypervisor Platform accelerator is operational` followed by `WHPX: Failed to emulate MMIO access` means enabling virtualization again is not a confirmed fix. The host accelerated execution path is failing; the exact cause depends on Windows, CPU, QEMU and guest. There is now an **explicit**, slower bypass:

1. Extract the updated Developer and player packages into fresh folders; preserve prior images/workspaces.
2. In Image Studio select **CPU execution → Software compatibility (TCG)**. Choose Install, the PC ISO and a fresh workspace. This passes `-accel tcg,thread=multi -cpu max` to BOTH installer and verification boots and does not query/initialize WHPX.
3. Complete installation and verification normally. Software emulation can take much longer. It is not a gaming-performance fix and may still fail with an incompatible guest.
4. On successful publishing, `images/acceleration.txt` records the explicitly chosen mode. The player uses this mode unless overridden in **Settings → Performance → CPU execution**. The default Settings choice is **Use prepared image's selection**. Software-mode player windows display a slow-software warning in their title. Restart after changing the setting.

No automatic WHPX retry/fallback takes place; no existing disk is modified by a hidden retry. To try hardware acceleration later choose WHPX in Settings, with the understanding the original host crash may persist. Firmware smoke tests in CI exercise real bundled QEMU/Q35/UEFI under multi-thread TCG with the `max` CPU; they do NOT install your Bliss ISO or validate WHPX on your machine. Report Windows build (`winver`), CPU model and the exact ISO filename/hash with the complete log if either mode fails. Resume is only for an installed disk; it does not rerun the ISO installer on a blank/partially installed disk.

## Image Studio easy setup and stable Browse dialogs

The default desktop screen now asks for only **1. Bliss PC ISO**, **2. extracted emulator folder**, then **3. Prepare Android disk**. Workspace names are generated automatically for new installs. The last existing ISO/player paths and CPU execution choice are remembered; a nearby player folder is detected when possible. Compatibility TCG is the default for a new Image Studio configuration, clearly labelled slow; previously saved execution choices are retained.

**Advanced options** contains Resume, workspace selection, optional ISO checksum, firmware mode and hardware/software execution. **Show technical log** reveals diagnostics; failures also reveal the log. **I finished this step** confirms a clean guest shutdown before the next stage. Both bundled QEMU tools are executable-self-checked before disk creation/boot, so blocked or incomplete bundles fail early with an actionable error. Actual Bliss partitioning/installation and boot verification remain guided and version-dependent; this does not blindly auto-format or auto-approve an installation.

Browse uses a Qt-owned ISO/folder picker. File/message dialogs are excluded from the global borderless-window filter, preventing their flags/handles from being changed during Show (which could immediately cancel a picker). Regression tests check file selection, folder cancellation and persistence of the visible picker with the chrome filter active. The offline file editor remains available and retains its WSLg dependencies.

### Compact player and Android display controls

The player's single title bar is now 36 logical pixels high (previously 56),
with compact dock controls and the same move/maximize/minimize/close behavior.
Display settings provide landscape/portrait resolution presets and DPI overrides,
plus Android-default reset. **Apply to running Android** is separate from Save:
it changes the guest immediately using `wm size` and `wm density` through ADB.
Android retains these overrides. Local ADB forwarding must have been enabled
before launch; the guest must support and authorize TCP ADB, and trusted Android
platform-tools must be supplied next to the player or on PATH. Operations are
asynchronous, timed, and report errors/partial application rather than claiming
success. Apply both Android defaults to recover from an unsuitable configuration.
Resolution changes may require adjusting keymaps.

This is still QEMU standard-VGA graphics with Rust runtime/input components, not
a completed Rust-native GPU engine. Vulkan, DirectX, ASTC and GPU passthrough
remain unavailable; resolution/DPI controls do not add hardware rendering.
