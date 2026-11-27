# New-engine device integration — status and first verified transport

The user explicitly requested starting milestone 4 while accelerated graphics is
still unfinished. This work does **not** mark graphics or device integration done.
Networking, sound and real Android package installation are not yet implemented
end-to-end on the independent engine.

## New native bridge transport

The Rust/WHP platform now has an **opt-in private COM2 UART** at ports 0x2f8–0x2ff,
IRQ3. COM1 remains the separate boot log. No physical host serial port, named pipe,
TCP listener, ADB process or QEMU is used by this path.

This is an initial integration transport, not a high-throughput APK-performance
claim. The UART model has immediate TX, a one-byte receiver and IRQ delivery; it
does not emulate baud timing, DMA or FIFOs.

- `--bridge-probe` creates the private device for a boot command. Without it, the
  previous platform remains unchanged; this is not a final player settings toggle.
- The bounded Rust parser requires the exact existing v1 guest greeting/capabilities
  before sending the single fixed `QUERY_APP` command. It validates package tokens,
  response ordering, frame length, bytes and aggregate receive limits.
- RX backpressure leaves the next host byte queued until the UART receiver is empty.
  PIC IRQ3 masking, edge behavior and priority against IRQ0/IRQ4 are tested.
- Success requires an APP reply plus `RESULT QUERY_APP OK`, not merely a greeting,
  heartbeat, guest boot log or open file. The probe stops at that checkpoint.
- This probe exposes no arbitrary shell execution, file path API or APK install
  command. It is not yet the regular interactive-player bridge/control service.

### Actual guest verification

Linux regression **45793114697** and later **45793593650** pass three independent
WHP boots each. A dependency-free diagnostic Linux PID1:

1. Opens the real `/dev/ttyS1` character device (major 4, minor 65).
2. Configures raw serial input/output with termios.
3. Sends the greeting to the host through the guest UART driver.
4. Reads and verifies **every byte** of the host's fixed query.
5. Emits a COM1 marker only after the exact match, then sends the query response
   over COM2. The Rust engine requires its own correctly parsed response and IRQ3
   injections before success.

The diagnostic package name is a fixture. This is **not Android app discovery**.
Neither Home timing nor Android readiness fields are populated. The same Linux
runs retain the existing display and raw/ext4 storage/persistence gates.
Evidence: `native-device-checkpoint.json`, `device-bridge-*.log`, timing JSON and
the generated initramfs in the native Linux workflow artifact.

## Guest daemon preparation and tests

The existing guest daemon now supports an explicit `NEXUS_BRIDGE_COM2` build.
It opens only `/dev/ttyS1`, configures raw 8-bit serial mode and uses its existing
bounded framing/worker implementation. It never silently falls back to COM1,
VirtIO, ADB or a network listener. The default legacy VirtIO build is unchanged.

A compile-time-only `NEXUS_AGENT_COM2_TEST` variant allows an explicit PTY pathname
for tests; distributed COM2 builds keep the fixed guest pathname. The production
COM2 open/termios/serve path is tested through a real Linux PTY with sandbox utility
stubs, checking greeting, query, response and lack of terminal echo. This validates
host-process behavior, not Android init or SELinux permissions.

Local tests passed:

- Five fixture/archive structural tests.
- Four daemon parser/build tests, including COM2 production compilation.
- Two daemon integration tests: production COM2/PTY query plus existing asynchronous
  worker heartbeat/binary APK framing/integrity/disconnect-cleanup harness.

The APK harness uses sandbox platform-utility stubs. **It is not an Android package
installation, signature-validation or new-engine APK-transfer acceptance test.**
Foundation **45793808528** passed these daemon tests on Linux and Rust
tests/strict Clippy on both hosts. Final Linux regression **45793865474** passed
with a freshly rebuilt pinned kernel, including the five fixture tests, three
private-bridge boots, software display and persistence gates. Original Android
second-stage-init regression **45793599261** passed after the device changes;
that run does not provision or verify the new Android bridge daemon.

## Required remaining gates

| Feature | Actual remaining implementation/verification |
| --- | --- |
| Native Android bridge | Provision the explicit daemon in a new derived image, keep original assets/user disks intact, configure a reviewed init service and SELinux policy, verify real Android replies, reconnect, bounded timeouts and clean shutdown. |
| APK installation | Implement the Rust host streaming installer and player action; verify exclusive guest staging, SHA-256, normal Android package/signature checks, partial-transfer cleanup, and a real signed benign APK install/query/uninstall through this engine. |
| Networking | Implement a guest NIC/driver transport and safe host backend; verify guest addressing, DNS and actual bidirectional TCP/UDP/internet operation, disconnect and reset. No NIC or host-network path exists yet on this engine. |
| Audio | Implement a matching guest audio device/HAL and host output path with bounded buffers/clocking; verify actual guest-generated output, underrun/reset and host device changes. No guest sound output exists yet on this engine. |

Android framework/package-manager verification remains blocked by the incomplete
Android userspace/display boot. Networking and audio also require their own
independent devices/backends; they do not become working by fixing graphics.
No performance, working internet/sound, APK delivery, or finished-player release
is claimed by this increment.

## Real Android APK acceptance and initial freeze diagnosis

Native workflow **45804869216** (b949954) and repeated run **45806838925**
(019ca9b) passed on Windows/WHP with the original Android 11 kernel and a
new derived guest image/disposable COW. The host streamed a signed benign
`org.nexus.smoke` APK over COM2; Android PackageManager installed it, the guest
verified package presence, uninstalled it and confirmed package absence.
Both runs report `NATIVE_BRIDGE_APK_INSTALL_VERIFY_UNINSTALL_OK`.
The first real attempt exposed Android 11 rejecting a trailing `-` in the
stdin installer arguments; removing that argument, rebuilding the embedded
daemon and rerunning actual Android resolved that installer failure.
This supersedes the earlier table's unimplemented host-streaming/real-APK gate;
the integrated player action, broader bridge lifecycle, networking and audio
remain incomplete. These runs use HWACCEL=0 and a software scanout, not a
verified accelerated guest GPU or a usable-Home/performance certification.

Step 5 began with a bounded Home launch, last-ANR/event capture and read-only
`pm list packages -f` inventory in run **45806838925**. Last-ANR explicitly
reported **no ANR since boot** before and after the short observation window.
However Home resolved to `android/com.android.internal.app.ResolverActivity`,
not verified usable Launcher3; events recorded repeated native Launcher3
crashes. The captured RenderThread SIGABRT is:

```
Abort message: 'Impossible totalDuration 0'
libhwui.so android::uirenderer::JankTracker::finishFrame
libhwui.so android::uirenderer::renderthread::CanvasContext::draw
```

This establishes a specific launcher rendering/timing crash, **not** a
reproduction or fix of the user's earlier Not responding failures. Investigate
guest clocksource/timestamp resolution and frame timing before changing Android
libraries or disabling services. Permissive SELinux audit messages alone do
not establish access denial as the cause. No packages have been removed or
disabled. The package inventory is evidence for dependency review only;
retain core services, both editor workflows, and Home pending regression tests.
Workflow artifacts retain complete serial/timing/JSON evidence for three days.
