# Independent Nexus engine — first foundation, not a working Android emulator

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
- Host-independent unit tests plus independent Python/zlib binary-format checks
  on Linux/Windows CI. No fabricated Android/GPU/FPS success condition.
- `boot` explicitly refuses to start; no silent fallback to the old QEMU engine.

## Required next implementation gates (not delivered yet)

1. **CPU + memory expansion:** general guest RAM layout and vCPU lifecycle,
   long-mode/SMP setup, exit handling and instruction/MMIO emulation; bounded
   I/O/interrupt and cancellation tests. A real-mode store/load/HLT test now
   passes, but it does not cover these missing pieces. Using WHP directly is
   not a guarantee to fix a host WHPX/MMIO failure.
2. **Machine + boot:** choose and document a machine ABI, firmware/loader,
   interrupts/timers/ACPI as required, PCI/virtio and block transport, persistence
   with separate safe write layers. Boot a Linux test kernel before Android.
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
