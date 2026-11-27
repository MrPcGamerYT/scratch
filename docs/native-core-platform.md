# Step 1 — native core/platform: active, not certified complete

Work is restricted to core/platform corrections and regression tests. Do not
interpret a successful individual test, Linux PID1, APK installation, or starting
another diagnostic as completion of this step or of the emulator.

## Configuration contract — standing requirement

Selected CPU count, RAM and renderer must govern the actual VM/device creation,
boot and runtime paths, not just labels in Settings. Unsupported combinations
must be rejected explicitly, not silently ignored or downgraded. Renderer
selection must identify a real rendering path; software scanout/WARP is not
proof of accelerated guest graphics. The player/UI configuration still needs
end-to-end wiring and acceptance for these requirements; the native CLI tests
below do not claim that integration is finished. Do not treat CPU/RAM changes
as hot-plug support: the verified settings apply when constructing a fresh VM.

## Latest verified scope: SMP/APIC disk DMA and COM2

Final code **2cd8fa9** passed foundation **45849986875**, actual Linux APIC
**45850222110**, and legacy Linux/PIC regression **45850990588**. The APIC OS
gate now runs **19 independent boots**: the existing 10-boot selected CPU/RAM
matrix, plus nine raw/ext4 persistence boots with 1/2/4/8 CPUs, actual block
DMA rendezvous, real IOAPIC disk interrupts, and exact private COM2 handshake.
CPU count and RAM remain effective VM inputs. Headless Linux APIC disk/bridge
mode is now accepted; Android, APK, player/renderer paths are not certified.
This is still **not completed Step 1**: full reset/failure/concurrency stress,
advanced interrupt semantics and physical Windows 10 acceptance remain.

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


## Continued Step 1: topology and two-vCPU startup (not full SMP boot)

Code **f71fa31** keeps this step active. Production boot still uses one vCPU;
there is deliberately no claim that Linux/Android can yet boot with SMP/APIC.

- Added indexed vCPU register/lifetime management, correctly deleting each vCPU
  by its own index. Whole-partition watchdog cancellation now covers all created
  vCPUs, not just CPU0.
- Added a reviewed one-socket/one-thread-per-core topology contract. Replies use
  WHP-filtered defaults, rewrite virtual core/APIC IDs and counts, clamp cache
  sharing counts, and remove unsupported MONITOR/VMX/SVM/x2APIC/TSC-deadline
  claims. CPUID.1, .4, .B/.1F and AMD topology leaves agree. Unit tests cover
  configurations 1..8; real guest conformance covers the one/two-vCPU profiles.
  This does not certify every other host feature bit or every CPU model.
- `smp-smoke` is an explicit experimental startup fixture, not a production SMP
  scheduler. BSP guest MMIO writes issue INIT/SIPI through the SDK instruction
  emulator to the startup frontend. A dormant AP gets architectural CS:IP only
  after these commands; two real WHP vCPUs execute, exchange shared RAM and
  independently store CPUID results. No host writes a successful AP marker.
- Foundation **45820887975** and **45821109570** passed **48** two-vCPU
  create/start/handshake/teardown cycles each, with distinct guest-observed CPU
  IDs and matching logical counts. Unit/Clippy/release and pre-existing native
  CPU/timer/MMIO/storage safety gates passed on both CI hosts as applicable.
- Linux **45820893306** passed the clock, boot, block, ext4, bridge and software
  display regressions. Android regression **45820887569** passed actual signed
  APK install/presence/uninstall using unchanged derived assets **45806838925**.

The failed intermediate tests were investigated, not skipped: opaque WHP xAPIC
startup ran the AP incorrectly when entered too early; the newer INIT/SIPI trap
flag was rejected by Server 2022 (`0x80070032`). The working explicit startup
frontend does not depend on that newer API. Real-mode AP handshake buffers also
stay below 64KiB to respect segment limits. Unsupported APIC operations fail
explicitly; this startup subset does not pretend to provide fixed IPIs, LAPIC
interrupt priorities/timers, IOAPIC routing or arbitrary AP restart.

**Still required before core completion:** production SMP scheduling and boot
enumeration, full LAPIC/IOAPIC/timer routing with OS-level acceptance, reset and
failure stress beyond the tested subset, and the physical Windows 10 matrix.
No Step 2 work or completion declaration is implied by these results.

## Sequential blocker: LAPIC fixed-interrupt routing

This increment worked only on LAPIC routing; it did not start IOAPIC or timers.
The production Linux/Android entry path is still PIC/one-vCPU. The new routing
path is exercised by the explicit two-vCPU native acceptance machine, not yet
an OS SMP boot profile.

Implemented `src/lapic.rs` with edge-request IRR, ISR, TPR/PPR arbitration,
software enable, EOI, physical destinations, flat logical LDR/DFR destinations
and self/all/all-except-self shorthand. Invalid vectors/targets/modes are
checked before changing destination IRRs. Priority classes block same/lower
class nesting. Requests remain pending while TPR or the CPU interrupt window
blocks delivery; ISR acknowledgement occurs only after WHP accepts injection.

The native test now makes both guest vCPUs execute actual interrupt handlers:

- BSP masks its priority class, queues a self-IPI and reads retained IRR/empty
  ISR; unmasking invokes the handler, which increments guest RAM and EOIs.
- A second self-IPI is queued under CLI, then delivered after STI without
  requiring another guest device access. Scheduling cancellation only allows
  the host to inspect interrupt windows; it does not fabricate a guest IRQ.
- BSP targets AP1 physically, then through its guest-programmed logical mask.
  AP1 must run its handler exactly twice, EOI and IRETD with its stack restored.
- Both IRR delivery state and ISR must be clear after the guest EOIs; BSP's
  stack is checked as well. Halted APs sleep until a deliverable interrupt;
  cancellation alone does not wake them or skip the instruction after HLT.
- The guest CLI/check/STI/HLT idle sequence is used on the AP. Each 16-cycle
  batch must actually exercise at least one halt/wake, not merely busy-loop.

The initial cross-CPU test exposed AP MMIO accesses through real-mode 64KiB
segments. Rather than broadening segment limits to hide the fault, AP startup
now executes LGDT, sets CR0.PE and far-jumps into flat protected32, installs its
own IDT, then accesses LAPIC MMIO and enables interrupts.

Verification: **45823248385** passed local IPI delivery; **45824852517** passed
physical remote delivery after the AP bootstrap correction; **45825222251**
passed physical/logical targeting and priority masking; **45825527510** passed
IF deferral; final **45826050006**, code **8b639fa**, passed both-host strict
unit/Clippy/release gates and **48 native two-vCPU cycles** with halt/wake checks.
Existing Linux regression **45825760525** passed clock/boot/storage/ext4/bridge/
software display gates after the interrupt-window rendezvous changes.

Still not implemented/verified by this routing gate: IOAPIC, level-source EOI,
LAPIC timers, NMI/ExtINT/cluster modes, production SMP scheduling/OS boot,
firmware CPU enumeration, and physical Windows 10 certification. Unsupported
operations fail explicitly. This supersedes only the earlier INIT/SIPI-only
frontend limitation, not the remaining core-completion requirements.

## Sequential blocker: LAPIC countdown timers (2026-11-24)

The next increment adds one-shot and periodic countdown timers to the same
explicit two-vCPU frontend. `lapic.rs` exposes LVT Timer (0x320), Initial Count
(0x380), read-only Current Count (0x390), and Divide Configuration (0x3e0).
The virtual APIC bus is 100 MHz, with all eight divide encodings (1..128).
Host monotonic elapsed nanoseconds advance each CPU's timer at rendezvous and
MMIO boundaries, including while the CPU is halted. Initial-count writes reload
and zero cancels. Divider writes restart the fractional prescaler. Periodic
expiry reloads with the elapsed remainder, not from the late host wake time;
missed periods coalesce into one edge in IRR rather than creating an IRQ storm.
LVT masking suppresses interrupt creation without stopping the countdown.
TSC-deadline mode is explicitly rejected, not silently treated as one-shot.

Deterministic model tests cover fractional countdown, all divide values,
one-shot non-retrigger, periodic reload, masked expiry, APIC software disable,
IRR coalescing while ISR blocks the priority class, reprogramming, zero-count
stop, invalid mode and read-only Current Count. The existing interrupt arbiter
continues to govern priority/IF deferral and acknowledgement.

Native acceptance **45827049269** (code **cda39ad**) passed Ubuntu and Windows
strict gates. The CLI repeats **48 two-vCPU cycles**, each requiring one guest
one-shot timer IRQ and two periodic IRQs in addition to the existing two BSP
self-IPIs and two remote AP physical/logical IPIs. The guest waits using
CLI/check/STI/HLT; its vector-40 handler masks the periodic LVT at the exact
expected count and EOIs. Guest code then writes zero to Initial Count. Host
assertions require exact counters, restored stacks, empty IRR/ISR on both CPUs,
and a masked/stopped BSP timer. The initial one-shot increment also passed
**45826761863** before periodic coverage was added.

Existing Linux entry regression **45827319947** passed after these changes,
reusing kernel artifacts from **45814022595** (the kernel-build job is skipped
on reuse; the early-entry acceptance job passed).

Scope: this is a tested virtual countdown timer, **not OS LAPIC-timer/SMP
acceptance**. Production Linux boot still uses the single-vCPU/PIC profile.
Physical Windows 10 timing/stress, firmware enumeration, production SMP,
IOAPIC/level EOI and actual OS LAPIC timer calibration remain required. This
supersedes earlier statements that the explicit frontend has no timer; it does
not complete Step 1 or authorize moving to graphics/Step 2.

## Sequential blocker: IOAPIC fixed routes and level EOI (2026-11-24)

`src/ioapic.rs` now models a 24-input IOAPIC with IOREGSEL/IOWIN, ID/version,
masked reset redirection entries, fixed physical and flat-logical destinations,
input polarity, edge latching, level assertion and remote IRR. Unsupported
lowest-priority/NMI/ExtINT modes are rejected rather than silently rerouted.
A level request cannot be reissued while remote IRR is set. LAPIC acknowledgement
tracks the incoming edge/level trigger separately from queued requests and
exposes in-service trigger bits. Only a level interrupt's guest EOI is broadcast
to IOAPIC; edge IPIs and timer EOIs must not clear remote IRR. The broadcast
matches the vector captured at delivery, even if the route was reprogrammed
before EOI. A still-active unmasked source then reasserts; masking stops further
delivery, and deassertion before EOI prevents reassertion.

The two-vCPU frontend now emulates IOAPIC MMIO at FEC00000/FEC00010 under the
same lock as the LAPIC fabric. Its **test-only synthetic device source** holds
input 0 high, initially masked. Guest BSP programs its own vector-42 IDT gate
and unmasks the fixed level route through real MMIO. Its handler must run twice:
the first EOI permits the held-level source to reassert; on the second entry
the guest masks the route and EOIs. Exact guest RAM count, restored stack,
empty IRR/ISR and cleared remote IRR with a masked route are mandatory. This is
not a claim that an existing production device has been migrated to IOAPIC.

Pure tests additionally cover physical targeting of AP1, logical edge routing,
active-low input, wrong-vector EOI, deassertion before EOI, masked/unmasked
level input, route reprogramming while in flight, nested edge/level LAPIC
metadata, and register/vector validation.

Verification: model **45829439500** passed both platforms; first native held-
level gate **45829745163** passed; final code **6ccdd44**, foundation
**45830093440**, passed Windows/Ubuntu strict gates and **48 native two-vCPU
cycles** retaining prior startup/topology/IPI/timer coverage. Linux regression
**45830321717** passed its early-entry acceptance job, reusing kernel artifacts
**45814022595** (kernel build skipped, not rebuilt).

Remaining Step 1 scope: production interrupt-source integration, firmware/CPU
and interrupt-controller enumeration, OS LAPIC timer calibration and IOAPIC
acceptance, production SMP scheduling/boot, reset/failure stress and physical
Windows 10 certification. The production OS profile is still single-vCPU/PIC.
Logical IOAPIC edge routing and AP physical routing are model tests; the native
IOAPIC fixture specifically proves BSP physical **level** routing. Do not
interpret the bounded fixture as full APIC/OS SMP completion. This section
supersedes the earlier absence of an IOAPIC model/frontend, not those remaining
acceptance requirements. No Step 2 work was started in this increment.

## Sequential blocker: real platform sources into IOAPIC (2026-11-24)

`EarlyPlatform::route_ioapic` now mirrors the existing device models into the
APIC profile: PIT IRQ0 -> GSI2 (the conventional PC override), COM2 -> GSI3,
COM1 -> GSI4, and caller-provided block IRQ -> GSI5. PIT monotonic edge counters
are sampled with an explicit cursor, so repeated routing cannot duplicate the
same edge. UART and block sources use their asserted/deasserted state. The
legacy PIC continues observing sources, but a runner must select exactly one
interrupt-injection profile; the APIC frontend never injects from the PIC.

The two-vCPU native runner now handles real guest IN/OUT accesses through the
same `EarlyPlatform` used for OS boot, with indexed register updates and proper
CPU-mode-specific RIP advancement. In addition to the previous synthetic
held-level test, the guest now performs these actual source sequences:

- Programs IOAPIC GSI2/vector43 and PIT mode0/count11932 through ports 43/40,
  waits in CLI/check/STI/HLT, and receives exactly one PIT interrupt. The host
  requires one actual PIT edge and one consumed routing-cursor edge.
- Programs GSI4/vector44 as a level route, enables COM1 OUT2 and THRE interrupts,
  then receives an interrupt from the existing UART model. The guest handler
  reads IIR (expected THRE ID2), clearing the device source **before** LAPIC EOI.
- After the first UART return, guest OUT to THR sends `U` and must cause exactly
  one additional interrupt. The handler preserves EAX/EDX and IRETD restores
  its stack. Final checks require count2, IIR2, output exactly `U`, a deasserted
  UART source, cleared IOAPIC remote IRR and empty LAPIC IRR/ISR.

Additional pure adapter tests prove repeated routing does not duplicate a PIT
one-shot edge and a block-source deassertion before EOI prevents reassertion.
COM2/block mappings exist in the adapter, but this native gate does **not**
certify an actual block completion or COM2 bridge transaction through IOAPIC.

Verification: initial native source gate **45831022503**, then final code
**cc8e41a** / foundation **45831322640**, passed both Windows and Ubuntu strict
build/test gates, including **48 two-vCPU native cycles**. Existing Linux
regression **45831555672** passed early-entry acceptance using reusable kernel
**45814022595** (kernel build skipped). Automatic kernel rebuilds
45831022301/45831322402 were canceled in favor of running the focused native
gate first, then the single artifact-reusing Linux regression sequentially.

This integrates real production device models into the APIC runner; it does
**not** switch the production OS boot profile to APIC or prove OS enumeration.
Production boot remains single-vCPU/PIC until firmware/enumeration and OS
LAPIC/IOAPIC acceptance are implemented and verified. Step 1 still also needs
production SMP, reset/failure stress and physical Windows 10 acceptance. No
Step 2 work was started.

## Sequential blocker: APIC-machine discovery data (2026-11-24)

`src/firmware.rs` now generates Intel MP Specification 1.4 discovery data for
the explicit APIC machine, not a BIOS/UEFI implementation. The 16-byte-aligned
floating pointer is in the legacy BIOS search area at F0000, pointing to a
checksummed PCMP configuration at F0100. Tables fit below 1 MiB, inside the
existing loader's reserved E820 hole; they are currently installed only in the
native APIC fixture, **not** the OS loader's PIC profile.

The table describes 1..8 enabled CPUs with APIC IDs matching virtual CPUID,
exactly one bootstrap CPU, LAPIC FEE00000, IOAPIC ID8/version11 at FEC00000, an
ISA bus, and 15 fixed ISA interrupt routes. IRQ0 -> input2 matches the real PIT
source adapter; IRQ2 is the absent cascade, not a conflicting second input2
route. Other ISA IRQs map to corresponding inputs. Conforming ISA routes are
active-high/edge; software may reprogram supported routes to level (as the UART
fixture does). No unsupported NMI/ExtINT local-interrupt routes are advertised.
CPU signature/feature fields are zero rather than inventing a host signature;
virtual CPUID remains authoritative. No ACPI, AML, power-management or PCI
firmware interfaces are supplied by this increment.

Pure tests independently walk all entries for every CPU count 1..8, validate
length/checksums, CPU IDs/bootstrap flags, IOAPIC identity/address, IRQ routes
and table bounds, and reject unsupported CPU counts. In the native two-vCPU
gate the guest itself scans the legacy BIOS area in 16-byte steps, finds `_MP_`,
reads the configuration pointer, computes both checksums, walks the variable-
length CPU/controller entries, and records enabled CPU count/ID bitset and
controller addresses. It accesses the discovered controller addresses through
actual MMIO, reading IOAPIC ID8/version/input-count and the BSP LAPIC ID. Host
assertions require all of those values to agree with the actual two-vCPU
machine and the existing guest CPUID/startup checks; host table serialization
alone cannot satisfy the gate.

Verification: initial tables/parser **45832112162** passed; final native
discovery/controller-identity gate **45832450342** (code **4f9b81c**) passed
Windows/Ubuntu strict tests/build and **48 two-vCPU cycles**, retaining real
PIT/UART, IOAPIC level EOI, LAPIC timer and IPI coverage. Existing Linux
regression **45832722130** passed early-entry acceptance, reusing kernel
**45814022595** (kernel build skipped).

Remaining blocker: connect an APIC-capable OS boot profile to these discovery
tables, interrupt/MMIO runner and AP lifecycle, then obtain actual OS CPU/
controller enumeration, timer calibration, interrupt and SMP acceptance. The
existing Linux regression deliberately remains single-vCPU/PIC with APIC and
ACPI disabled; it does **not** certify these tables under Linux. Reset/failure
stress and physical Windows 10 acceptance also remain. Step 1 is incomplete;
no graphics/Step 2 work was started.


## Sequential blocker: actual Linux APIC boot acceptance (2026-11-24)

Added an explicit `--apic-probe` option to `boot-kernel` only. It constructs a
one-CPU APIC-capable partition, installs matching MP discovery data in the
reserved BIOS hole, loads the real bzImage/initramfs, and executes the same
controller/MMIO/device frontend used by the native two-CPU acceptance machine.
It records real guest serial output and stops only after PID1's ready marker;
controller accesses and actual interrupt injections are additionally mandatory.
Disk, Android, bridge, player and framebuffer combinations are rejected before
starting this profile, and the disk entrypoint also rejects an APIC plan before
opening/creating its COW overlay. The existing PIC profile remains the default.

New `.github/workflows/native-apic.yml` builds pinned Linux 6.6 with SMP,
X86_MPPARSE, X86_LOCAL_APIC and X86_IO_APIC enabled. The kernel artifact from
**45834154987** is reusable as `native-apic-linux-kernel`. Its successful kernel
build must not be confused with that run's failed first OS invocation (the test
initially needed its runpy import path fixed). `native_apic_checkpoint.py`
performs eight independent VM/process boots with maxcpus=1, not eight CPU
startups in one Linux instance. APIC is not disabled and Linux's timer-route
validation is not bypassed. Acceptance requires MP/IOAPIC enumeration, PID1's
ready marker and two sleep/tick markers, positive LAPIC timer injections, no
panic and no unchecked MSR access error.

Actual OS bring-up exposed and corrected these dependencies, sequentially:

- Unsupported MSR accesses now inject architectural #GP(0) at the faulting RIP
  rather than returning invented values or aborting safe guest probes. The
  WHP pending-exception layout is SDK-asserted. Native guest RDMSR and WRMSR
  tests require two #GP fixups, zero error code, preserved EAX/EDX and restored
  stack. An existing pending exception also blocks external IRQ injection.
- The virtual APIC base MSR reports the fixed xAPIC base/enable/BSP state;
  relocation/disable requests remain explicitly unsupported. Unsupported AMD
  OSVW capability is hidden; read-only virtual HWCR supplies the invariant-TSC
  bit consistent with the virtual TSC contract. This is not host MSR passthrough.
- Non-timer LVT configuration and ESR reset/read are retained for Linux setup;
  absent local NMI/ExtINT/error/performance sources are not generated. Linux's
  8259 auto-EOI setup is now supported and separately unit-tested, including
  reinitialization back to normal EOI. The APIC runner does not inject PIC IRQs.
- IOAPIC lowest-priority mode now actually selects one matching destination by
  PPR (stable tie breaking), with tests switching the recipient as TPR changes.
  Masked dormant route configurations are retained; unsupported active delivery
  modes are not silently converted to fixed delivery.
- A first Linux boot reached PID1 but a repeat failed its real PIT/IOAPIC timer
  check. Requested 1ms rendezvous intervals could be rounded by Windows's
  default scheduling resolution. A scoped WinMM timeBeginPeriod(1) request now
  covers the VM timer worker, with timeEndPeriod on normal/error/panic teardown.
  No extra guest ticks were fabricated and no Linux timer check was disabled.

Verification progression: **45837226484** first passed three real OS boots;
**45837933608** passed the stricter eight-boot gate; final **6096f2d** passed
**45838588570** with eight boots and positive LAPIC timer counts. Visible sample
summaries include 514 IOAPIC MMIO accesses, 144/151 interrupt injections and
27 LAPIC timer injections (sample data, not a performance benchmark).
Foundation **45838293945** passed both platforms and the 48 two-CPU native
cycles including the new MSR-fault recovery tests. Legacy PIC Linux regression
**45838866459** passed early-entry acceptance reusing **45814022595**, including
its existing clock/storage/ext4/bridge/software-display checks.

Still required: actual Linux AP startup/SMP scheduling and cross-CPU interrupt
acceptance, APIC disk/bridge integration and OS device-interrupt acceptance,
reset/failure stress and physical Windows 10 coverage. ACPI/UEFI interfaces and
NMI/ExtINT delivery are not supplied by this profile. These results supersede
older statements that Linux APIC enumeration/timers are entirely untested, but
not the remaining Step 1 completion criteria. No Step 2 work was started.


## Sequential blocker: real Linux SMP and effective CPU/RAM settings

Extended the APIC OS path from one CPU to the selected 1..8 CPUs. The same
count is used for WHP vCPU creation, virtual CPUID topology, MP discovery entries,
LAPIC instances and dormant AP workers. AP register state starts in real mode;
only actual guest INIT/SIPI commands set the requested CS:IP and start an AP.
Linux runs its own trampoline and mode transitions; the host does not skip to
Linux's secondary entry. Physical startup destinations and INIT all-except-self
routing are supported. Level INIT deassert is not misinterpreted as another
reset. This is initial startup, not complete INIT/reset/hotplug semantics.

All AP run loops share the same controller/device fabric, while vCPUs execute
concurrently outside its lock. Fixed IPIs, EOI and LAPIC timers use the existing
runtime path. Shared stop state stops APs when BSP reaches the diagnostic goal
or a worker fails. A scoped stop guard is dropped before implicit thread joins
on early setup errors and unwind; workers join before RAM/partition teardown.
No host serial/device state is read after teardown while an AP can still run.

The diagnostic PID1 now optionally calls sched_setaffinity for each selected
CPU and then getcpu to verify that CPU is actually executing it. A wrong count,
failed migration or incorrect reported CPU faults PID1 rather than printing the
SMP marker. The generated ELF contains writable affinity/result buffers, with
new deterministic/oracle tests for counts1..8. Existing no-affinity initramfs
fixtures remain unchanged and their six local structural tests pass.

`--cpus` rejects empty, signed, out-of-range and duplicate settings. More than
one CPU without the explicit APIC profile is rejected before input file I/O;
KernelPlan independently rejects inconsistent count/profile combinations.
No setting is silently accepted while creating just one WHP vCPU. Renderer and
Android/player integration are not added by this CPU/RAM change.

Verification progression:

- **45840915520**: first multi-CPU-capable runner passed strict foundation gates.
- **45841266420**: three real two-CPU Linux boots passed; each required both
  CPUs online and PID1 migrating onto CPU0 and CPU1.
- **45841740658**: first complete selected-core/RAM matrix passed.
- Final code **76c08de**, **45842185302**: Ubuntu/Windows foundation passed,
  retaining 48 native dual-vCPU interrupt/timer/discovery/MSR-recovery cycles
  and adding CPU configuration validation tests.
- **45842479962**: final **10 independent Linux boots** passed, with this matrix:

| Selected CPUs | Selected RAM MiB | Required guest proof |
|---:|---:|---|
| 1 | 128 | online count, E820 limit, PID1 pinned/getcpu on CPU0 |
| 2 | 256 | same, executing PID1 on CPU0..1 |
| 3 | 384 | same, executing PID1 on CPU0..2 |
| 4 | 512 | same, executing PID1 on CPU0..3 |
| 5 | 640 | same, executing PID1 on CPU0..4 |
| 6 | 768 | same, executing PID1 on CPU0..5 |
| 7 | 896 | same, executing PID1 on CPU0..6 |
| 8 | 1024 | same, executing PID1 on CPU0..7 |
| 2 | 256 | independent repeat boot and migration |
| 2 | 512 | independent repeat with changed RAM |

Every sample additionally requires actual IOAPIC enumeration/accesses, positive
LAPIC timer injections, PID1 sleep/tick and ready markers, no panic and no
unchecked MSR access errors. This is sequential per-CPU task migration, not
simultaneous multi-process stress or a performance benchmark. The RAM parser
accepts up to 3072 MiB, but this SMP OS matrix verifies only the sizes listed.
All APIC OS runs reuse the SMP-capable kernel **45834154987**.

Legacy PIC Linux regression **45842774985** passed clock/storage/ext4/COM2/
software-display acceptance after the final SMP changes, reusing legacy kernel
**45814022595**. Automatic kernel rebuilds were canceled in favor of the focused
native/APIC gates followed by this artifact-reusing regression, sequentially.

Remaining Step 1: APIC disk/bridge/device OS integration, concurrency and reset/
failure stress, CR8/advanced interrupt semantics as needed by broader workloads,
and physical Windows 10 acceptance. Android SMP, renderer setting integration,
accelerated guest GPU and player settings are not certified. These results
supersede the earlier absence of actual OS SMP startup/scheduling acceptance,
not the remaining platform or complete-emulator criteria. No Step 2 was started.


## Sequential blocker: APIC disk DMA and COM2, including multiple CPUs

Extended `boot-kernel-disk-create/open ... --apic-probe --cpus=N` to use the
APIC controller runner and existing validated VirtIO MMIO/block/COW backend.
The first stage deliberately enforced one CPU and passed before lifting that
restriction. Block queue service is outside synchronous emulator callbacks,
not an unbounded callback-side device operation. Device IRQ state now feeds
IOAPIC GSI5. Acceptance requires actual queue completions, MMIO accesses and
positive GSI5 deliveries, not merely a bound driver or a startup marker.

### SMP DMA synchronization

Every vCPU holds a shared guest-RAM gate **only while inside WHP Run**. Host
block service takes the exclusive gate before constructing a mutable guest-RAM
view or examining/updating queues. The existing 1ms cancellation worker brings
running CPUs out of WHP; their guards drop before they attempt the fabric lock.
The writer waits until all vCPUs are stopped and prevents reentry until device
service completes. Guest-memory fetch/translation callbacks participate in the
same gate without holding it while acquiring the controller fabric, avoiding
a lock-order inversion. RAM/queue views drop before the exclusive guard.
Block backend and queue state remain serialized by the fabric lock. Completed
rendezvous are counted and reported in native disk evidence. No claim is made
that this stop-all DMA path is optimized or benchmarked.

### Private bridge device

The query-only `--bridge-probe` now works in headless Linux APIC mode, including
with SMP disk I/O. The existing COM2 model/client is advanced in the serial
observer, its interrupt source feeds GSI3, and successful termination requires
both guest PID1 ready and the actual host client query result. Guest code checks
the exact QUERY_APP request before replying. Host acceptance requires app
`org.nexus.fixture`, a complete result, and positive GSI3 deliveries. This is a
Linux test fixture, **not an Android app, APK install or launcher acceptance**.
Android/APK/player/framebuffer combinations remain explicitly rejected.

### Verified progression and final matrix

- **45847490309**: strict foundation passed the first APIC disk frontend.
- **45847885723**, code **8ee39d6**: 10 core/RAM boots plus nine **single-CPU**
  raw/ext4 disk/persistence boots passed before SMP DMA was enabled.
- **45848339143** and **45848611300**: scoped all-vCPU DMA synchronization
  passed foundation and the complete SMP disk matrix respectively.
- **45849149467** and **45849399406**: COM2 integration passed foundation and
  all 19 boots with the exact bridge query added to every disk boot.
- Final **2cd8fa9**, **45849986875**: Windows/Ubuntu strict tests, SDK ABI,
  release and native dual-vCPU interrupt/timer gates passed.
- Final **45850222110**: all 19 Linux boots passed, including this disk matrix:

| Sequence | First boot | Second boot | Third boot |
|---|---|---|---|
| Raw pattern | 1 CPU / 128 MiB: write, fsync, read/compare | 2 / 256: reopen/read/compare | 8 / 1024: reopen/read/compare |
| Clean ext4 | 4 / 512: file+directory fsync, unmount | 2 / 256: readonly remount/read/unmount | 8 / 1024: readonly remount/read/unmount |
| Mounted ext4 VM stop | 8 / 1024: file+directory fsync, stop mounted | 4 / 512: require journal recovery, read/compare | 2 / 256: readonly remount/read/compare |

For every disk sample PID1 first migrates through **all selected CPUs**, ending
pinned on the last CPU; subsequent disk and bridge work therefore exercises AP
execution and indexed callbacks when N>1. Every sample requires the exact data
marker, PID1 ready, bound virtio-blk device, actual disk/GSI5 work, exact COM2
request/result/GSI3 work, unchanged base/source SHA256 and no disk error.
The mounted-VM-stop case is **not host power-loss or arbitrary failure injection**.
All APIC OS tests reuse the SMP-capable kernel **45834154987**; no QEMU is used
by this Rust/WHP execution path.

Legacy PIC Linux regression **45850990588** passed its clock/storage/ext4/COM2/
software-display gates after final changes, reusing **45814022595**. Its kernel
build was skipped on artifact reuse, not rebuilt. Six local Python initramfs
structural tests also pass. Superseded automatic legacy kernel rebuilds and old
long-running generic legacy QEMU builds were canceled rather than treated as
part of independent-engine acceptance; the editor/ISO code was not removed.

### Startup limitation made explicit

First SIPI delivery commits AP startup state under the routing lock. Duplicate
SIPIs to an already-started CPU are ignored architecturally. At this checkpoint,
INIT after startup failed explicitly rather than silently ignoring a reset.
**That runtime-INIT rejection is superseded by the target-owned reset work below**;
OS hotplug and full architectural reset semantics remain uncertified.

Remaining Step 1 work includes broader runtime-reset and fault/interruption and
long-duration concurrency coverage, broader CR8/advanced interrupt
semantics and physical Windows 10 testing. Android SMP/disk/bridge, renderer
selection, player settings integration and accelerated guest GPU remain outside
this gate. Earlier statements that APIC disk/COM2 or SMP DMA are wholly absent
are superseded only to the exact verified scope above; Step 1 is not complete.


### DMA failure/unwind and concurrent direct-I/O acceptance

Code **f16c14b** passed strict Windows/Ubuntu foundation **45856344205**,
APIC OS **45856566117**, and legacy PIC Linux **45857282066**. Both OS gates
reused the pinned kernel artifacts above. The APIC gate now comprises the
previous 19 boots, **12 injected failures plus 12 same-overlay recovery boots**,
and **four concurrent-I/O/persistence boots** (47 VM invocations total).
Seven local Python initramfs structural tests pass.

`--test-dma-failure=error|panic|error-after|panic-after` is deliberately limited
to APIC SMP disk-create using a **new disposable overlay**, without Android,
bridge or player options. At 2, 4 and 8 CPUs, error/panic is injected either
on BSP before queue service or on the last AP after nonempty queue service,
while holding the exclusive RAM gate and device/fabric lock. The latter
exercises worker panic/unwind and poisoned-lock cleanup, not just BSP return.
Every case requires a bounded nonzero exit, all APs started, zero AP workers
remaining, a released RAM gate, final VM-resource-drop evidence and no reported
WHP teardown API errors. A fresh SMP VM then opens that very overlay, migrates
through every CPU and compares seeded disk data. The immutable base hash must
remain unchanged. This is **not a mid-sector write, host power-loss, or runtime
reset test**; the fault is at an explicit pre/post-service boundary.

The concurrency fixture forks 4/8 separate child processes before waiting;
each pins to its own selected CPU. Each performs 16 iterations of aligned
512-byte `O_DIRECT` pwrite, fsync, pread and exact compare in a distinct disk
region. PID1 requires every child to exit successfully. A fresh VM reopens the
same overlay and concurrently checks all persisted per-CPU patterns using
direct reads. Positive disk IRQ/completion evidence and immutable-base hashes
remain mandatory. Structural tests check fork counts, readonly recovery and
4096-byte user-buffer alignment; the guest run supplies actual execution proof.

The first concurrent workload **failed** in **45855542117**. Inspection exposed
a possible ACK/completion race: if a CPU acknowledged the block interrupt and
another completed more work before the next host tick sampled the source,
an edge-triggered GSI5 could remain logically high and lose a new rising edge.
**f16c14b** now updates the block IRQ line under the same fabric lock as device
MMIO acknowledgement, and samples platform IRQ changes under its I/O lock.
The unchanged concurrent workload then passed along with the complete matrix.
This is bounded functional coverage, not a throughput benchmark or exhaustive
interrupt/reset/concurrency certification. Runtime INIT was explicitly rejected
at that checkpoint (superseded below); physical Windows 10 acceptance and the other Step 1 gaps above remain
open. No Android Home, guest GPU, networking/audio, renderer selection or player
settings acceptance is implied.

### Native CR8 and software TPR priority coherence

Code **b470be7** fixes the missing connection between WHP's native CR8 register
and the software xAPIC task-priority register. Each stopped VP samples its own
CR8 before interrupt arbitration; CR8 changes replace TPR[7:4] while preserving
TPR[3:0]. MMIO TPR reads first synchronize that alias, and MMIO TPR writes update
native CR8 before the VP resumes. Thus native `MOV CR8` can no longer leave the
host arbitration path using a stale task-priority class. The Windows SDK ABI
check now explicitly requires `WHvX64RegisterCr8 == 0x20`.

The added real **long64** guest fixture writes TPR=7, writes CR8=4, enables IF,
and queues a self-IPI vector 0x40. It captures TPR=0x47, the vector's IRR bit set,
and a zero handler counter before lowering CR8=3. It then waits for exactly one
handler/EOI/IRETQ round trip, writes MMIO TPR=0x5b and captures MOV-from-CR8=5.
Final host evidence must be exactly `[1,0x47,1,0,5]`; exit reason and bounded
execution must also pass. This is actual guest instruction execution, not
simulated register assignments. The fixture runs three times in the Windows
foundation CLI gate, which requires the exact `NATIVE_CR8_TPR_OK` evidence.
A model-level test also checks all 16 priority classes and all 16 subpriorities
against every external vector, deferred IRR retention/unmasking, and rejection
of out-of-range CR8 values without discarding pending work.

Verification:

- **45859463908**: strict Windows/Ubuntu foundation at b470be7 passed, including
  three real guest priority fixtures and the existing 48 dual-VP cycles.
- **45859713362**: the entire 47-VM APIC Linux matrix passed with native CR8
  synchronization enabled (core/RAM, storage/COM2, fault/recovery, concurrent IO).
- **45860572300**: legacy PIC Linux regression passed, reusing the existing kernel.
- **45860603320**, code **659f633**: foundation passed again with the explicit
  CR8 marker requirement and SDK assertion. Windows check **140050307728**
  records the three actual long64 alias/deferred-delivery cycles as passed.

This closes the tested native CR8/TPR alias gap only. It does **not** certify
runtime INIT reset/hotplug, all APIC priority/delivery modes, arbitrary interrupt
contention, long-duration stress or physical Windows 10. Step 1 remains open;
Android Home/GPU/network/audio/player settings and launch-time targets are not
advanced by this gate.


### Target-owned runtime AP INIT/SIPI restart

Code **15148d3** replaces the earlier explicit runtime-INIT rejection with a
shared AP worker used by both the native fixture and Linux SMP frontend. Guest
INIT is queued to the target worker. The 1 ms cancellation pump brings that VP
out of WHP execution; its run loop checks the control channel before reentry.
Only the target worker, with no WHvRun active on that VP, deletes and recreates
its WHP processor under the device/fabric lock. This discards stale native
register/pending-event/translation state rather than attempting a partial list
of register repairs. It replaces the target's software LAPIC and establishes
real-mode reset entry state (including CR8=0), then waits for a new SIPI to set
CS:IP. The partition, RAM mapping, other VPs and shared devices are not recreated.
This is a host implementation of a guest-requested AP reset; no QEMU is involved.

Routing validates all destinations before sending commands, commits INIT/SIPI
startup state under the fabric lock, ignores duplicate SIPIs after startup,
and ignores level INIT deassert as a second reset. Physical AP destinations
and all-except-self startup routing remain available. BSP/self-all reset and
whole-machine reboot are not supplied by this change. Normal shutdown, error
and panic paths retain scoped worker joins and the existing RAM/device gates.

The two-VP guest fixture now performs three **active** runtime AP resets after
its original startup/interrupt/device checks. Before each INIT, AP1 dirties its
TPR/CR8 and queues a masked self-IPI, publishes its generation, then spins with
IF=0 rather than halting. BSP issues INIT assert/deassert and duplicate SIPIs.
Each restarted AP checks reset TPR/SVR defaults and empty IRR/ISR at the probed
vector class before enabling its LAPIC. Shared generation counters must show
exactly four starts and four completions, with no stale bits; the final
generation halts cleanly so the original no-pending-IRQ and restored-stack
assertions remain enforced. Three CLI repetitions of 16 cycles require **144
active AP resets**, using the same production worker as the Linux frontend.
The test initially failed because its final generation deliberately left a
pending test IPI, violating the existing drain assertion; the final generation
now leaves a clean halt, without weakening that drain assertion.

Verified runs:

- **45862730479**: strict Windows/Ubuntu foundation at 15148d3 passed. Windows
  check **140057651449** records all 144 active AP resets and the three long64
  CR8/TPR cycles; the CLI requires the exact reset marker in all 48 cycles.
- **45862939896**: all 47 APIC Linux VM invocations passed using the new worker,
  including selected CPU/RAM, raw/ext4/COM2, DMA errors/panics and recovery, and
  concurrent direct-I/O/persistence. Kernel artifact 45834154987 was reused.
- **45863607435**: legacy PIC Linux regression passed with artifact 45814022595.
- Seven local Python initramfs structural tests passed again.

**Limits:** runtime reset is directly exercised on AP1 executing protected32
code, not on a running Linux scheduler, long64 AP workload, all eight APs under
reset storms, or during an in-flight disk transaction. Initial Linux SMP boots
through the shared worker pass, but they are not OS CPU-offline/online tests.
No claim of complete INIT architectural semantics, hotplug, BSP reset, ACPI
power management, physical Windows 10 acceptance or exhaustive interrupt stress
is made. Step 1 remains incomplete to those bounds; Android Home, accelerated
guest graphics, networking/audio, ANR fixes and player-settings integration are
still separate unfinished stages.
