# Actual Bliss Android validation — investigation, not a release certificate

## Evidence

- Android run: https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45558822347
- Full existing application/packaging suite: https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45558861278
- Evidence inspection: https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45561018624
- Fixture: Bliss-v16.9.7-x86_64-OFFICIAL-gapps-20241011.iso
- Recorded SHA-256 matched: `17137711fb42236640ac6fe4421fcb3bd15710065fcf75c20807c018f42e3751`.
  This matches the user's diagnostic fixture; it does not independently authenticate the publisher.
- Environment: GitHub Ubuntu runner, Linux QEMU, q35, TCG `-cpu max`,
  two vCPUs, 3072 MiB. Live ISO, direct boot of its own kernel/initrd using
  its ISOLINUX live arguments plus serial console. No installed virtual disk.

## Results

| Check | Result |
|---|---|
| Windows application build, Rust/native/launcher tests, packaging checks | Passed existing suite |
| Exact ISO download and recorded fingerprint | Passed |
| Real guest kernel starts and finds ISO | Observed in VirtIO serial output |
| Standard VGA Android completion | Not verified within 12-minute boot observation; no three boot-complete replies |
| VirtIO Android framework | Three consecutive ADB `sys.boot_completed=1` replies |
| VirtIO usable desktop | **Not validated**: screenshot still boot animation; Setup Wizard software-renderer crash in logcat |
| Installed-image boot and persistence | Not tested here |
| Windows guest rendering / WHPX on user's host | Not tested here |
| Accelerated gaming / FPS / input latency | Not validated |

The original VirtIO workflow job was green because its initial gate checked only
framework boot completion. **That is not an Android graphics pass.** Inspection
caught the false-positive readiness criterion, and the probe was hardened to
require compositor service readiness, stopped boot animation, and no captured
fatal renderer/compositor/setup-wizard signal. These classifiers have unit tests;
the stricter real-guest rerun is separate from the original evidence above.

## Concrete observations

Serial output includes:

```
Found at /dev/sr0
# WARNING: The system is not prepared with Virtual A/B,
# so OTA Update will not be supported
```

Android nevertheless subsequently returned boot completion over ADB in the
VirtIO test. Thus this OTA warning is **not itself a fatal boot error** in that
run; its presence alone cannot diagnose a user's stalled screen.

Selected captured properties:

```
[ro.build.version.release]: [13]
[ro.hardware]: [bliss_x86_64]
[ro.hardware.egl]: [mesa]
[ro.hardware.gralloc]: [minigbm_arcvm]
[ro.hardware.hwcomposer]: [drm_minigbm_celadon]
[init.svc.surfaceflinger]: [running]
[init.svc.bootanim]: [running]
[sys.boot_completed]: [1]
```

Selected logcat evidence:

```
EGL-MAIN: Failed to open any DRM device
SurfaceFlinger: Boot is finished (521248 ms)
Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR) ... (llvmpipe-0) ... (oid.setupwizard)
```

This establishes a real software-rendering/UI failure in a clean CI guest,
without relying on the user's PC. It does not establish the precise root cause
of that crash or prove it is identical to the user's Standard VGA corruption.
Do not infer physical GPU damage, claim acceleration, publish a validated base,
or ask the user to reinstall based on these results.

## Next engineering gate

Find a guest renderer/virtual-hardware combination that survives Setup Wizard
and renders a readable desktop, capture the final screen, repeat on the bundled
Windows QEMU, then test installed-disk shutdown/reboot and persistence. Only then
should unattended preparation publish an image. Bypassing Setup Wizard alone
would hide this failure, not demonstrate a correct graphics stack.
