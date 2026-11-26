# Native guest software scanout

This is a **software framebuffer transport**, not an accelerated GPU, interactive
player window, Android graphics HAL, OpenGL/Vulkan/DirectX implementation or FPS
measurement. No Android image, editor or user disk is changed by this checkpoint.

## Implementation

- A fixed 800×600, 32-bit little-endian B,G,R,X surface lives at GPA `0xc0000000`.
  It is separate from ordinary guest RAM and the VirtIO block page at `0xd0000000`.
  The 3072 MiB RAM limit ends exactly before this framebuffer window.
- The Linux `screen_info` handoff describes the geometry, pitch, color masks and
  linear framebuffer. A fourth E820 entry reserves the surface against the guest
  page allocator. No BIOS calls, VGA host-port passthrough or QEMU are used.
- WHP maps a separate host allocation read/write at that GPA. Mapping teardown
  unmaps its actual GPA, not ordinary RAM's GPA zero. Instruction emulation and
  checked block DMA continue to treat the surface as outside normal guest RAM.
- The opt-in test kernel uses SYSFB_SIMPLEFB and FB_SIMPLE to expose `/dev/fb0`.
- A capture is made only after guest execution stops and synchronous workers have
  returned. B,G,R,X pixels are converted to binary RGB PPM; reserved padding and
  the X channel are excluded. Capture files are created exclusively **before VM
  execution**, so an existing file is never overwritten or used to start a VM.

Example diagnostic invocation:

```
nexus-engine boot-kernel <bzImage> <NEW-serial.log> <cmdline> <initramfs> --framebuffer=<NEW-capture.ppm> --ram-mib=256
```

The two options may appear in either trailing order. Duplicate/empty capture
options and duplicate/invalid RAM values are rejected before kernel input IO.
The disk boot variants accept the same trailing options. Captures from a failed
boot may exist for diagnosis; their existence does not imply guest readiness.
The bounded Linux diagnostic still stops at its diagnostic PID1 marker.

## Real guest acceptance

Tested code `33a95819486b581ab7fad7d239838c1e4bdc1a53` passed native Linux run
**45775724726**, using the kernel built by **45774576016**. Foundation
**45775957425** passed Windows/Ubuntu tests, strict Clippy, release builds, SDK ABI
checks and repeated real WHP fixtures at that same exact code commit. The display test runs
three separate WHP guests and requires all of the following:

1. Diagnostic PID1 mounts devtmpfs and opens the real Linux `/dev/fb0` read/write.
2. FBIOGET_VSCREENINFO reports 800×600 and 32 bits per pixel.
3. Six hundred checked 3200-byte write syscalls draw an asymmetric two-axis color
   pattern; every next row is tinted differently to detect pitch/orientation and
   channel mistakes, rather than accepting a blank or uniform-color surface.
4. A shared read/write mmap of the page-rounded visible surface succeeds. The
   guest stores and reads back a sentinel at the final pixel through guest paging,
   then unmaps and closes fbdev. Reserved allocation padding is not mapped by the
   guest mmap request.
5. The guest emits `NEXUS_FB0_PATTERN_WRITTEN` and reaches diagnostic readiness.
   Host capture must match **every RGB byte** of an independently computed
   reference including the mmap sentinel; merely obtaining a file is insufficient.
6. Repeating the command with the same capture path and a fresh log fails without
   any guest CPU execution, preserves the exact capture, and produces no serial
   output. Timing must retain `vcpu_run_requested_ms:null` for this refusal.

`DISPLAY_ACCEPTANCE` records `guest_fbdev_write:true`, `guest_fbdev_mmap:true`,
`exact_host_pixels:true`, `repetitions:3` and
`existing_capture_refused_before_vcpu:true`, while `accelerated_gpu:false` and
`android_ready:false` remain explicit. CI uploads `display-*.ppm`, serial logs,
initramfs and `display-checkpoint.json`; generated images are actual guest output,
not a rendered mockup of Android.

The same run retains all eight raw-block/ext4 clean-remount/journal-replay boot
checks. New unit tests cover the handoff and reserved memory, color conversion,
padding/length rejection and overwrite refusal. Fixture/oracle tests cover the
ELF/archive, geometry ioctl and exact write/mmap sentinel instructions. They do
not substitute for real guest execution.

## Outstanding work

Continuous frame presentation and input, an actual virtual GPU protocol and
matching guest DRM/Mesa/HAL stack, host graphics rendering, Android SurfaceFlinger
and usable Home, app ANR diagnosis/remediation and end-to-end performance testing
remain unfinished. This scanout does not certify any physical graphics card or
the approximately 100-second launch-to-usable-Android-Home target.

Linux ABI references: pinned Linux v6.6 `include/uapi/linux/screen_info.h`,
`drivers/firmware/sysfb*.c` and `drivers/video/fbdev/simplefb.c`, at source commit
`ffc253263a1375a65fa6c9f62a893e9767fbebfa`. Test kernel options are in
`.github/workflows/native-kernel.yml`; no proprietary emulator source is used.
