# `gustimg.zlag` — ZLAG disk-container v1

This is a new container, **not renamed QCOW2**. The independent Rust loader does
not depend on QEMU. There is no Android boot implementation using this format yet.
Do not replace your player's `images` folder with this experimental container.

All integers are little endian. Header size is 64 bytes, payload is an uncompressed
standalone raw disk. Exact file length must be 64 + disk bytes; no trailing data.

| Offset | Size | Value |
| --- | --- | --- |
| 0 | 8 | `ZLAGIMG\0` |
| 8 | 4 | version 1 |
| 12 | 4 | header bytes 64 |
| 16 | 4 | logical sector size 512 |
| 20 | 4 | flags 0; unsupported flags refused |
| 24 | 8 | raw disk bytes; nonzero, multiple of 512, at most 2 TiB |
| 32 | 4 | IEEE CRC32 of the entire payload |
| 36 | 24 | reserved, all zero |
| 60 | 4 | IEEE CRC32 of header bytes [0,60) |
| 64 | disk bytes | raw disk sectors |

CRC32 is accidental-corruption detection, **not authentication or cryptographic
integrity**. A future distribution/signature scheme must additionally authenticate
images. There is no compression, encryption, snapshots or writable overlay in v1.
Disk reads are read-only and range checked with checked arithmetic. Opening
validates header/length; `verify` additionally reads/checks the entire payload.

Commands in the separate foundation build:

```
nexus-engine import-raw stopped-standalone.raw gustimg.zlag
nexus-engine verify gustimg.zlag
nexus-engine capabilities
nexus-engine whp-probe
```

Import requires an offline source, creates a new destination exclusively, and
commits the valid header only after the payload has been written/flushed. A failed
import may leave an invalid-header partial destination. Do not modify the source
while importing: checking length cannot detect concurrent same-length changes.
QCOW2, VHDX and existing ZLAG magic are explicitly refused as raw inputs; this is
not a general image-format detector or converter. An unrelated unknown container
may still look like raw bytes; users must supply an actual standalone raw disk.
Existing guest QCOW2 images have NOT been converted by this work.

Future format changes require versioning and compatibility tests, not silent
reinterpretation of existing files. `boot` deliberately returns an error until
there is an actual CPU, firmware, device and Android boot path.
