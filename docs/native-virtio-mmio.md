# Modern VirtIO-MMIO block register/service model

## Verified scope

`virtio_mmio.rs` implements a transport-independent modern VirtIO-MMIO v2
register and queue-service model over the validated split queue/COW block core.
The model tests call the register API directly. A separate live WHP fixture now
maps the device page through the Windows SDK instruction emulator. The opt-in
Linux disk boot commands now expose it to a real Linux virtio-blk driver. The
legacy diskless command and Android player remain unchanged.

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

### Guest-notified queue service and COW checkpoint

Code `44fe10c` passed foundation **45760536459** (Windows/Ubuntu, strict Clippy,
release, SDK ABI and repeated live fixtures) and Linux diagnostic regression
**45760536464** (three boots). The `mmio-smoke` fixture now additionally performs:

- Guest CPU negotiates features, configures/starts queue and writes QueueNotify.
- After synchronous SDK emulation returns, with the only vCPU stopped, a bounded
  GuestMemory borrow feeds queue service. No DMA occurs inside an SDK callback.
- Three fixture-seeded request chains write sector 1, FLUSH and read sector 1 into
  another buffer. All three status bytes and used.idx=3 are checked, together with
  exact patterned data, and exactly three successful completions are required.
- Guest reads latched InterruptStatus=1, acknowledges it, then reads zero.
- Device teardown and separate COW reopen recover the written bytes; the verified
  base image remains byte-for-byte unchanged. Temporary fixture files are removed
  on success, never using a user disk.

Both protected32 and long64 execute this path, three repetitions each in the CLI
suite. Queue rings are supplied by the host fixture, not constructed by Linux.
At this historical commit only interrupt status/ACK was tested. Later checkpoints
below add external interrupt delivery and Linux block IO; the fixture itself is
not a Linux driver test.

Hardening commit `64296d8` additionally rejects oversized register accesses before
scalar conversion. Foundation **45759258169** and Linux regression **45759258205**
both passed at that exact commit; the latter reached diagnostic PID1 in three
boots, not Android Home or VirtIO disk discovery.

## Dedicated IRQ5 and real Linux block IO

Final tested code **`e1ff3875ab788a826dd3549c82de9e6ecfe60f42`**:

- Foundation **45762223941** passed Windows/Ubuntu, strict Clippy, release builds,
  SDK ABI checks and all repeated live WHP fixtures.
- Linux **45762223769** passed three actual kernel/diagnostic PID1 boots, reusing
  the VirtIO-enabled pinned Linux 6.6 kernel from **45761627640**.
- Earlier engine integration `87ae768` passed foundation **45761627651** and
  Linux discovery **45761627640**. Userspace persistence tests `3292dac` passed
  Linux **45762013313** before the final evidence/CLI polish.

PIC routing supports IRQ0 edges and a dedicated IRQ5 input. IRQ5 defaults to edge
mode, with explicit ELCR bit 5 selecting level mode; other ELCR sources fail
closed. Tests cover masks, pending requests, correct/wrong EOIs, held-level
reassertion, deassertion, rising-edge latching and fixed-priority IRQ0 preemption.
The WHP injector retains IF/shadow/pending-event guards and acknowledges the
selected PIC request only after WHP accepts it. IRQ0 and IRQ5 timing counters
are separate. No slave sources, IOAPIC, MSI, SMP or full chipset are claimed.

The protected32 live fixture now takes one external IRQ5. Its handler captures
InterruptStatus, acknowledges the device, EOIs the PIC and IRETD returns with the
original stack; count/status and absence of a residual request are checked.
Long64 still uses the polled register-ACK fixture; real Linux exercises IRQ5
while executing its own kernel and driver code.

### Linux acceptance sequence

The CI kernel enables BLOCK, MSDOS_PARTITION, VIRTIO_MMIO,
VIRTIO_MMIO_CMDLINE_DEVICES and VIRTIO_BLK. The boot command line adds
`virtio_mmio.device=4K@0xd0000000:5`. Host queue service borrows RAM only after
synchronous emulation returns, with the sole vCPU stopped. Linux constructs its
own rings and requests; these are not fixture-seeded descriptors.

1. A new 2 MiB zero-filled test disk is imported to ZLAG and its digest recorded.
2. Boot 1 creates a new COW overlay. Linux reports `[vda] 4096 512-byte logical
   blocks`. Diagnostic PID1 mounts devtmpfs (no guessed device major), opens
   `/dev/vda`, writes a patterned 512-byte block at byte offset 4096, calls
   `fsync`, reads and compares it, then emits `NEXUS_VDA_WRITE_FLUSH_READ_MATCH`.
3. Boots 2 and 3 use separate engine processes/VMs and explicitly reopen that
   overlay. Their PID1 only reads/compares the saved pattern and emits
   `NEXUS_VDA_REBOOT_READ_MATCH`; they never rewrite the expected data.
4. Every sample requires real IRQ5 injections, exact marker checks, successful
   diagnostic completion, unchanged ZLAG digest and Linux PID1/timer evidence.

Serial logs and timing/evidence JSON are uploaded by CI. The report explicitly
records whether the supplied kernel has VirtIO drivers: reusing an old pre-VirtIO
kernel is only a diskless regression, not storage acceptance.

### Opt-in CLI

```
nexus-engine boot-kernel-disk-create <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <NEW-overlay.cow>
nexus-engine boot-kernel-disk-open <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <existing-overlay.cow>
```

Creation and reopen are distinct; never overwrite a user's overlay. Supply the
VirtIO discovery parameter above and a compatible guest kernel. The diagnostic
boot loop remains bounded and stops at its PID1 readiness marker, not Android.

## Ext4 filesystem and journal replay acceptance

Final test/CI code `b1e9d27c7f249ea86ceda76fc751a20435ab7f30` passed Linux
**45773550049**. Engine changes `dbef542` separately passed Windows/Ubuntu
foundation **45773319762** and Linux **45773331591**. Kernel/image build
**45772174051** enables EXT4_FS and creates a disposable 32 MiB ext4 image with
standard e2fsprogs, checked by e2fsck and SHA256SUMS. Source test ext4 SHA256:
`62f9301d9b3e7fbb8a043eb0c7aa814af9aa33c4e719d77de3ebb8aa53be58f7`.

The latest acceptance run executes eight separate engine/VM boots:

- Three raw-block persistence boots retained from the previous checkpoint, now
  exercising 256, 512 and 1024 MiB guest RAM respectively.
- Three clean filesystem boots: first mount ext4 read/write, create/truncate
  `/mnt/probe.bin`, write and compare the patterned bytes, fsync the file and
  parent directory, close and cleanly unmount. Two subsequent separate process/VM
  boots reopen COW, mount read-only, open the existing file without create/truncate
  or any write syscall, read/compare its bytes, close and unmount.
- Two journal-recovery boots on a separate fresh image/overlay: first fsync file
  and parent directory but **do not unmount** before the diagnostic marker causes
  VM teardown. Next boot reopens that overlay, mounts read-only (Linux replays the
  journal), requires `recovery complete` in the actual ext4 log, and reads/compares
  the file before unmounting. This is abrupt **guest/VM termination while mounted**,
  not host process kill, host power loss, or arbitrary mid-write crash injection.

Every filesystem sample requires its operation-specific guest marker, successful
engine exit, real IRQ5 delivery, diagnostic PID1 readiness, no ext4/IO error, and
unchanged source ext4/ZLAG digests. Failure reports remain `verified:false`.
A separate CI step requires all three clean and both recovery sample checks and
emits `EXT4_ACCEPTANCE` with exact commit/hash and `android_ready:false`; it cannot
silently count the raw-block-only regression as filesystem acceptance.

Three local and CI structural/oracle tests verify ELF/newc layout, no write syscall
in the read-only reboot fixtures, real buffer comparison, and no unmount syscall
in the unclean-stop fixture. They are explicitly not substitutes for WHP execution.

### Larger guest RAM

Kernel boot commands accept a trailing `--ram-mib=N`, 128 through 3072 MiB.
Default stays 256 MiB. Unit tests check the full range, E820, initrd limits and
separation from MMIO at GPA `0xd0000000`; actual Linux regression covers 256/512/
1024 MiB, not live 2/3 GiB or Android. More than 3 GiB needs discontiguous GPA RAM
and matching E820/placement support; it is intentionally rejected. This is not a
host/guest RAM cleaner, SMP support or a measured speed improvement.

## Remaining integration gates

1. Broader filesystem workloads, corruption/failure injection and host-crash/power-loss
   testing. The clean ext4 and fsynced unclean-VM journal-replay gates above pass;
   guest-initiated reboot, arbitrary interrupted writes and production durability
   are not certified.
2. Android system/vendor/data partition layout, init/fstab/AVB/encryption and
   SELinux integration; a usable Android Home and actual app-ANR diagnosis.
3. Broader valid descriptor layouts and workloads. Current requests remain bounded
   to 128 KiB, direct descriptors, one data segment and separate header/status.
4. Async/batched IO, SMP-safe guest-memory synchronization, quotas, overlay
   compaction and recovery UX. This is still synchronous single-vCPU bring-up.
5. Player connection, accelerated graphics, input, networking/audio and end-to-end
   readiness/performance validation on the supported Windows 10 hardware.

The roughly **100-second launch-to-usable-Android-Home goal remains unverified**.
No Android image or user disk was modified; the player does not use this device yet.
