# Native integration follow-up — not yet a verified release

The last downloadable verified image remains c032908, run 45648355363.
The changes described here have only passed the local tests below. GitHub TLS
connections currently fail from this sandbox, preventing push/CI dispatch.
Do not describe the new host, guest or engine code as a finished binary release.

## Implemented for the next candidate

- Android operations run in a bounded child process group. The control loop
  continues boot-status heartbeats; a long platform command no longer blocks the
  host heartbeat. A disconnected client kills its worker and removes owned APK
  staging files. Completion is forwarded only after the worker is reaped.
- `QUERY_APP` returns a validated resumed package over VirtIO. Host per-app
  keymaps use this instead of ADB when advertised. This is polling Android's
  activity service, **not** a framework event subscription. Unknown, disconnected
  or >15-second-stale identities clear the active app/profile.
- APK transfer uses fixed BEGIN/DATA/COMMIT framing, a 512 MiB file limit,
  64 KiB binary chunks with acknowledgements, SHA-256 verification and a private
  exclusive/no-symlink staging file. There is no caller-supplied guest path or
  shell command. Android's regular `pm install -r --user 0` enforces package and
  signing rules; checksum matching is not proof an APK is trustworthy.
- The Player confirms installation with the user, computes hashes incrementally
  without loading the complete APK into RAM and reports actual command results.
  Connection loss/timeouts explicitly leave the installation outcome unknown.
- The engine FPS badge is wired to SDL 2D presentation-completion trace events
  over a random user-restricted local pipe. No guest/ADB telemetry, Qt repaint
  ticks, dirty-rectangle counts or refresh-rate guesses are used for that badge.
  An engine build marker gates this instrumentation. Unsupported/older engines
  and OpenGL paths show N/A. It measures engine presentation calls completed per
  second, **not** app-render FPS, monitor scanout, GPU throughput or game smoothness.
  This pipe must still be exercised with the real Windows engine before release.

## Local checks

- Production C daemon compiles with `-std=c11 -O2 -Wall -Wextra -Werror`.
- Three parser/CPIO tests pass, including package parsing and invalid APK headers.
- A production-code subprocess harness passes asynchronous-heartbeat, binary
  transfer (including NUL/LF/non-ASCII), corrupted-digest rejection, successful
  checksum/installer-stub invocation and partial-transfer disconnect cleanup.
- Existing three guest-builder and eight boot-probe tests pass.
- Updated Qt tests cover the new greeting, package callback, APK frames and
  presentation-event counting. **Not run locally:** Qt development files absent.

The subprocess harness compiles explicit temporary utility/staging paths into a
separate test executable. Distributed builds use only fixed Android paths. The
harness does not replace real package-install, SELinux or Windows testing.

## Required CI before delivery

1. Push the current branch once GitHub connectivity returns.
2. Run Emulator CI with a fresh engine build (do not reuse an old QEMU artifact
   for the presentation-counter test). Confirm Qt tests and SDL engine workload.
3. Run android11-candidate.yml. Its first boot checks Home and native package
   identity. The second disposable overlay installs a locally built/signed,
   code-free CI APK through VirtIO. This overlay is discarded: the packaged base
   must not contain the smoke-test package. Base hash must remain unchanged.
4. Inspect Home/app/install/shutdown results and record separately the newly
   added `home_verified_seconds` and `home_command_completed_seconds` timings.
5. Verify the new downloadable ZIP CRCs and every packaged SHA256 before linking
   it. The previous verification workflow pins the previous artifact on purpose;
   it must be updated to the actual new successful artifact, not guessed IDs.

Still outside these changes: Android display geometry/cursor calibration,
native memory-cleaner integration, framework push events, accelerated OpenGL
presentation counters, Windows end-to-end guest transport, WHPX/MMIO stability,
physical GPU/game/sound/network testing and the blocked large Drive transfer.
No new performance or hardware-compatibility claim is established here.
