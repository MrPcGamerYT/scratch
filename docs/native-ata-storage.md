# ZLAG-backed ATA PIO storage bring-up

## Verified result

At commit `899e843`, foundation run
[45753956772](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45753956772)
passed Windows and Ubuntu tests, strict Clippy and release builds. The Windows
runner executed a real protected-mode vCPU that:

1. Issued ATA IDENTIFY through primary-master task-file ports.
2. Read all 256 words and stored the capacity response in guest RAM.
3. Issued a one-sector LBA28 PIO read of sector 1 from an actual verified ZLAG file.
4. Read 256 words using scalar guest `IN AX,DX` and stored them in guest RAM.
5. Issued a prohibited write command and observed ATA error status.
6. Halted with an unchanged stack.

The host compared exact sector bytes and IDENTIFY capacity. Python independently
checked the original raw payload and ZLAG SHA256 remained unchanged. This ran
three times. A corrupt ZLAG was refused before guest execution. The Windows SDK
ABI and the existing CPU, timer, cancellation, kernel and image contracts stayed
green. Pure Rust tests cover multi-sector reads, zero count = 256 sectors,
register-width preservation, bounds, unsupported commands, reset, absent slave
and corruption refusal.

This is a **read-only polled storage device test**, not Linux filesystem mounting,
Android storage, persistence, DMA or throughput certification. The ordinary Linux
PID1 checkpoint still boots its initramfs; it does not mount this disk.

## Interface and limits

- Primary master: task-file `0x1f0..0x1f7`, alternate status/control `0x3f6`.
- Data register requires scalar 16-bit I/O; other registers are byte-only.
- ATA IDENTIFY (`0xec`) and READ SECTORS (`0x20`), LBA28 only.
- Capacity up to 128 GiB. Sector count zero means 256; a request buffers at most
  128 KiB, not the whole disk.
- Payload reads use the image sector API: the 64-byte ZLAG container header is
  not visible as a disk sector.
- `AtaPio::open()` validates the image and full payload CRC before attachment.
  CRC is corruption detection, not authentication. Use stopped/offline source
  images; concurrent external modification is unsupported.
- `EarlyPlatform` accepts an optional ATA device; the default machine has none.
  Device attachment is currently exercised by the explicit smoke command, not
  the player or normal Linux `boot-kernel` command.
- Default and supported control state has nIEN set (polled operation). Attempting
  IRQ-enabled operation fails explicitly because IRQ14 is not implemented.
- Software reset drops incomplete transfers and restores the task-file signature.
  Master/slave selection is modeled; there is no slave device.
- Writes, DMA, LBA48, ATAPI and other unsupported commands return ABRT without
  writing any host image. Out-of-range reads return IDNF and no data transfer.
  Host backing-file read failures stop execution rather than fabricate data.
- No guest port is forwarded to host hardware. Invalid widths/unknown port
  requests fail closed. No REP/string I/O support was added.

Developer-only Windows invocation:

```text
nexus-engine ata-pio-smoke <offline-test.zlag>
```

The image must have at least two sectors. This does not install Android or boot
an arbitrary operating system. Linux/macOS refuse the live WHP command rather
than substitute an emulator.

## Next storage dependencies

Android needs safe writable `/data` and system/vendor integration, compatible
guest block drivers, interrupt delivery and a persistent overlay/journaling
contract with crash recovery. None is supplied by a read-only PIO fixture.

PIO's per-word exits are useful for bring-up but are **not** the intended final
high-throughput gaming storage path. A batched block device path (for example,
virtio with a matching Android driver) must be implemented, validated and
measured before making boot-speed or IO-performance claims. Full-image CRC
verification must also be included in end-to-end startup measurements; this
change does not bypass integrity checks to improve a timing number.

The current Android launch-to-usable-Home goal is approximately **100 seconds**.
It is independent of the bounded diagnostic test deadlines and remains unverified.
