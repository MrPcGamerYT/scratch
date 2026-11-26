# Modern VirtIO-MMIO block register/service model

## Verified scope

`virtio_mmio.rs` implements a transport-independent modern VirtIO-MMIO v2
register and queue-service model over the validated split queue/COW block core.
The model tests call the register API directly. A separate live WHP fixture now
maps the device page through the Windows SDK instruction emulator; the normal
Linux boot path does not yet expose it.

Tested commit `028056d`:

- Foundation **45758465816** passed Windows/Ubuntu tests, strict Clippy, release
  builds and existing repeated live WHP CPU/timer/ATA/COW contracts.
- Linux regression **45758465572** passed three diagnostic PID1 boots. These use
  the existing initramfs and are not `/dev/vda` discovery/mount tests.

Ten new unit tests cover the model and file-backed COW integration. Earlier queue,
memory and storage tests remain green. No Android image or user disk was modified.

## Implemented registers and behavior

- Magic `0x74726976`, version 2, device ID 2 (block), local NEXS vendor label.
- Feature selectors and negotiated lower/upper feature words. Advertises
  VERSION_1, FLUSH, SIZE_MAX (128 KiB) and SEG_MAX (one data segment), plus RO for a read-only device. No packed rings, indirect
  descriptors, EVENT_IDX, multiple queues or asynchronous workers advertised.
- ACKNOWLEDGE/DRIVER/FEATURES_OK/DRIVER_OK status handling. Unsupported/missing
  version features clear FEATURES_OK. Features lock after acceptance; invalid
  status removal is rejected. FAILED/DEVICE_NEEDS_RESET require full reset.
- One direct split queue, maximum 256 entries. Size/alignment validation,
  descriptor/available/used 64-bit address halves, queue select and ready state.
  Full RAM validation happens before any service/backend IO.
- Ready queues cannot have addresses/size changed. QueueReady=0 synchronously
  disables service and discards the active parser/pending notification, not saved
  disk data. Re-enabling starts a fresh parser requiring used.idx=0; persistent
  live-index reuse is not supported. Full reset is the tested reinitialization path.
- QueueNotify coalesces pending work, but performs no disk IO in the register
  write. The separate synchronous `service()` handles at most 1..256 requests
  per call and retains pending work when the budget is exhausted.
- Used-buffer interrupt status latches until acknowledged. NO_INTERRUPT
  completions do not create a new interrupt. `irq_asserted()` remains true while
  any interrupt-status bit is set. This is a **line-state output**, not IRQ routing.
- Fatal queue/configuration faults set DEVICE_NEEDS_RESET and a configuration
  interrupt; no more requests run until reset. Ordinary protocol/backend errors
  complete with IOERR without falsely declaring a successful transfer.
- Capacity is static and readable as little-endian 1/2/4-byte config accesses.
  Other registers require aligned 32-bit accesses. Unknown accesses fail explicitly.
- Reset clears feature/queue/interrupt/pending state while retaining fixed
  capacity/read-only policy. It does not roll back the external COW backend.

## Backend policy and tests

Read-only policy blocks writes even if a writable backend is accidentally supplied.
FLUSH is passed to the backend only when negotiated; otherwise a valid request
completes UNSUPP without flushing. Advertised capacity is checked against the
backend before dispatch. The provided read-only backend fully verifies ZLAG; the
COW backend checks its own base binding and durable-sector record contract.

Tests exercise rejected feature negotiation, selector overflow, register access
widths, capacity byte reads, unknown queues, live-queue protection, queue disable,
full reset, notify-before-ready refusal, bounded multi-request service, latched
interrupt/ack/reassertion, NO_INTERRUPT, fatal descriptor cycles and invalid GPAs,
RO write prevention, negotiated/unnegotiated FLUSH and ordinary IO failure.

A real file-backed test drives registers and queue service to write/flush through
COW, resets/tears down the model, reopens the overlay and reads the saved bytes
through a newly negotiated model. The base bytes remain unchanged. This is a
host model/integration test, **not guest MMIO execution or a filesystem test**.

## Live WHP register bridge

Commit `c06ccaa` passed foundation **45759109225** on Windows and Ubuntu, including
strict Clippy, SDK ABI assertions, release builds and three repetitions of the
new live `mmio-smoke` contract. Actual protected32 and long64 guests read/write
registers at GPA `0xd0000000`, negotiate features, ignore read-only writes and
reset the device. Long-mode execution forces the SDK emulator to fetch instructions
through a nonidentity virtual mapping, exercising translation and RAM callbacks.
The adapter owns its SDK callback table for the emulator lifetime and contains
callback errors/panics. RAM callbacks support up to eight bytes; register callbacks
are limited explicitly to one, two or four bytes.

This does **not** exercise queue DMA, disk requests, VirtIO interrupt delivery or
Linux device discovery. The separate model tests still provide queue/COW evidence.

Hardening commit `64296d8` additionally rejects oversized register accesses before
scalar conversion. Foundation **45759258169** and Linux regression **45759258205**
both passed at that exact commit; the latter reached diagnostic PID1 in three
boots, not Android Home or VirtIO disk discovery.

## Remaining integration gates

1. Connect the verified WHP instruction-emulator adapter to the real Linux run
   loop and queue service; the current live fixture only tests registers.
2. A safe adapter from host-mapped guest RAM to the checked memory API. All vCPUs
   and workers must be stopped for current service; concurrent DMA/Acquire-Release
   atomics and SMP support are not implemented by adding ordinary Rust slices.
3. Real level-triggered interrupt-controller routing beyond the existing IRQ0
   path, including assertion/deassertion and completion/configuration acknowledgment.
4. Guest-visible queue/transfer-limit negotiation, broader valid descriptor
   layouts and Linux driver compatibility testing. Current requests are limited
   to 128 KiB and separate 16-byte header/one-byte status descriptors; this is not
   a fully certified production VirtIO device.
5. Actual Linux `/dev/vda` discovery, block IO, filesystem write/sync/reboot and
   crash testing, followed by Android system/vendor/data and SELinux integration.
6. Async/batched host IO, quotas, overlay compaction, player/GPU integration and
   affected-guest freeze diagnosis.

The roughly **100-second launch-to-usable-Android-Home goal remains unverified**.
The normal Linux and player paths still do not expose this VirtIO device.
