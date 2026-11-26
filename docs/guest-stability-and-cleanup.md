# Guest stability before slimming

The user's previous guest/player can show apps as “Not responding” after boot.
The cause is **not established**. Boot completion, Home focus and absence of a
single ANR dialog are not proof that applications stay responsive. Do not hide
ANR dialogs, raise ANR timeouts, clear logs or force-stop system services to
make a test appear successful.

## Evidence now collected

The developer-only Android Home gate now collects a read-only `stability/`
snapshot on both Home success and Home failure. This captures last ANR,
activity state, CPU load, memory, pressure counters (where supported), disk
capacity, installed/disabled packages, services and bounded event/system/crash
log tails. Missing ANR/event evidence is inconclusive, not success. Historical
ANR events remain visible; no logs or app data are cleared. Captures can contain
private package names/log messages; review before sharing. Existing player
production controls do not acquire an ADB dependency.

Developer invocation against a disposable diagnostic guest:

```sh
python tests/android_stability.py --endpoint HOST:PORT --output NEW_PRIVATE_DIRECTORY
```

This does not enable guest debugging, change a guest, install software, collect
unbounded logs, or overwrite existing evidence. Commands have per-command
15-second timeouts. The snapshot can take several minutes if the guest is
unresponsive. `no_anr_in_capture` is a narrow evidence result, **not** a soak-test
or graphics-performance certification. CPU/IO/memory errors remain listed in
`failed_commands` even if ANR records are readable.

## Safe slimming policy (not yet applied)

1. Inventory packages and services on the exact candidate image first. Do not
   guess package IDs from another Android build.
2. Preserve the launcher/user-selected replacement, SystemUI, Settings,
   Package Installer, permission controller, providers, storage/downloads,
   WebView, input method, networking/audio, graphics services, native bridge,
   required app compatibility libraries and **the file editor**.
3. Review optional preinstalled games, demos, wallpapers and duplicate user
   utilities as candidates, not an automatic deletion list. Preserve upstream
   license notices. Obtain a concrete inventory before deciding removals.
4. Make changes on a disposable overlay/new image, never the user's installed
   disk. Record package/version, dependencies, rationale, original image hash
   and rollback path. Prefer reversible per-user disable for an experiment;
   it does not reclaim system-image space. Actual size reduction requires a
   rebuilt image and separate boot validation.
5. Compare baseline and lean candidate with identical CPU/RAM/GPU/storage.
   Validate two boots, Home, installing/opening/closing apps, browser/WebView,
   file editor, sound/network and sustained idle/interaction. Inspect ANR
   traces, event logs, storage headroom, CPU and memory pressure. Renderer or
   disk stalls must be fixed at their source, not attributed to “bloat” without
   evidence.
6. Measure process launch through usable Home and post-boot responsiveness
   separately. Approximately 100 seconds total Android boot remains a target,
   not a verified result for the independent engine.

## Remaining work

No packages have been removed by this change and no new guest image has been
published. A real affected-guest run and repeated app-interaction/soak tests are
still required. The independent Rust engine currently stops during early Linux
platform initialization; it cannot yet validate Android ANRs or the final
100-second boot target. Continue interrupt-controller/CPU capability work in
parallel, without claiming a completed Android emulator.


## Latest safeguards

The ADB CI Home gate now adds a 30-second bounded ANR/crash-log observation window
and captures input/compositor state. A historical `am_crash` event fails the
assessment; logs are never cleared to get a pass. This is not app interaction
or a long soak test. The native image Home-check recipe now bounds launch and
activity/ANR queries; rebuilding and validating a guest image is still needed.

Inventory-only cleanup review (no changes or executable removal plan):

```sh
python guest-tools/plan_cleanup.py --packages-json PRIVATE_CAPTURE/packages.json \
  --home-package ACTUAL.INSTALLED.HOME --file-editor-package ACTUAL.INSTALLED.EDITOR \
  --output NEW_REVIEW.json
```

Replace the two IDs with inventory-proven installed packages. The planner refuses
missing/invalid protected IDs and failed/unrecognized captures; it retains all
unreviewed packages. No space or boot-time savings are asserted. See
[engine-progress-report.md](engine-progress-report.md) for remaining work.
