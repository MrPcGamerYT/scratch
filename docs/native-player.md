# Interactive native player — first implementation slice

The independent Rust/WHP engine now has an opt-in Win32 software presentation window.
This is developer scaffolding, **not the finished Android interactive player**. It does
not use or launch QEMU and does not replace the editor, ISO tools, legacy player, or
existing user disks.

## Implemented

- Resizable Windows window, black letterboxing and aspect-preserving 800×600 BGRX
  software presentation through GDI. No D3D/OpenGL/Vulkan acceleration is claimed.
- Guest-framebuffer snapshots copied only on the VM execution thread between vCPU
  runs, after WHP returns. The UI thread sees owned, mutex-protected bytes, never
  guest mappings. The cancellation worker does not access guest memory.
- At most one new snapshot per 33 ms while running; a bounded final-frame drain
  lets the UI submit the completed surface before teardown. Software blit counts
  are **not guest FPS**.
- WM_CLOSE signals the engine to stop at its next scheduling rendezvous; the VM
  cancellation worker joins before partition/mapping teardown. Closing is an
  unclean guest stop, not Android's graceful shutdown.
- ASCII WM_CHAR input goes through a bounded 256-byte host queue into the one-byte
  UART RX path with IRQ4. Receiver-busy and loopback backpressure prevent overwrite.
  UART input is a serial console, **not Android key/touch/mouse injection**. Host
  queue/UART semantics are unit-tested; a real guest typed-input round trip has
  not yet been certified.
- Win32 FFI structure layouts checked both by Rust and the Windows SDK C++ oracle.

## Developer usage

Append `--window` and an exclusive capture path to an existing boot command:

```text
nexus-engine boot-kernel <bzImage> <NEW-serial.log> <cmdline> <initramfs> --ram-mib=256 --framebuffer=NEW-frame.ppm --window
```

The same flag works with native disk boot and Android probe commands. All existing
probe deadlines and readiness-stop conditions remain in force: this is not yet an
unbounded player session. No existing capture file is overwritten. New disk layers
still require explicit create vs open commands.

`player-window-smoke` opens a real native window, submits a software test surface,
posts WM_CLOSE, verifies the close state, and joins the UI thread. Foundation CI
runs three independent cycles. Linux explicitly refuses a native Windows window.

The `NATIVE_PLAYER_JSON` stdout record reports software blits and the CRC32 of the
last BGRX frame successfully submitted to GDI. It does not prove a physical monitor's
pixels, Android rendering, acceleration, or presentation latency.

## Verification

- Foundation `45786225776` PASS (Ubuntu and Windows), including three actual
  native window create/present/WM_CLOSE cycles, library tests and strict Clippy.
- Linux regression `45786316526` PASS: three actual Linux fbdev write/mmap fixture
  boots, exact final host captures, and independent BGRX CRC comparison against
  the bytes submitted to the native window. Storage/persistence tests also pass.
- Original Android second-stage-init regression `45786320788` PASS after player
  integration; this does not verify Android display or input.

## Next gates

1. Verify window-close cancellation while a real guest is executing, and real
   guest input round trips (serial console first, then a specified Android input
   device/driver protocol).
2. Resolve Android SurfaceFlinger's repeated aborts, with actual crash/logcat
   evidence and a compatible guest display/HAL path. The original Android kernel
   does not provide the diagnostic Linux kernel's simplefb support; a black window
   is not Android graphical boot.
3. Add the agreed thin icon-only controls, fullscreen and correct pointer mapping,
   focus/release behavior, Android Back/Home/Recents and per-app keymaps. The current
   standard Windows titlebar is temporary, not the requested finished design.
4. Verify sustained Android framework/Home operation, then GPU transport,
   networking/audio, app/game input, latency/FPS and end-to-end Home timing.

Only one native player window is opened by a boot command. No web preview server
was started or replaced in this work.
