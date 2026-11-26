# VirtIO block milestone 1: bounded queue/request core

## Implemented and verified

Tested revision `5ad788cb83b634552d06d097fbfdd628fde6b93b`:

- [Foundation 45757147913](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45757147913) passed Windows/Ubuntu unit tests, strict Clippy, release builds and existing live WHP contracts.
- [Linux regression 45757147928](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45757147928) passed three diagnostic Linux PID1 boots. These still use an initramfs, **not a VirtIO disk**.

Sixteen new pure-Rust tests exercise the components below. Existing ATA/COW live
WHP tests remain green. The new VirtIO queue tests run against checked test RAM;
no WHP MMIO transport or guest Linux VirtIO driver was involved.

### Checked guest memory

`guest_memory.rs` accepts 1..128 explicit non-overlapping RAM regions. Addresses
and exclusive range ends are checked for overflow and holes. Transfers may cross
adjacent GPAs backed by separate host allocations. A write validates the whole
scatter/gather range and permissions before copying any bytes. MMIO/ROM holes
must not be included as writable RAM regions. No guest GPA is cast to an HVA.

**Concurrency contract:** all vCPUs and device workers touching these regions
must be stopped for the duration of the exclusive memory borrow. This API is not
safe for concurrent guest DMA. It does not claim that Rust slices or arbitrary
Acquire/Release fences solve concurrent shared-memory access. A future live
transport must enforce quiescence or supply a separately reviewed synchronized
memory API before running workers/vCPUs concurrently.

### Split queue parser

`virtqueue.rs` supports one direct-descriptor split queue:

- Power-of-two queue size 1..256; aligned descriptor/available/used tables.
- Full RAM-range validation; metadata ranges must not overlap.
- Available-index advance bounded by queue capacity; device-owned used-index
  tampering rejected; 16-bit index wrap tested.
- Bounded descriptor traversal with visited-index checks; cycles and invalid
  indices rejected. Indirect descriptors, unknown flags and empty buffers are
  unsupported and rejected, not guessed.
- Every descriptor range/permission checked before disk IO or memory writes.
- Descriptor buffers may not alias one another or any queue metadata. This is a
  conservative subset; broader permitted layouts need explicit additional tests.
- Available-ring NO_INTERRUPT produces a completion hint; it is not IRQ routing.

Malformed descriptor topology, bad DMA or a missing trustworthy status tail
returns a **fatal queue error with no completion/disk side effect**. It is unsafe
to guess where to write IOERR through an invalid chain. A future transport must
stop/reset such a queue and expose device-needs-reset rather than retry forever.

### VirtIO block requests

`virtio_block.rs` executes bounded requests through a backend trait:

- Type 0 (IN): device-writable data descriptors; read into a bounded host scratch
  buffer first, then copy into guest RAM only after backend success.
- Type 1 (OUT): device-readable data descriptors; snapshot before backend writes.
- Type 4 (FLUSH): header + writable status only, no data descriptors. The sector
  field is unused for FLUSH; no artificial sector-zero dependency is imposed.
- Unsupported types return status 2 (UNSUPP). Validly framed protocol/backend
  errors return status 1 (IOERR) through a validated status tail.
- Header is exactly 16 device-readable bytes; status is exactly one writable
  byte. Merged header/data and extended status layouts are not implemented yet.
- Scatter/gather data lengths aggregate to a nonzero multiple of 512, at most
  128 KiB. Request sector arithmetic is overflow/bounds checked before IO.
- Completion counts device-written bytes (read payload + status, or status only)
  and writes the used-ring entry/index after request data/status are ready.
- The persistent COW backend supports reads/writes/flush. A read-only adapter
  fully verifies ZLAG payload integrity before exposing the base; writes fail.
  Multi-sector COW writes remain per-sector commits, not atomic transactions.

Tests include real file-backed COW write -> FLUSH request -> close -> reopen ->
read request, with the original base bytes unchanged. They also cover injected
backend read failure, read-only backing, corrupt-base refusal, over-limit
requests, readonly RAM, invalid status/header/direction, descriptor cycles,
metadata/data aliasing, index overflow, holes and notification suppression.

## Not implemented by this milestone

1. Modern VirtIO-MMIO v2 registers, feature/status negotiation and queue lifecycle.
2. WHP memory-access exits and instruction-emulator callback integration.
3. Level-triggered interrupt routing beyond the existing tested IRQ0 path.
4. A Linux driver discovering `/dev/vda`, filesystem mounting or reboot/write tests.
5. Asynchronous/batched disk scheduling, production queue reset/drain behavior,
   quotas, overlay compaction or SMP/shared-memory synchronization.
6. Android fstab/SELinux/system/vendor/data integration, GPU or player wiring.

No VirtIO device is advertised to Linux yet. No generic Android fstab template is
installed: actual guest filesystem/layout, AVB/verity and first-stage mount
requirements must be inspected before changing them. Existing file editor and
image metadata are untouched.

## Next acceptance gate

Implement feature-negotiated modern MMIO registers and a safe WHP MMIO adapter,
then the level IRQ path. Configure a pinned Linux kernel with the corresponding
VirtIO drivers and test actual discovery, sector IO, flush and reopen through
the Linux block driver. Keep these results separate from Android Home readiness.

The approximately **100-second launch-to-usable-Android-Home target remains
unverified**. This milestone establishes request validation, not boot speed or
an app-freeze fix.
