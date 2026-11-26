# New engine and guest stability — progress report

## Bottom line

**The independent engine now boots a real Linux kernel into diagnostic userspace.**
It is **not yet a usable Android emulator**. No new Android guest image or
ready-to-use Windows emulator release is being published with this milestone.

## Guest-notified VirtIO disk queue checkpoint

Code `44fe10c` passed foundation **45760536459** and three-boot Linux diagnostic
regression **45760536464**. Actual protected32/long64 guest CPUs now start and
notify a fixture-seeded split queue. Stopped-vCPU service executes write, flush
and read against a real COW backend. Tests check exact data, three status bytes,
used index, guest-observed interrupt latch/ACK, reopened persistence and unchanged
base image. CI repeats the fixture three times. This adds live queue/DMA coverage;
it is not Linux device discovery, external VirtIO interrupt delivery, filesystem
persistence across guest reboot, or Android readiness. See `native-virtio-mmio.md`.

## WHP MMIO register bridge checkpoint

Code `64296d8` passed Windows/Ubuntu foundation **45759258169** and the three-boot
Linux diagnostic regression **45759258205**. The new live CPU fixture verifies
modern VirtIO-MMIO register reads/writes in protected32 and long64, negotiation,
reset, read-only write handling and forced SDK instruction fetching through a
nonidentity mapping. Transfer limits are advertised as SIZE_MAX=128 KiB and
SEG_MAX=1. See `native-virtio-mmio.md` for evidence and integration boundaries.

The normal Linux loop still has no VirtIO disk: queue DMA/service, dedicated
interrupt routing and `/dev/vda` persistence remain integration gates. This is
not Android readiness, an ANR fix or achievement of the 100-second goal.

## Latest update: storage bring-up and revised target

The user has revised the goal to **approximately 100 seconds from launch to
usable Android Home**. New runtime/test reports use `android_home_target_ms:
100000`; archived 60-second reports are preserved as historical evidence.
Android readiness and target-met fields remain null.

Added a verified ZLAG-backed primary-master ATA LBA28 PIO read-only device and a
live guest test. Guest code issued IDENTIFY and a sector read, placed exact disk
bytes in guest RAM, and received an error for a prohibited write. Three Windows
WHP repetitions and source-integrity checks passed in
[foundation 45753956772](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45753956772).
Repeated Linux diagnostic PID1 boots also passed with the new target in
[checkpoint 45753956761](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45753956761).

The original read-only bring-up is now extended with an optional persistent
sector overlay (see below), but is **not** Android filesystem mounting or player
integration. No IRQ14/DMA is implemented. The normal Linux boot still uses the initramfs and has no ATA disk
attached. See [native-ata-storage.md](native-ata-storage.md) for the exact contract
and limitations. New-target evidence is in
`deliverables/native-kernel/100s-target-checkpoint.json`.

### New: persistent writable sector overlay

At `a048d9e`, the optional ATA COW mode passed a real Windows guest test:
write a sector -> flush -> exact guest readback -> tear down vCPU/device ->
separately reopen the overlay and verify the saved bytes. This passed three
times with the original ZLAG SHA256 unchanged. Base images remain read-only.

The append-only overlay validates its base binding and record integrity, prevents
a second writer, and defaults to rejecting incomplete/corrupt tails. Explicit
recovery truncates only a short last record after all complete records validate.
Injected host write errors poison the instance; prior committed data is verified
after reopening. Partial sectors are discarded on reset. Writes are per-sector
commits, not multi-sector transactions or physical power-loss certification.

- [Final foundation 45755799925](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45755799925): Windows/Ubuntu unit tests, strict Clippy, release and repeated live WHP/COW contracts passed.
- [Linux PID1 regression 45755800012](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45755800012): three diagnostic userspace boots passed; not a Linux disk mount or Android boot.
- [Storage format, safety and remaining limits](native-writable-storage.md).

The normal Linux/player paths still do not attach this disk. Android block-driver
integration, IRQ/batched IO, quotas/compaction and filesystem-level validation
remain necessary. The per-word PIO/per-sector-sync implementation is a correctness
prototype, not a measured fast storage path or an app-freeze fix.

### New: validated VirtIO block queue/request core

At `5ad788c`, a new checked scatter/gather RAM API, bounded split queue parser and
VirtIO IN/OUT/FLUSH executor passed **16 new tests**. Requests reach the existing
COW backend; a file-backed write/flush/close/reopen/read test preserves base bytes.
Malformed GPA ranges, loops, aliasing, bad permissions and oversized requests
are rejected before backend access. Invalid status tails are never guessed.

Foundation **45757147913** passed Windows/Ubuntu tests and existing WHP contracts;
Linux regression **45757147928** passed the existing diagnostic PID1 boots.
These are host-side VirtIO core tests, **not** Linux `/dev/vda` discovery.
The memory API requires stopped vCPUs/workers; concurrent DMA memory ordering
is not implemented. The MMIO register/negotiation/service model is now implemented (below); WHP
memory trapping and actual level IRQ routing remain the next dependencies. No VirtIO disk is advertised to Linux yet.

See [native-virtio-block.md](native-virtio-block.md) for the exact implemented
subset and remaining acceptance gate. Android Home, app-freeze diagnosis,
slimming, GPU/player integration and the 100-second target remain unfinished.

### New: modern VirtIO-MMIO register and queue-service model

At `028056d`, the modern MMIO v2 register model now connects feature negotiation,
queue setup/disable/reset, notifications and bounded dispatch to the validated
block core. Used-buffer interrupt status latches until acknowledged; fatal queue
errors request a reset. Read-only and negotiated-FLUSH policies are enforced.

Ten new tests passed, including register access rules, negotiation failure,
service budgets, interrupt status/ack/reassertion, fatal queue errors and actual
COW model-level write/flush/reset/reopen/read with the base unchanged.
Foundation **45758465816** and Linux diagnostic regression **45758465572** passed.

This is still a host-side register/service model: **no WHP guest has accessed
these MMIO registers, no interrupt line is routed, and Linux does not see
`/dev/vda` yet**. Next: WHP memory-access/instruction-emulation adapter and real
level IRQ delivery, then Linux driver/block/filesystem validation.
See [native-virtio-mmio.md](native-virtio-mmio.md) for precise scope and limits.

## Implemented and verified in this work

### 1. Linux timer, interrupts and userspace

- Added PIT channel 0 binary mode 0 (one-shot) and mode 2 (rate generator),
  monotonic elapsed-time counting, coherent latched reads and pending-edge
  coalescing. Time does not advance by counting host I/O requests.
- Connected PIT0 to PIC IRQ0 request/mask/in-service/EOI state and external WHP
  interrupt injection. Injection waits for IF and interrupt-shadow permission;
  it does not overwrite a queued event.
- Added a bounded event-pump rendezvous, real guest HLT wait/wake and joined
  thread teardown. Scheduling cancellation is not treated as a guest timer tick.
- Added a live WHP test proving one-shot timer -> PIC -> external interrupt ->
  HLT wake -> guest handler -> EOI/IRETD, exact RAM count and restored stack.
- Declared unpopulated ISA DMA, secondary serial ports and PS/2 controller as
  absent devices. This is not an implementation of DMA, extra UARTs or input.
- Added a deterministic diagnostic initramfs. Linux executes its `/init` as
  PID1, checks `getpid()`, successfully returns from two 100 ms `nanosleep()`
  calls and writes a distinct readiness marker through `/dev/kmsg`.
- Three actual Linux PID1 checkpoints passed. Host stops the VM deliberately at
  the diagnostic marker; this is not an Android-ready signal or a Linux soak test.

**Evidence:**

- [Actual Linux/PID1 checkpoint 45751571983](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45751571983), tested engine `ca99d3b`.
- [Windows + Ubuntu foundation 45751571735](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45751571735): unit tests, strict Clippy, release builds, SDK ABI and repeated live WHP contracts passed.
- Exact serial bytes, timing and provenance are under `deliverables/native-kernel/userspace-*`.
  The 6,255-byte log was independently reconstructed and checked against the
  original full-log SHA256, not normalized text.

### 2. Previous guest's freeze/readiness problem

Historical guest evidence (run 45663481567) shows provisioning reached a Home
launch and later failed; it does **not** contain enough evidence to establish
the user's app-ANR root cause. A long `am start -W` wait was a weakness in the
checker, not proof that the launcher or extra packages caused all freezes.

Changes:

- Replaced that native Home check with a bounded launch and bounded activity/ANR
  queries. A wedged framework must fail the readiness gate instead of waiting
  indefinitely inside that command.
- Added input/compositor evidence to the read-only diagnostic snapshot.
- Made app-crash events fail the stability assessment, not merely appear in logs.
- Added a 30-second post-Home ANR/crash observation window to the ADB-based CI
  probe; delayed errors and missing evidence fail. Historical logs are not cleared.
- Added tests for delayed ANRs, crash detection, timeouts and non-mutation.

**Verified:** 13 diagnostic/parser/cleanup tests, 3 guest bridge tests and 1 native
agent integration test passed locally and in
[diagnostic safeguards run 45751288619](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45751288619).
These are tool/agent tests, **not** an affected-guest Android runtime validation.
The native production bridge remains independent of ADB; its image recipe needs
rebuilding before these shell-check changes reach a guest image.

### 3. Safe image slimming

Added `guest-tools/plan_cleanup.py`, an inventory-only review planner. It requires
successful installed-package evidence and explicit installed Home/file-editor
package IDs. It preserves those packages and every unreviewed package. Only
three narrow optional entertainment/screensaver IDs can be listed for review,
and only if actually present in the inventory. It generates no deletion or
disable commands and makes no dependency-safety or space-savings claim.

**No guest packages were removed.** Dependency review, a disposable-overlay
comparison, a rebuilt image and runtime regression tests are still required.
The file editor is preserved.

## Boot-time status

The recorded first sample reached **diagnostic Linux PID1 readiness at 704 ms**
from engine main, including input loading. It recorded 59 IRQ0 injections and
50 HLT waits. This tiny fixture runs on a hosted Windows Server 2022 machine;
it is not Android, a cold-cache benchmark, the Qt player's startup time or a
physical Windows 10 result. Host CPU/MSR and TSC calibration warnings remain.

The approximately **100-second launch-to-usable-Android-Home target is unverified**.
Android Home readiness and target-met fields deliberately remain null. This work
does not establish real-time timer accuracy, GPU speed or Android boot speed.

## What is still left

| Area | Remaining work |
|---|---|
| Linux platform | Broader CPU/MSR capability contract and clock calibration; APIC/SMP and additional interrupts/devices; sustained userspace tests. Current support is one vCPU and a tiny initramfs, not a general Linux distribution. |
| Android boot | Android-compatible kernel/device contract, system/vendor/data and init integration, usable storage device path, Android framework/services and repeatable Home readiness. |
| `gustimg.zlag` | Read-only ATA and optional persistent COW writes pass live guest tests. Normal Linux/player attachment, IRQ/batched IO, guest block drivers, scalable overlay management and the Android disk lifecycle remain unfinished. |
| App freezes | Capture and reproduce on the affected guest/player, correlate ANR traces with CPU/memory/IO/compositor behavior, apply a demonstrated root-cause fix and test sustained app interactions. No confirmed ANR fix yet. |
| Slim guest | Inventory the exact candidate, review dependencies, test changes on a new overlay/image, retain file editor/launcher/core Android services and validate two boots plus applications. |
| Graphics/GPU | Real virtual graphics device, matching Android drivers, host GPU-backed renderer and measured DirectX/OpenGL/Vulkan integration. No accelerated graphics was added in this work. |
| Player/devices | Wire the new engine into the existing player; display, input, native controls, cursor alignment, audio/network and portable Windows packaging. Existing editor/player files are preserved, not migrated. |
| Performance | Measure complete player launch through usable Android Home and post-boot responsiveness on representative low/high-end Windows 10 PCs. Optimize from those measurements, not the diagnostic PID1 number. |

**Next dependency:** connect the tested writable backend to an interrupt-capable/batched guest block device and Android drivers, plus the device/CPU platform, followed by
Android init and Home. GPU/device and affected-guest stability work remain major
engineering tasks; this report does not imply they can be completed by simply
removing more applications.
