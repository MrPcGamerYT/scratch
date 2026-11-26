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
  Hypervisor Platform partition create/configure/setup/delete probe. The latter
  does NOT execute a guest CPU instruction and fails if WHP is unavailable.
- Host-independent unit tests plus independent Python/zlib binary-format checks
  on Linux/Windows CI. No fabricated Android/GPU/FPS success condition.
- `boot` explicitly refuses to start; no silent fallback to the old QEMU engine.

## Required next implementation gates (not delivered yet)

1. **CPU + memory:** direct WHP RAM mappings, vCPU register/run/cancel, exit
   handling and instruction/MMIO emulation; small bounded HLT/I/O/RAM guest tests.
   Using WHP directly is not a guarantee to fix a host WHPX/MMIO failure.
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
