# Graphics backend integration status

## What is wired, versus what is verified

The experimental Player OpenGL/VirGL setting now maps to **actual** QEMU device
and display arguments: `virtio-vga-gl,id=nexus_display` and `-display sdl,gl=on`.
It is not a renamed Standard VGA mode. Rust owns the argument policy; the Player
checks that QEMU advertises the device before launch and never silently falls
back to 2D scanout when the backend is missing. GL context errors remain launch
errors and are captured in the normal session diagnostics.

The Windows QEMU source build now explicitly requires OpenGL + virglrenderer,
with MSYS2 UCRT64 virglrenderer/libepoxy dependencies and their existing DLL and
license collection. This requires a new engine build; old cached QEMU artifacts
do not establish these capabilities. Compilation and actual Windows guest GL
rendering are separate validation gates. This change is not a certification.

The image's compatible CPU mask can be requested with `images/cpu-profile.txt`
containing exactly `reduced-avx`; only TCG uses this fixed whitelist. No arbitrary
CPU arguments are read from the guest. This file is a staging contract, not an
automatic installer or permission to overwrite an existing base/overlay.
The current distributed experimental guest's standalone launcher already uses
the reduced-AVX mask. Player staging/firmware and image identity must still match.

## Real OpenGL test

The Linux Android investigation now exercises SDL OpenGL + VirGL in Xvfb,
with software host GL explicitly enabled because GitHub hosted runners do not
provide a reference physical GPU. It requires the previous service-readiness
checks **and a VirGL string in the guest SurfaceFlinger renderer dump**.
The dump is retained as surfaceflinger.txt. A software fallback in Android must
not count as a VirGL pass. This tests the guest/host GL transport, not physical
GPU acceleration, Windows WGL, game compatibility, speed or Vulkan.

## Not implemented

- DirectX-backed Android rendering (requires a separate compatible translation
  stack; Windows SDL/Qt interface rendering is not Android DirectX acceleration).
- Vulkan guest/host transport and validation (a reported `ro.hardware.vulkan`
  property or a host Vulkan driver is not proof of working guest Vulkan).
- Automatic integrated/dedicated adapter selection for guest rendering.

These choices remain unavailable rather than presenting disconnected dropdowns.
Gfxstream/rutabaga or another suitably licensed stack requires separate guest
transport, Windows host integration, build/license work and API tests. The
current guest may need a driver/HAL rebuild for that path; do not promise that
rebuilding only the host will enable all three API families.

## Source feasibility checks

- QEMU pinned source meson.build resolves virglrenderer/libepoxy for the GL path.
- MSYS2 MINGW-packages/mingw-w64-virglrenderer/PKGBUILD provides a UCRT64 Windows
  package (observed 1.3.0) and copies its MIT license.
- google/gfxstream README describes graphics API streaming; its existence does
  not imply a drop-in QEMU/Bliss/Windows integration.

## Recorded results for this integration

- Windows source build and complete native/application/package suite passed:
  https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45580334830
- Earlier real GL run reached framework/service readiness but failed the QMP
  screenshot gate (`no surface`). That was retained as a **failed** run:
  https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45580335016
- Its saved SurfaceFlinger dump was inspected through:
  https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45584826931

The actual guest renderer line was:

```
GLES: Mesa, virgl (LLVMPIPE (LLVM 20.1.2, 256 bits)), OpenGL ES 3.2 Mesa 24.0.8
```

This establishes that Android selected VirGL for its GLES compositor path in that
run. LLVMpipe was explicitly forced on the Linux host: this is **not physical GPU
acceleration**, an app conformance test, or a Windows result. QMP software-surface
capture is inappropriate for that GL path, so the probe now requires a complete
PNG from Android `screencap` instead. A subsequent stricter repeat run
45582628910 timed out before boot completion; **repeatable startup is not solved**.
No green result is claimed for the full Android GL test or for Windows Android GL.

DirectX and Vulkan integration remains unimplemented. Do not market the new
experimental setting as three working graphics APIs, game compatibility, or
hardware acceleration on the user's PC. The downloaded 2D candidate remains
unchanged; no need to replace it just to obtain a different UI label.
