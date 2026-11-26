# Native Linux kernel loader — implemented, early entry verified

This is the independent Rust/WHP engine, not the existing Qt/QEMU player. No QEMU
executable, library, firmware or device model participates. This change implements
a direct Linux **loader**, not a new Linux kernel written from scratch.

## Current result

[Actual-kernel checkpoint 45744847270](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45744847270)
passed on a Windows Server 2022 runner. A source-built, pinned x86-64 Linux v6.6
kernel entered via its 32-bit bzImage entry, progressed through its own startup,
and printed its actual Linux version banner, command line, the supplied E820 RAM
map and early serial-console initialization. It then requested an unsupported
byte read from PC timer/speaker-control port **0x61** and the engine stopped it.

**Full Linux boot is NOT verified.** No init process, root filesystem, disk,
Android, physical Windows 10, GPU or performance certification is implied.
The checkpoint deliberately asserts only the actual early Linux banner and
reports `full_boot_verified=false`. It does not count a synthetic handshake as
Linux boot. The runtime returns an error at the unsupported device rather than
inventing a successful response or a completed boot.

Exact complete serial evidence (774 bytes):

- [`../deliverables/native-kernel/early-boot.log`](../deliverables/native-kernel/early-boot.log)
- [`../deliverables/native-kernel/provenance.json`](../deliverables/native-kernel/provenance.json)
- Serial SHA256: `5989ea3f7a2b625e8c9231ba1d58ebb175ed6555d0871943532fd620b440a9f5`
- Tested bzImage SHA256: `0b08e799e0f15e46d97fc94c5fa1b96be53c61c1dd04e479f47e64c4d0c9864a`
- Linux source: `torvalds/linux`, commit `ffc253263a1375a65fa6c9f62a893e9767fbebfa` (v6.6).
- Kernel build: [45744144583](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45744144583).

The kernel's standard message "BIOS-provided physical RAM map" labels its E820
input; there is NO BIOS executing here. The map was supplied directly by Rust.
The Linux banner's timestamp is the build-runner timestamp, retained verbatim.

## Implemented contract

`rust/nexus-engine/src/kernel.rs` accepts a deliberately bounded subset:

- x86-64 loaded-high bzImage, `AA55` boot flag, `HdrS`, boot protocol 2.12–2.15,
  standard PC hardware subarchitecture, sensible setup-header/payload lengths.
- Kernel and optional initramfs: each 1–64 MiB, regular files; inputs are trusted,
  and header checks are NOT signature/authenticity verification.
- 128–512 MiB page-aligned RAM plan; CLI currently uses 256 MiB and one vCPU.
- Relocatable load alignment; separate compressed-load and preferred runtime
  addresses for nonrelocatable kernels; `init_size` decompression workspace.
- Zero-initialized 4096-byte boot_params at `0x10000`, setup-header copy with
  loader-owned fields reset; three E820 entries, NUL-terminated command line at
  `0x20000`, no inherited setup_data linked list.
- Optional page-aligned initramfs below both RAM and initrd_addr_max, outside the
  kernel load/decompression reservation. This is loader placement, not proof a
  real initramfs has reached userspace.
- Nonempty, nonoverlapping, bounds-checked load regions. No writable guest disk.
- GDT at `0x5000`: Linux __BOOT_CS `0x10`, __BOOT_DS `0x18`, flat 4 GiB segments.
  Protected-mode entry, paging off, interrupts/DF clear, ESI=boot_params,
  EBX/EBP/EDI zero, private stack at `0x90000`.
- Polled COM1 diagnostic subset with divisor latch, status and scratch registers,
  no real host port passthrough. **UART IRQs/timing are NOT implemented.**
- Mode-aware I/O after guest startup transitions between supported 16/32/64-bit
  states; 16-bit protected/compatibility I/O is explicitly unsupported.
- Entire execution bounded by 30 seconds, 250,000 exits and 1 MiB serial output.
  Unsupported accesses stop with an error; cancellation has a hard fail deadline.

Boot protocol reference:
https://github.com/torvalds/linux/blob/ffc253263a1375a65fa6c9f62a893e9767fbebfa/Documentation/arch/x86/boot.rst
and `arch/x86/include/uapi/asm/bootparam.h` at the same revision.

## Developer commands

```
nexus-engine kernel-contract-smoke
nexus-engine inspect-kernel path/to/bzImage
nexus-engine boot-kernel path/to/bzImage NEW-serial.log "kernel command line" [initramfs]
```

`inspect-kernel` is portable header/layout validation only. Execution requires
Windows x64 and an available WHP hypervisor. Serial output must be a NEW file;
existing logs are never overwritten. Keep the console and serial log when a
device blocks startup. These commands are not added to the end-user player yet.
The generic `boot` command still refuses a full Android boot claim.

The real-kernel checkpoint uses a tiny diagnostic configuration and options such
as `noapic nolapic acpi=off pci=off`; these isolate an early bring-up target, NOT
recommended gaming settings, a security-hardened release or a speed optimization.
This old test kernel must not be treated as a production Android guest update.

## Tests and reproducibility

[Foundation run 45744586601](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45744586601)
passed the expanded unit suite, Clippy with warnings denied, release builds and
independent image/CLI checks on Ubuntu 24.04 and Windows Server 2022. There are
20 pure Rust tests including malformed kernel headers, version/alignment/length
refusal, command-line bounds, overlap/overflow and initramfs-placement checks,
nonrelocatable runtime placement, UART divisor behavior and exit-mode detection.

On Windows, the synthetic loader handshake ran three times through WHP. It read
ESI, boot_params/E820, the command line, loaded entry address and optional initramfs
contents inside the guest and verified exact register results. This remains
separate from the actual-kernel checkpoint and from complete Linux boot.
Earlier real/protected/long-mode, paging, software-interrupt, I/O, cancellation
and ZLAG first-sector tests remain green.

`.github/workflows/native-kernel.yml` builds a tiny kernel from the pinned source
with explicit config flags and records config/source/SHA. Windows independently
checks that SHA before AND after execution. The workflow can reuse that exact
kernel artifact while rebuilding the current engine for debugging. No kernel
binaries or large disk images have been committed to the repository.

## Next blocker and retained scope

Implement proper PIT/channel-2 and port-0x61 behavior, then the remaining timer,
PIC/APIC/interrupt and platform-device requirements revealed by actual boot logs.
Do not return constant "ready" values solely to pass the checkpoint. A later
separate gate must verify a real initramfs/userspace marker, stable repeated
boots and shutdown before Linux boot is marked complete.

Android kernel/drivers, a bootable Android `gustimg.zlag` contract, safe disk
persistence, GPU transport plus DirectX/OpenGL/Vulkan rendering, audio, input,
networking and measured Windows 10 performance all remain on the roadmap.
The old player, existing guest bases and personal instance overlays are untouched.
