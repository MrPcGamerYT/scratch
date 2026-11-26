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

## Latest verified scope: selected-core Linux SMP/APIC

Code **76c08de** passed a **10-boot actual Linux matrix** in **45842479962**:
selected CPU counts 1..8, RAM 128..1024 MiB, and repeat two-CPU boots at 256/512
MiB. Linux reports the selected online CPU count and E820 memory range; PID1
pins itself onto **every selected CPU** and verifies getcpu(). Foundation
**45842185302** passed both platforms, and legacy PIC Linux regression
**45842774985** passed. `boot-kernel ... --apic-probe --cpus=N --ram-mib=M`
now applies N to WHP processor creation, CPUID, MP tables and guest INIT/SIPI
startup, and M to actual guest RAM/E820. The default disk/Android/PIC profile
still rejects multiple CPUs. This is headless initramfs SMP acceptance, **not
SMP Android/disk/player acceptance, comprehensive concurrency stress, physical
Windows 10 certification or completed Step 1**.

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
