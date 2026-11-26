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

## PIT progress and 60-second boot target

Commit `6d92774`, real-kernel run **45746596939**, passed three actual
Linux 6.6 early-entry checkpoints with a clock-driven PIT channel 2, binary
mode 0, ports 0x42/0x43/0x61. It uses elapsed monotonic nanoseconds, not an
I/O-count shortcut or constant-ready status. Unsupported PIT modes remain errors.
Linux progressed beyond the previous port 0x61 blocker and stopped at an
unsupported **write to interrupt-controller port 0xA1**, RIP
`0xffffffff81011538`. This is not full Linux or Android boot.

The three engine-main-to-Linux-banner samples were **188, 176, 179 ms**;
engine-main-to-stop samples were **339, 335, 329 ms**. These are hosted
Windows Server 2022 early-kernel measurements, with uncontrolled host caches,
not cold-boot measurements or Windows 10 performance certification. Captured
output also shows `Fast TSC calibration failed`; one sample includes an
unsupported host-derived AMD MSR warning. CPU feature exposure and clock
calibration require additional platform work; clearing port 0x61 does not
certify timing correctness for Android.

The user's approximately **60-second total boot** requirement means emulator
launch to verified usable Android Home. It is a pending engineering target,
not achieved by reaching a Linux banner. Each boot writes a create-new
`<serial-log>.timing.json` with header, VM creation, first vCPU request,
Linux banner, stop, VM exits and PIT counters. The external checkpoint also
measures process launch through exit. Android readiness and target-met values
are deliberately null and `total_boot_verified` is false until real Android
Home readiness exists. Current launch measurement excludes a future player
startup and image-preparation phase, which must be included in the final
end-to-end benchmark.

Evidence: https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45746596939
Next platform work: interrupt-controller initialization and safe advertised
CPU/MSR capabilities, followed by clock/interrupt delivery and userspace.

Foundation run **45746596905** also passed Ubuntu and Windows unit tests,
strict Clippy, release builds and repeated Windows WHP contract checks.

## PIC register and RTC milestone

The following checks supersede the port 0xA1 blocker above:

- `8a92c00`: real-kernel run **45749525762** passed three entries past PIC
  access and stopped at port **0x70**. Foundation **45749525735** passed.
- `2bfb95e`: real-kernel run **45749694804** passed three entries past RTC
  access, now stopping at **PIT control 0x34** (channel 0 rate generator), RIP
  `0xffffffff81132591`. Foundation **45749694948** passed.
- Same pinned kernel SHA256 as earlier. Latest first serial capture was
  6208 bytes, SHA256
  `40658a75f64ea1e01b53f948a3e911866c02a8351180ec7f8716c2a5060e76ed`.
  Full raw logs and timing reports are workflow artifacts, not the earlier
  774-byte committed capture. Individual runs can differ with host CPU/MSR
  exposure; unsupported AMD MSR and TSC warnings remain unresolved.

PIC scope: cascaded edge-triggered 8086 initialization, byte mask register
roundtrips, OCW3 IRR/ISR selection and EOI with no pending interrupts. No
connected interrupt sources, interrupt acknowledge/injection, rotation,
auto-EOI or special-mask modes. Unsupported commands fail. Zero IRR/ISR is
valid only for this disconnected-source subset, not a claim of working IRQs.
PIT channel 0 must be connected to real guest interrupt delivery before Linux
scheduler/tick operation can be certified.

RTC scope: UTC BCD 24-hour calendar, seconds advancing from an initial host
epoch with monotonic engine elapsed time, update-in-progress window, valid
battery flag, NMI-mask shadow and shutdown register. Alarm/periodic interrupts,
time setting and unsupported CMOS registers fail explicitly. No host I/O port
access is performed. Leap date/day rollover, unsupported commands, masks,
initialization and port routing have deterministic tests.

No Android guest ran in these tests. The previously reported app ANRs are
**not fixed or diagnosed** by these chipset changes. Keep the read-only
stability evidence gate and safe cleanup policy in
[guest-stability-and-cleanup.md](guest-stability-and-cleanup.md); do not remove
Android dependencies to conceal engine/device stalls. No guest packages were
removed or new image published in this milestone.

Final routing-test foundation run **45749838738** at `81b2b54` passed both
platforms, including strict Clippy, release builds, chipset width/unknown-port
refusal, upper-RAX preservation and repeated live WHP contracts. The 11 Android
probe/diagnostic unit tests also passed locally; that is not an affected-guest
runtime test.
