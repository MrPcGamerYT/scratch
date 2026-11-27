# Nexus guest-image compatibility contract (Phase 1 gate)

Status: engineering target, **no validated Nexus guest image published yet**.

## Current investigation

The recorded Bliss 16.9.7 diagnostic ISO is not the proposed proprietary Nexus
product. It reproduces a Setup Wizard llvmpipe crash on a clean Linux TCG guest.
`android-boot.yml` now compares three explicit hypotheses with VirtIO scanout:

- `no-avx`: limit advertised AVX/FMA CPU features, unchanged guest renderer.
- `drm-gbm`: select the guest's DRM/GBM composer/gralloc options, unchanged CPU.
- `drm-gbm-no-avx`: combine the two after isolating their individual effects.

These are experimental compatibility probes, not performance presets. None
activates host GPU acceleration or changes the production Player defaults.
The baseline and Standard VGA evidence is recorded in android-validation.md.
Tests do not disable or skip Setup Wizard to conceal its crash. Evidence is
retained even if Android never reports boot completion. Each probe also captures
a final screen before terminating its isolated live guest.

## How the eventual prepared image will be produced

1. Select one source release and architecture (Android x86-64), with recorded
   source revision, license inventory, verified download provenance and patches.
   Prefer a suitable FOSS build for redistribution; do not bundle Google services
   or ARM translation binaries without the necessary distribution rights.
2. Integrate guest kernel, graphics HALs and user-space renderer compatible with
   the exact host QEMU build. Validate a software baseline before claiming host
   acceleration. A working Linux guest is not proof of Windows compatibility.
3. Install into a **new isolated virtual QCOW2 disk**, never a host physical
   drive. Retain matching firmware variables. Do not publish a disk backed by
   another local image, a running disk, or an unverified ISO extraction.
4. Test on the bundled Windows backend: readable Setup Wizard and launcher,
   responsive input, compositor stability, clean shutdown, second boot, and
   persistence of a test file. Validate audio/network separately; services
   starting does not prove either works.
5. Export a clean standalone base, remove test accounts/keys/logs, preserve
   required notices/source offers, hash every distributed component, and record
   QEMU version, machine type, firmware, display, CPU requirements and kernel
   arguments in a versioned compatibility manifest. Do not rely on renaming
   an arbitrary .img file to .qcow2.
6. Upload the verified image and SHA-256 manifest to a versioned GitHub Release
   or build artifact (not Git history), with exact validation results and any
   limits. Failed test artifacts must be labelled diagnostic, not ready-to-use.
7. Player boots `images/android-base.qcow2` using its managed per-user overlay;
   match BIOS/UEFI staging to the guest. Never replace a backing base beneath
   an existing user overlay. Updates require a migration/instance policy.

## Speed validation

Only compare profiles after functional tests pass. Record host CPU/GPU/driver,
acceleration, warm/cold boot time, frame-time distribution, guest/host memory and
input latency with the same workload. TCG software rendering is a compatibility
baseline, not a competitive gaming engine. CPU instruction masking can reduce
speed and is being tested solely as a crash-isolation hypothesis.

## What the user should do now

Nothing needs to be reinstalled on the user's PC for this investigation. Keep
existing workspaces as backups. GitHub runs the experiments and retains evidence.
`guest-tools/prepare_guest.py` and `guest-candidate.yml` now implement an
experimental disk-construction path: fresh GPT QCOW2, EFI bootloader, ext4 Android
payload and writable data directory, source/image hashes and compatibility
manifest. A CI job attempts disk-only UEFI boot with no attached installer ISO.
The chosen DRM/GBM + reduced-AVX combination is an unproven hypothesis, not a
selected production default. The candidate remains `ready_for_player: false`.
CI uploads build evidence, not a redistributable image. Desktop inspection,
second boot/persistence, Windows validation and redistribution review remain
required. This is not yet a certified production-image pipeline.
