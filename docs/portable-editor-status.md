# Portable Windows 10 editor — implementation status

This is work in progress, not a released or packaged editor. The existing ISO
installer and Linux/WSLg file editor remain unchanged.

## Architecture being developed

Bundle QEMU, a Linux maintenance kernel/initramfs, Python and filesystem tools
with the Qt application. Windows must not need WSL, Docker, Python, disk drivers,
or separately installed command-line programs. Run with software emulation as a
baseline so hardware virtualization is optional. No writable host filesystem
shares, network, or original guest disk are to be attached to this VM.

The Windows controller must first create an exclusive standalone working copy
using bundled qemu-img, reject backing chains/in-use sources, and attach only
that copy and a disposable scratch disk. Import/export must use a bounded local
protocol, not a writable host share. The original is never modified.

## Implemented so far

`image-studio-tools/portable_workspace.py` is an initial **Linux-side library**,
not the portable application. It recognizes ext4 Android-x86 layouts and the
nested system.sfs → system.img layout, preserves file permissions/xattrs on
replacement, refuses symlink traversal and retains unsaved work. APK staging
uses aapt package metadata and keeps existing launcher files. It refuses
unsupported nested filesystem formats instead of silently modifying them.

Four host-side safety tests pass (`python3 tests/portable_workspace_test.py`).
These do not test mounting, Android boot, APK installation, or Windows operation.

## Still required before delivery

- Build and pin a redistributable maintenance appliance with licences/SBOM;
  include squashfs-tools, e2fsprogs, util-linux, Python and aapt.
- Implement isolated transport, explicit browse/import/export/edit operations,
  limits/timeouts, shutdown and recovery, and Windows controller/UI integration.
- Enforce disk-copy/locking and scratch-only format rules in the boot service.
- Handle safe capacity checks, repack interruption/recovery, validation and
  error cleanup. Directory-based squashfs systems are not supported yet.
- Add launcher replacement confirmation, signature/privilege warnings, and
  fallback guidance: a staged APK is not a validated Android install. Preserve
  an existing working launcher until a separate boot test succeeds.
- Inspect the real initrd boot-banner script and patch only user-facing output;
  never blanket-replace Bliss identifiers, package names or copyright notices.
- Test real nested filesystem round trips and recovery in Linux CI, then build,
  extract and test the self-contained Windows 10 package and boot an edited
  guest through the existing Home/ANR gates.

No new download should be advertised until those delivery gates pass.
