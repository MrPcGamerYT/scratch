# Independent Nexus engine — incremental implementation and test gates

## User request

A custom Rust engine without QEMU, loading `gustimg.zlag`, with real graphics
acceleration using DirectX, OpenGL and Vulkan and tested Android guest operation.
The existing Qt/QEMU player and all existing user images are preserved unchanged.
The new engine is a separate crate: `rust/nexus-engine`. It does not invoke, link,
rename or bundle QEMU. Its initial CI artifacts are developer foundations only.

## Implemented in this increment

- Documented ZLAG v1 disk container; streaming exclusive raw-disk import, header
  and payload CRC32 checks, exact-size/version/reserved-bit validation, read-only
  sector transport with overflow/bounds checks. No third-party crate dependency.
- Independent Rust CLI with capabilities, import, verify and a direct Windows
  Hypervisor Platform partition create/configure/setup/delete probe.
- Direct WHP real-mode CPU execution: private 4 KiB guest RAM, vCPU register
  initialization/run/result checking, guest store/load/HLT program, bounded cancel
  watchdog, and RAII vCPU/mapping/partition cleanup. This is not a complete CPU
  exit emulator or boot machine. Windows SDK compile-time assertions validate the
  small hand-written Rust FFI layouts and constants.
- Scalar IN/OUT exit dispatch for private test ports, AL/AX/EAX input semantics,
  RIP advancement, strict unsupported-port/string/REP rejection and bounded output.
  This is not a serial UART, PCI chipset or production machine/device ABI.
- A whole-run deadline and 64-exit budget, live infinite-loop cancellation and
  restart of the SAME vCPU after cancellation.
- Verified ZLAG first-sector test loader: checks image CRC/signature, maps private
  1 MiB RAM, copies the sector to `0000:7c00`, and executes it through WHP. No BIOS,
  UEFI, disk-device/INT 13h, interrupts, framebuffer or Android support is implied.
- Host-independent unit tests plus independent Python/zlib binary-format checks
  on Linux/Windows CI. No fabricated Android/GPU/FPS success condition.
- `boot` explicitly refuses to start; no silent fallback to the old QEMU engine.

## Required next implementation gates (not delivered yet)

1. **CPU + memory expansion:** general guest RAM layout and vCPU lifecycle,
   protected/long-mode and SMP setup, interrupt delivery and instruction/MMIO
   emulation. Scalar I/O, bounded execution, cancel/resume and real-mode RAM tests
   now pass, but do not cover those missing pieces. Using WHP directly is
   not a guarantee to fix a host WHPX/MMIO failure.
2. **Machine + boot:** choose and document a machine ABI, firmware/loader,
   interrupts/timers/ACPI as required, PCI/virtio and block transport, persistence
   with separate safe write layers. The tiny first-sector test loader is complete;
   a PC BIOS/UEFI implementation is NOT. Boot a Linux test kernel before Android.
3. **Android contract:** compatible guest kernel/initramfs and device drivers;
   new image provisioned from a verified raw disk, Home/ANR/persistence/shutdown
   gates across repeated boots without relying on QEMU's emulated machine.
4. **Graphics transport:** actual virtio-gpu or another specified guest/host
   protocol and matching Android DRM/Mesa/HAL drivers, fenced command submission,
   resource sharing, synchronization and context recovery. A host GPU API is not
   a guest accelerated graphics driver.
5. **Host rendering:** evaluate D3D12-backed rendering/translation for Windows,
   Vulkan and OpenGL compatibility paths with explicit feature negotiation.
   These are engineering choices to validate, NOT implemented API support.
   Vulkan driver availability cannot be guaranteed on old Intel HD 530 hardware.
6. **Integration:** real input, audio, networking, clipboard/native guest bridge,
   frame presentation telemetry; only then connect the new engine to the Qt UI.
7. **Certification:** real Windows 10 hardware/driver matrix, no corruption on
   interrupted writes, repeated Android boot/Home/no-ANR tests, GPU rendering
   correctness/reference scenes, games, cursor/audio/network and measured latency/
   FPS comparisons. CI build success is not physical GPU or game certification.

There is no credible single-turn implementation or test that makes this entire
list finished. Hosted Windows runners may lack virtualization/GPU support; their
capability probe outcomes must be reported as unavailable, not falsely passed.
No performance improvement is claimed and no QEMU-free Android download is ready.

## Verified first CPU milestone

[Run 45740200529](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45740200529)
passed on Windows Server 2022 and Ubuntu 24.04 runners. Both platforms passed
four Rust image tests, Clippy with warnings denied, release builds and independent
Python/zlib format/source-safety checks. The Windows job additionally compiled
FFI size/alignment/offset/enum assertions against its Windows SDK, created a WHP
partition, and ran the real guest CPU store/load/HLT test three times, checking
AX and guest RAM both contained `0x5a17`. No QEMU executable or library is involved.
Linux explicitly reports WHP unsupported; it does not pretend to execute that test.

This is a Windows **server runner** observation, not physical Windows 10/i3/HD530
certification. There is still no Android boot, firmware/device model, accelerated
GPU backend, working converted Android `gustimg.zlag`, or speed comparison.
No end-user emulator replacement is published. Existing player and guest files
remain unchanged; all artifacts from this workflow are developer foundations.

## Verified I/O, cancellation and file-loaded execution milestone

[Run 45741571782](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45741571782)
passed on Ubuntu 24.04 and Windows Server 2022. Eight pure Rust image/boot/I/O tests,
Clippy (warnings denied), release builds, independent Python binary-format checks
and the expanded Windows SDK ABI assertions passed. Linux correctly refuses WHP
execution commands rather than reporting emulation success.

Windows also passed, each with three repetitions:

- Existing CPU RAM store/load/HLT program, without regression.
- Real scalar IN/OUT exits: six debug bytes, dword device write, AL/AX/EAX reads,
  ten I/O exits, exact register and guest-RAM results, then HLT.
- Rejection of actual REP OUTSB, unknown host-port requests and I/O exit storms.
  Host port access is never forwarded to real hardware.
- Cancellation of a guest `jmp $` loop after the 250 ms deadline; verified
  `WHvRunVpExitReasonCanceled`, then ran the RAM program on the same vCPU.
- A Python-generated raw test sector imported into a REAL ZLAG file, fully
  verified and loaded at `0000:7c00`, executed by WHP, producing exactly
  `NEXUS-ZLAG-BOOT` via the private debug port, then halting. Input image SHA256
  unchanged. Invalid sector signatures and corrupt ZLAG payloads were refused.

No QEMU executable, library, driver or device model participates in these tests.
These are limited test-machine milestones, not Linux/Android boot, production
firmware, physical Windows 10/GPU or performance certification.

### Ordered remaining work (all retained in scope)

1. Protected/long-mode CPU startup, general RAM layout, MMIO/interrupt handling.
2. A documented bootable machine ABI, Linux loader/firmware choice, disk and
   PCI/virtio devices plus safe persistence. First target: Linux kernel boot.
3. Compatible Android kernel/drivers and a real bootable Android `gustimg.zlag`;
   repeated Home, ANR, clean shutdown and persistence gates.
4. Accelerated guest graphics transport and matching Android drivers; host GPU
   enumeration/capability checks and actual DirectX/OpenGL/Vulkan renderer paths.
5. Input, audio, networking and the native guest-control bridge; Qt player hookup.
6. Physical Windows 10/GPU matrix, game/rendering correctness and measured CPU/GPU
   performance/latency benchmarks. No speed claim before those measurements.

The old player and user images remain unchanged. Native-only changes now use
native CI without unnecessarily rebuilding the legacy QEMU engine.
