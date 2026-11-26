# Accelerated graphics — host renderer foundation, not completed guest acceleration

User milestone order: **3 graphics first; 4 device integration only after graphics
passes**. The native player still presents software snapshots. Android Home is not
usable yet, and SurfaceFlinger restarts in longer boot observations.

## Implemented host component

`rust/nexus-engine/src/graphics.rs` uses the Windows Direct3D 11 APIs directly from
Rust, with no QEMU, third-party rendering crate or proprietary emulator engine.

- Explicit hardware request (`D3D_DRIVER_TYPE_HARDWARE`) or explicit WARP CPU
  renderer (`D3D_DRIVER_TYPE_WARP`). No silent hardware-to-WARP fallback.
- Compile vertex/pixel HLSL with the OS D3DCompiler, create shaders and render target,
  set viewport/topology, issue an actual triangle draw, copy to staging memory,
  synchronize via a blocking read Map, and check every pixel including row pitch.
- The shader generates an asymmetric 64×64 RGBA pattern; an independent Python
  oracle checks every byte. A distinct clear color cannot pass as shader output.
- Owned COM handles release on error and normal return. Readback is unmapped before
  resource destruction. Device-removed status is checked.
- DXGI adapter name, vendor/device IDs, flags and selected feature level are recorded.
  A hardware request is **not** itself evidence of physical GPU execution.
- Reject software-flagged DXGI adapters and the known Microsoft Basic Render Driver
  vendor/device/name, even if DXGI reports Flags=0. This exact inconsistency occurred
  on the hosted Windows runner; it must not become a false hardware pass.
- Output files are created exclusively; repeated commands cannot overwrite a prior
  render. Windows SDK and Rust tests check FFI layouts.

Developer commands:

```text
nexus-engine graphics-smoke-hardware NEW-output.rgba
nexus-engine graphics-smoke-warp NEW-output.rgba
```

The output is raw 64×64 RGBA, not an Android screenshot. Each CI invocation has a
30-second host timeout. The host renderer currently has **no guest command stream,
shared-resource protocol, swapchain, virgl/Venus translator or Android HAL link**.
Neither command establishes guest acceleration or app/game compatibility.

## Guest contract audit

Workflow `native-graphics.yml`, run **45788367253**, passed a read-only audit of the
pinned original Android system payload. It verifies the ISO fingerprint, extracts
the nested filesystem, uses debugfs without `-w`, and checks the filesystem SHA
before/after. No user image or upstream system content is patched.

Observed original kernel configuration:

| Option | Value |
| --- | --- |
| DRM | module |
| DRM_VIRTIO_GPU | module |
| DRM_BOCHS / DRM_VMWGFX / DRM_QXL | module |
| VIRTIO_MMIO / VIRTIO_PCI | built in |
| FB / FB_VESA | built in |
| FB_SIMPLE / SYSFB_SIMPLEFB | not set |

Original guest library inventory includes Mesa EGL/GLES, ANGLE, SwiftShader,
`vulkan.virtio.so`, gralloc variants and DRM hwcomposer variants. Library presence
is not proof of ABI compatibility, a selected driver, working rendering or a loaded
module. The current emulator exposes no GPU device. This audit does not establish
SurfaceFlinger's precise abort cause; crash/logcat evidence is still required.
Full inventory: artifact `native-graphics-guest-contract` / `native-graphics-guest-audit.json`.

## Verification and limits

Foundation **45788257697** passed both platforms and the shader/readback oracle.
A subsequent identity-aware run **45788750133** exposed Basic Render Driver with
Flags=0 for the hardware request, prompting the stricter classification guard.
**Do not use those earlier runs' `hardware_renderer_verified:true` as hardware
certification.** The corrected guard must report hardware unavailable on that
adapter while continuing to test the separately labeled WARP implementation.

CI retains `native-engine-graphics.json` alongside normal foundation evidence.
`guest_acceleration_verified` and `android_graphics_verified` remain false.
Corrected foundation **45789044283** passed Ubuntu/Windows tests, strict Clippy,
SDK ABI checks and three independent WARP shader/readback/output-preservation
checks. It explicitly reports `hardware_renderer_verified:false` and
`GRAPHICS_DEVICE_UNAVAILABLE` for Basic Render Driver; feature level 0xb000 is
reported for the tested WARP path. Actual supported physical-GPU execution is
not yet verified.

Post-change Linux display/storage regression **45789261262** and original Android
second-stage-init regression **45789266159** both passed at code `2b5a6b5`.
Neither is a guest-acceleration test.

## Gates before declaring milestone 3 done

1. Real GPU adapter render/readback and recovery on supported Windows 10 hosts,
   with explicit unsupported-device reporting. A hosted CPU renderer is insufficient.
2. Implement and verify a bounded guest GPU transport and advertised feature/capset
   contract. Never offer 3D/Venus/virgl features solely because libraries exist.
3. Match the guest DRM/Mesa/HAL stack to that protocol and implement host command
   translation, resource ownership, synchronization/fences, reset and context loss.
4. Resolve the Android compositor abort using actual crash evidence; verify sustained
   SurfaceFlinger/framework/Home and a nontrivial guest-rendered reference scene.
5. Verify accelerated output correctness, native-window presentation and frame/input
   latency. Then proceed to milestone 4, not before.
