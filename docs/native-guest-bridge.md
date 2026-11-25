# Nexus native guest bridge v1 (prototype)

This change is an initial implementation, not the complete ADB replacement plan.
Real Android guest integration passed Linux TCG gates in run 45648355363
(commit c032908); Windows end-to-end transport and acceleration remain unverified.

## Implemented code

- Static x86-64 guest daemon, installed through recognized Android 11 system init files (or a copied
  Android-x86 ramdisk for supported layouts), launched
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

The new clean CI base has a `/data/.nexus-provision` marker. A fixed
`PROVISION_HOME` command only works while this marker exists; it persists setup,
selects a supported installed launcher, disables the empty template's swipe lock,
and removes the marker. It does not accept arbitrary shell text or paths.
Both first provisioning and second persistence boots now run **without a NIC
or ADB**. They require native readiness, launcher/SDK 30/no-ANR checks, Home and
clean shutdown. The branding asset is installed offline in the copied system.
Run 45648355363 passed both boots and packaging. First native boot-ready time
was 314.59 seconds, second boot 157.8 seconds, under Linux TCG, 2 vCPUs / 3072 MiB.
Both confirmed SDK 30, resumed Home, no recorded ANR, completed native Home and
clean shutdown. These are functional checks, not smoothness/FPS measurements.
Fresh settings initialization preconfirms the standard immersive tutorial to
avoid its measured focus-event ANR; no ANR records or dialogs are suppressed.

Guest artifact: `15040221057` (Nexus-Android11-FOSS-Candidate).
Matching Windows host test/build run: `45617255804`, Player artifact `15023935349`.
Use a separate new instance; never replace an existing backing base in place.
Drive transfer has not completed: workspace uploads are capped at 100 MB, the
artifact is about 2.1 GB, and GitHub integration cannot write the temporary upload
session secret (HTTP 403). No Drive file content has been uploaded.

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
