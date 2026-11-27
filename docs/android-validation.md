# Actual Bliss Android validation — investigation, not a release certificate

## Experimental download now retained

Rebuild [45575332515](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45575332515)
passed the boot-readiness and packaging gates, and uploaded
[Nexus-Experimental-Android-Guest](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45575332515/artifacts/15004767630)
(approximately 2.16 GB, seven-day retention; GitHub sign-in may be required).
This is an experimental upstream Bliss diagnostic guest, not a finished Nexus OS.
Windows, setup completion, repeat boot/persistence and gaming remain unverified.

The pristine base was verified through a disposable overlay, and its original
hash was checked again before packaging:
`6d51ab7beb663735a625cc6626d12c094952b0668228dfaa7f8c39efcff689b1`.
The package includes full SHA256SUMS, compatibility metadata, matching firmware,
required firmware notices and a Windows diagnostic launcher whose syntax was
checked in CI. That syntax check is **not** a Windows Android boot test.

Extract all files together. Do not replace an existing Player backing image.
Read README.txt and launch Start-ExperimentalGuest.ps1, selecting the QEMU system
executable from the complete updated Developer package. It creates a new retained
instance on each invocation, does not publish ADB host forwarding, and does not
implement audio or persistent-instance selection. No image was uploaded to Drive.
This rebuild's captured service-readiness result is not a new visual certificate;
the earlier inspected screenshot below remains the visual evidence for the recipe.

## Earlier installed-disk result

Candidate run [45571864477](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45571864477)
**passed disk construction, QCOW2 checks and automated UEFI disk-only Android readiness checks**.
Evidence inspection [45573884344](https://github.com/scratchhhhhhhhhhhhhhhhhhhhhhhhhhh/scratch/actions/runs/45573884344)
confirmed a readable Android **Hi there** Setup Wizard screen, not the earlier
striped/tripled display or just a boot logo.

![Actual installed-disk setup screen from CI](evidence/android-installed-setup.jpg)

Configuration: Linux QEMU/TCG, q35, two vCPUs, 3072 MiB, VirtIO VGA,
`max,avx=off,avx2=off,f16c=off,fma=off`, installed kernel arguments
`root=/dev/ram0 SRC=/android HWC=drm GRALLOC=gbm`. The boot command had **no ISO,
no direct -kernel/-initrd injection**, and booted the disk's own EFI GRUB.
Captured properties: `sys.boot_completed=1`, SurfaceFlinger running, boot animation
stopped, gralloc `gbm`, hwcomposer `drm`. No matching fatal renderer/setup-wizard
signal was found in the captured log window. Mesa warnings remain: this does not
prove acceleration or absence of all errors. Probe elapsed time including evidence
collection was 637.9 seconds; do not market this as fast boot.

The image used the ISO's **system.efs** EROFS payload. Its built pre-boot SHA-256
was `4d4ab9e3029c6569c1834613929192359d9ec145b08e350fefccab46e0216a76`.
This hash is not a post-boot disk hash or a download currently offered to users.
The run retained evidence only, **not a downloadable guest disk**.

**Still unverified:** completing Setup Wizard, launcher/game behavior, input/audio/
network functionality, clean shutdown and second boot/persistence, bundled Windows
backend, WHPX and the user's physical PC. The candidate remains experimental;
Phase 1 is not fully complete and no ready-to-use Nexus guest is certified.

## Earlier live-ISO investigation

### Evidence

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
