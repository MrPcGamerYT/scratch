# Nexus native guest bridge v1 (prototype)

This change is an initial implementation, not the complete ADB replacement plan.
It must not be called working in Android until the actual guest gate passes.

## Implemented code

- Static x86-64 guest daemon, installed in a copied Android-x86 ramdisk, launched
  by Android init as a privileged fixed-command service. SELinux is not disabled.
  The current recipe requires a userdebug image whose policy permits the `su`
  domain; other images fail the test rather than being silently weakened.
- Virtio-serial port `org.nexus.control.0`, resolved by its sysfs port name.
- Host Qt local-socket client using QEMU's Windows named-pipe chardev backend.
  Random per-launch pipe name, no guest TCP debugging port for these controls.
  The QEMU backend prefixes `\\.\pipe\` itself: only a basename is passed.
- Exact version/capability greeting, boot-status heartbeat, bounded messages,
  fixed navigation/volume/shutdown commands, diagnostic Home check, timeout and
  disconnect input release. No generic guest shell command API or file paths.
- Player opts in only for `images/nexus-bridge.txt` containing `1`. Legacy
  images keep their existing behavior. Native controls are preferred; legacy
  ADB controls remain a fallback for images without a working bridge.

Guest input/control currently invokes fixed Android platform utilities with
fixed arguments. This removes ADB transport, not Android command startup costs;
it is not yet an in-process Android framework input service. Readiness currently
uses the platform getprop utility, not a newly implemented property-area reader.
The privileged service and local-pipe ACL behavior need security review before
production distribution. Host and guest are mutually trusted by this prototype.

## Provisioning versus everyday controls

The new clean CI base has a `/data/.nexus-provision` marker. Only while that marker
exists does a one-shot provisioning service enable TCP adbd for the CI setup
steps. Branding installation removes it before the base is flattened. The
second boot has **no NIC and no ADB invocation**: it must report native boot
readiness, verify saved setup/Home/no recorded ANR, execute Home and shut down
cleanly through the virtio bridge. The original downloaded ISO remains unchanged;
ramdisk original/modified hashes and original source ramdisk are retained.
This must never be applied to a personal existing instance.

## Not implemented in v1

Foreground-app events (per-app keymap detection still uses optional ADB), APK
streaming/package-installer integration, display geometry/cursor calibration,
RAM cleanup integration, or engine presentation/FPS instrumentation. Current
FPS remains explicitly labelled ADB display/compositor telemetry. No promise of
BlueStacks parity, an ANR fix, stable hardware rendering or faster CPU execution.

## Tests and release gates

Native C parser tests exercise allowed/unknown versions, command injection,
oversized frames and invalid bytes with privileged execution stubbed. CPIO tests
check preservation of existing records and boot integration. Windows Qt tests
exercise handshake, send allowlist and protocol rejection using local sockets.
Those tests do not prove the guest daemon starts or its policy is correct.
The Android image workflow independently requires actual no-ADB guest replies
and a clean engine exit before publishing a matched candidate. Source/runtime
build success alone must not bypass that gate.
