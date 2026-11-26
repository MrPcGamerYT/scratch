# Step 1 — native core/platform: active, not certified complete

Work is restricted to core/platform corrections and regression tests. Do not
interpret a successful individual test, Linux PID1, APK installation, or starting
another diagnostic as completion of this step or of the emulator.

## New core findings and verified changes

### Guest-driven CPU bootstrap

Foundation run **45814299392** passed on Windows Server 2022 and Ubuntu 24.04.
The Windows gate executes `transition-smoke` in three fresh WHP partitions.
Starting only in real mode, guest instructions load a GDT, enable CR0.PE, load
CR3, enable CR4.PAE and EFER.LME, enable paging and far-jump into long mode.
Private I/O exits independently identify real16, protected32 and long64.
The guest then stores/loads through a nonidentity page mapping; registers and
physical RAM are checked. This supplements, rather than renames, the previous
host-seeded execution tests.

The same run executes `protection-smoke` three times: a nonpresent-page read and
a supervisor write through a read-only PTE with CR0.WP set must fault into the
guest #PF handler. Two handler returns, final CR2/error code, unchanged protected
RAM, correct subsequent read and restored stack are required. Missing mappings
are not silently allocated or treated as successful accesses.

### Cancellation worker lifetime

Previously watchdog/timer thread handles could detach if their calling action
panicked. Both now use scoped threads, with stop channels inside the scope:
unwinding disconnects the channel and joins the worker before partition/RAM
teardown. `cancel-smoke` injects errors and panics through both paths and then
cancels/reruns the same vCPU. Foundation **45814022581** and later
**45815166802** passed these Windows gates and strict library/Clippy checks.

### High-resolution guest clock

Read-only inspection of the original Linux evidence **45793865474** found:

```
tsc: Unable to calibrate against PIT
tsc: No reference (HPET/PMTIMER) available
tsc: Marking TSC unstable due to could not calculate TSC khz
clocksource: Switched to clocksource refined-jiffies
```

This is a real platform-clock deficiency, not a guest-component dependency or
proof that the guest's SELinux policy causes launcher crashes.

The engine now queries `WHvCapabilityCodeProcessorClockFrequency` and publishes
an exact CPUID.15H ratio. For older basic CPUID ranges, intermediate synthetic
leaves explicitly report zero features; vendor identity is preserved. Windows
SDK assertions verify the capability/property constants and record layout.
Linux on AMD does not consume the Intel CPUID15 calibration interface, so the
native Linux loader also passes actual WHP frequency via `tsc_early_khz`.
It checks command-line capacity/overlap and rejects conflicting user frequency
claims. No nominal CPU MHz, forced `tsc=reliable`, invented clock frequency or
change to IRQ tick timing is used.

The initial frequency gate **45814724797** exposed an older CPUID range; the
next **45814993626** exposed AMD's different calibration path. Neither failure
was waived. After the compatibility corrections, **45815166044** passed.

Final Linux regression **45815493260** (code **9ae7579**) passed:

- Three Linux PID1 boots, real IRQ0/nanosleep and separate-boot block persistence.
- Explicit TSC clocksource selection, without an unstable-TSC report.
- In each PID1 fixture, **4096 actual CLOCK_MONOTONIC syscalls**, no backward
  timestamps, and at least one positive consecutive delta below one millisecond.
  Equality is allowed; a stuck or jiffy-only clock cannot satisfy the gate.
- COM2 query boots, exact software-framebuffer captures, ext4 clean remount and
  unclean-VM recovery gates.

Example captured selection (not a promised CPU frequency for another host):

```
tsc: Detected 2596.147 MHz processor
clocksource: Switched to clocksource tsc-early
```

These observations validate the tested Linux/WHP profile. They do not establish
all kernel versions support the handoff, repair every Android freeze, or certify
Windows 10 physical hardware.

## Remaining core/platform acceptance work

- SMP/AP startup and local-APIC/IOAPIC routing/timer implementation and tests.
  Current boot profile is one vCPU with the PIC/PIT and explicit no-APIC flags.
- A reviewed virtual CPU capability/topology contract beyond the clock leaf;
  do not imply arbitrary host feature exposure proves device support.
- Clock correctness/compatibility across supported guest kernels and physical
  Windows 10 CPU/hypervisor versions, including migration/suspend/reset behavior.
- Broader instruction/MMIO/device fault/reset stress and resource failure tests;
  existing tests cover a bounded, deliberately restricted machine profile.
- Sustained/repeated machine lifecycle validation and explicit hardware matrix.

Disk base images remain read-only; disposable COWs are used for guest writes.
Existing editor/ISO workflows and user disks are not modified. Accelerated
GPU, networking/audio implementation, Android Home repair and package removal
are not part of this increment. No final core-complete or full-emulator claim.

### PIT one-shot activation and final regression

The Android core-regression **45815813984** selected `tsc-early` but correctly
failed on unsupported PIT control **0x38**. Enabling a high-resolution clock
made Linux activate channel0 **mode4 software-strobe one-shot** clock events.
The engine now implements that mode: one rising IRQ after the one-tick low
strobe, no repeated event until reload, and an incomplete LSB/MSB reload does
not prematurely cancel an already armed mode4 deadline. Integer-nanosecond
boundary tests use the actual reload epoch (initial test-only rounding error
was corrected rather than relaxing the timer behavior).

The native IRQ fixture now runs both mode0 and mode4: PIT -> PIC IRQ0 -> actual
WHP interrupt -> halted vCPU wake -> guest EOI/IRETD, with exact counter/stack
checks. Latest production code **6b7af44** passed:

- **45816416883**: foundation, both hosts, strict tests/Clippy/release; Windows
  CPU transitions, protection faults, both one-shot IRQ modes and worker
  error/panic recovery.
- **45816622988**: Linux high-resolution-clock/PID1/storage/COM2/framebuffer/ext4
  regressions using the pinned kernel from **45814022595**.
- **45816617671**: existing Android signed-APK install/query/uninstall regression
  using the unmodified derived assets from **45806838925**. Last-ANR reports no
  ANR in the short capture; the captured events contain no `am_crash` records,
  unlike the pre-clock-fix sample. Home still resolves to ResolverActivity.
  This is **not** certification of Launcher3, Android Home, all ANRs or Step 5.

The high-resolution clock fix is verified for these tested guest profiles;
SMP/APIC and the other remaining core acceptance items above are not waived.
