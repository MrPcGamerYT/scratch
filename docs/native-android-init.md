# Native Android 11: original kernel → second-stage init

Verified on Windows WHP using the independent Rust engine, without QEMU.
This is an integration checkpoint, **not a usable Android player release**.

## Verified path

- Original Bliss 14.10.3 Android 11 kernel and initrd; asset SHA manifests are checked before boot.
- New isolated 4 GiB ext4 filesystem containing the original `system.sfs` at `/android/system.sfs`, plus an empty `/android/data` directory. Native import produces `android-system.zlag`; each probe creates a fresh COW overlay. User disks are not involved.
- Upstream `/init` runs directly (`rdinit=/init`), finds `/dev/vda` using `ROOT=/dev/vda SRC=/android`, mounts the nested original system image, and hands off to Android init.
- Android loads SELinux policy and executes its real second-stage init. The exact serial marker is `init: init second stage started!`.
- Serial output now supports non-FIFO THRE IRQ4 and UART probe loopback. PIC priority/masking/edge tests cover IRQ4 alongside IRQ0 and VirtIO IRQ5. No external serial input or baud/FIFO emulation is claimed.
- `ignore_loglevel` keeps the checkpoint visible after upstream userspace changes console verbosity; absent marker still fails. A diagnostic trace invocation also passed but is not used for direct-entry acceptance.
- Engine emits a separate `android_init_second_stage_ms`; it never assigns Android Home timing or declares the 100-second Home target met.

## CI evidence

Code `59bf989` includes UART/PIC regression tests and runs four Android asset/CPIO-contract host tests on both foundation platforms.

| Run | Result | Launch → second-stage init | IRQ4 injections | VirtIO IRQ5 injections |
| --- | --- | ---: | ---: | ---: |
| 45783235380 (7d83e12) | PASS | 27,557 ms | 10 | 324 |
| 45783239306 (59bf989) | PASS | 27,267 ms | 10 | 321 |
| 45783243550 (59bf989) | PASS | 30,098 ms | 9 | 318 |

All three verify that the ZLAG base SHA remains unchanged, with original initrd/system payload and native VirtIO root. These measurements stop at second-stage init, **not Home**. They are hosted-runner observations, not low-end-PC benchmarks.

Foundation `45783235643` PASS (Ubuntu and Windows); Linux storage/display regression `45783247803` PASS.
Reusable native system disk artifact: `native-android-system-disk`, run `45778665211` (short retention; rebuild from the pinned ISO cache if expired).

Run the checkpoint:

```sh
gh workflow run native-android-system.yml --ref arena/b4ea6e6d-scratch -f disk_run=45778665211
```

Each run retains serial logs, timing JSON, checkpoint JSON, source manifests and software framebuffer capture. The original kernel has no built-in simplefb driver, so a framebuffer capture is not proof of an Android display.

## Next observed blocker (not resolved)

The deliberately longer observation run `45782609955` continues beyond second-stage init and **fails** the bounded deadline, as intended for an incomplete boot. Android starts real services, but `surfaceflinger` repeatedly exits with signal 6; its restart policy kills/restarts zygote and related services. This does not establish a Java framework or Home boot. The captured console alone does not establish SurfaceFlinger's precise abort cause; do not call this a fixed ANR or a confirmed GPU-only diagnosis.

`observe=true` selects `probe-android-observe-create`, which records the same second-stage timestamp but does not stop there. It retains failure evidence and never treats deadline/exit-budget exhaustion as success. No userspace/framework success marker has been implemented for this mode.

Remaining work includes guest graphics/compositor integration and crash evidence, framework/Home readiness verification, native window/input/network/audio, acceleration and usable-Home performance. No claim that the complete emulator, Android apps, or approximately 100-second Home goal works follows from this checkpoint.
