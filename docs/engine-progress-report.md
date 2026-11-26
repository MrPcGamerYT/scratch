# New engine and guest stability — progress report

## Bottom line

**The independent engine now boots a real Linux kernel into diagnostic userspace.**
It is **not yet a usable Android emulator**. No new Android guest image or
ready-to-use Windows emulator release is being published with this milestone.

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

The approximately **60-second launch-to-usable-Android-Home target is unverified**.
Android Home readiness and target-met fields deliberately remain null. This work
does not establish real-time timer accuracy, GPU speed or Android boot speed.

## What is still left

| Area | Remaining work |
|---|---|
| Linux platform | Broader CPU/MSR capability contract and clock calibration; APIC/SMP and additional interrupts/devices; sustained userspace tests. Current support is one vCPU and a tiny initramfs, not a general Linux distribution. |
| Android boot | Android-compatible kernel/device contract, system/vendor/data and init integration, usable storage device path, Android framework/services and repeatable Home readiness. |
| `gustimg.zlag` | Runtime guest storage integration, safe writes/overlays and full Android disk lifecycle. Format validation/first-sector execution is not a working Android disk device. |
| App freezes | Capture and reproduce on the affected guest/player, correlate ANR traces with CPU/memory/IO/compositor behavior, apply a demonstrated root-cause fix and test sustained app interactions. No confirmed ANR fix yet. |
| Slim guest | Inventory the exact candidate, review dependencies, test changes on a new overlay/image, retain file editor/launcher/core Android services and validate two boots plus applications. |
| Graphics/GPU | Real virtual graphics device, matching Android drivers, host GPU-backed renderer and measured DirectX/OpenGL/Vulkan integration. No accelerated graphics was added in this work. |
| Player/devices | Wire the new engine into the existing player; display, input, native controls, cursor alignment, audio/network and portable Windows packaging. Existing editor/player files are preserved, not migrated. |
| Performance | Measure complete player launch through usable Android Home and post-boot responsiveness on representative low/high-end Windows 10 PCs. Optimize from those measurements, not the diagnostic PID1 number. |

**Next dependency:** an Android-capable storage/device/CPU platform, followed by
Android init and Home. GPU/device and affected-guest stability work remain major
engineering tasks; this report does not imply they can be completed by simply
removing more applications.
