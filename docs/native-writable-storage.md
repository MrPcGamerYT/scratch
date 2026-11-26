# Persistent sector overlay and writable ATA bring-up

The base `gustimg.zlag` is opened read-only. Guest data writes can now be directed
to a separate append-only `.cow` overlay. This is a diagnostic storage milestone,
not a finished Android storage stack or a new guest image release.

## Implemented contract

- `Overlay::create(base, NEW_overlay)` verifies the base and creates exclusively.
  It never overwrites an existing image/overlay.
- `Overlay::open(base, overlay)` verifies base integrity, exact header/base
  size/CRC binding, sequential records, sector bounds, record CRC and commit tag.
- Unchanged sectors fall back to the original base. For changed sectors the last
  valid record wins. Overlay records are revalidated on reads.
- Sector writes append a record and call `sync_data()` before updating the live
  index/returning success. `flush()` calls `sync_all()`.
- Host append/sync failures poison the instance: subsequent reads, writes and
  flushes fail until it is closed and inspected/reopened. No successful guest
  write is reported after such an error.
- Windows sharing flags and Unix advisory `flock` prevent a second cooperating
  overlay writer. The base must remain offline/externally unmodified.
- Default reopen is strict. An incomplete trailing record causes an error and
  does not mutate the file. Explicit `recover_incomplete_tail()` validates every
  complete record first, then truncates only a short last record and syncs.
  Complete bad-CRC/commit/sequence records are never automatically repaired.
- A multi-sector request is **not atomic**: sectors committed before a later
  error remain committed. Filesystem journaling and guest flush/barriers are
  still needed. CRC is accidental-corruption detection, not authentication.

## Format v1

Little-endian, 512-byte virtual sectors. Overlay is a separate file, not a changed
ZLAG base format.

Header (64 bytes):

| Offset | Content |
|---|---|
| 0..8 | `NXCOW001` |
| 8..12 | Version 1 |
| 12..16 | Sector size 512 |
| 16..24 | Base payload byte length |
| 24..28 | Base payload CRC32 |
| 28..60 | Reserved, all zero |
| 60..64 | CRC32 of header bytes 0..60 |

Record (536 bytes):

| Offset | Content |
|---|---|
| 0..8 | Sequence, starting at 1 and incrementing by one |
| 8..16 | Sector LBA |
| 16..528 | 512-byte sector payload |
| 528..532 | CRC32 of record bytes 0..528 |
| 532..536 | `CMIT` tag |

## ATA integration

`AtaPio::with_overlay(base, overlay, create)` enables WRITE SECTORS (`0x30`) and
FLUSH CACHE (`0xe7`). A sector is committed only after all 256 scalar guest data
words arrive. Reset discards an incomplete sector; already committed sectors
remain. READ SECTORS (`0x20`) sees the overlay first. Existing `AtaPio::open()`
remains read-only and rejects writes/flushes. ATA IDENTIFY model names are now
exactly 40 bytes so they cannot overwrite subsequent capability words.

Developer-only Windows test command:

```text
nexus-engine ata-cow-smoke <offline-test.zlag> <NEW-overlay.cow>
```

The live protected-mode guest identifies the disk, writes a known sector,
flushes, reads it back into guest RAM and checks rejection of unsupported DMA.
After vCPU/device teardown the host reopens the overlay separately and verifies
that the guest-written bytes persisted. CI also checks base SHA256 preservation,
exclusive creation and three repetitions. The normal Linux PID1 boot still uses
its initramfs and does not mount this device. No player disk was changed.

## Safety tests and remaining limits

Tests cover persistence/latest-write resolution, unchanged-sector fallback,
wrong-base/header/record corruption refusal, second-writer refusal, bounds,
explicit incomplete-tail recovery, partial guest-sector reset and an injected
host write failure that poisons the instance while preserving prior commits.
These are simulated failure tests, **not physical power-loss certification**.
Filesystem/device guarantees apply; parent-directory durability, storage-cache
behavior and real power interruption require further platform validation.

Still needed for production Android:

- IRQ14/cascaded interrupts or a matching batched modern block-device transport.
- Guest block drivers and Android system/vendor/data mounting/SELinux integration.
- Disk quotas, bounded/scalable indexing, overlay compaction and lifecycle/UI.
- Filesystem-level crash recovery and broad failure/soak testing.
- High-throughput asynchronous/batched IO; current per-sector sync and per-word
  PIO exits are bring-up correctness paths, not optimized gaming storage.
- Packaging and wiring into the player. No app-freeze fix is implied.

The approximately 100-second launch-to-usable-Android-Home goal remains
unverified. No performance improvement is claimed from these tests.


## Verified CI evidence

Tested commit `a048d9ebcec7d92e2c2f0318c64531c3360d7e68`:

- [Foundation 45755799925](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45755799925) passed Windows/Ubuntu tests, strict Clippy/release builds, SDK ABI and repeated actual WHP guest read/write/flush/reopen checks.
- [Linux PID1 regression 45755800012](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45755800012) passed three Linux diagnostic userspace boots with the approximately 100-second Android goal still unverified.

The Linux test uses an initramfs; it is not a filesystem test of the new overlay.
No user disk or prepared Android guest image was modified in this work.
