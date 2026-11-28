#!/system/bin/sh
# Disposable native-engine diagnostic image only. Never certifies usable Home.
exec >/dev/ttyS0 2>&1
# Wait for Android boot to complete before querying PackageManager.
boot_ready=0
for attempt in 1 2 3 4; do
    if [ "$(getprop sys.boot_completed 2>/dev/null)" = "1" ]; then boot_ready=1; break; fi
    sleep 2
done
echo "NEXUS_HOME_BOOT_READY $boot_ready"
echo NEXUS_HOME_CANDIDATES_BEGIN
# The clean-launch flow sends CHECK_HOME immediately after boot_completed=1,
# with no prior package-manager operation. The APK flow's pm install/uninstall/
# disable triggers PackageManager to scan and register all apps; without that
# trigger, query-activities returns nothing. Force a scan with pm list packages,
# then retry query-activities until at least one HOME candidate is returned.
candidates=""
for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do
    pm list packages >/dev/null 2>&1
    candidates=$(timeout 15 cmd package query-activities --brief --components --user 0 -a android.intent.action.MAIN -c android.intent.category.HOME 2>/dev/null)
    if printf '%s\n' "$candidates" | grep -qE '^[A-Za-z0-9_.]+/[A-Za-z0-9_.$]+$'; then break; fi
    sleep 5
done
printf '%s\n' "$candidates"
echo NEXUS_HOME_CANDIDATES_END
# Use only a component returned by PackageManager, not an invented package name.
eligible=$(printf '%s\n' "$candidates" | grep -E '^[A-Za-z0-9_.]+/[A-Za-z0-9_.$]+$' | grep -v -E '^(android|com.android.provision|com.android.settings)/')
# PackageManager ordering is not a preference; provision/fallback aren't launchers.
component=$(printf '%s\n' "$eligible" | grep '^com.android.launcher3/' | head -n 1)
if [ -z "$component" ]; then component=$(printf '%s\n' "$eligible" | head -n 1); fi
if [ -n "$component" ]; then
    echo "NEXUS_HOME_SELECTED $component"
    if ! timeout 10 cmd package set-home-activity --user 0 "$component"; then
        echo NEXUS_HOME_DEFAULT_UNAVAILABLE
    fi
    echo NEXUS_HOME_EXPLICIT_LAUNCH_BEGIN
    if ! timeout 20 am start -W --user 0 -a android.intent.action.MAIN -c android.intent.category.HOME -n "$component"; then
        echo NEXUS_HOME_EXPLICIT_LAUNCH_FAILED
    fi
    echo NEXUS_HOME_EXPLICIT_LAUNCH_END
else
    echo NEXUS_HOME_NO_CANDIDATE
fi
package="${component%%/*}"
# Find the launcher PID via dumpsys activity processes (Android-native, reliable).
find_pid() {
    dumpsys activity processes 2>/dev/null | grep -F "$package" | awk '{print $1}' | head -n 1
}
# Short readiness wait: just 2 attempts, 1s each.
ready=0
for attempt in 1 2; do
    pid=$(find_pid)
    if [ -n "$pid" ]; then ready=1; break; fi
    sleep 1
done
echo "NEXUS_HOME_PROCESS_READY $ready"
before=$(find_pid)
echo "NEXUS_HOME_PID_BEFORE $before"
# Send keyevents with retries; use full path and capture failure reason.
send_keyevent() {
    local code="$1" label="$2"
    if timeout 8 /system/bin/input keyevent "$code" 2>&1; then
        echo "NEXUS_HOME_${label}_KEY_SENT"
    elif timeout 8 /system/bin/input keyevent "$code" 2>&1; then
        echo "NEXUS_HOME_${label}_KEY_SENT_RETRY"
    else
        echo "NEXUS_HOME_${label}_KEY_FAILED"
    fi
}
echo NEXUS_HOME_RECENTS_BEGIN
send_keyevent 187 RECENTS
sleep 1
timeout 8 dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity|ResumedActivity'
echo NEXUS_HOME_RECENTS_END
send_keyevent 3 HOME
sleep 2
after=$(find_pid)
echo "NEXUS_HOME_PID_AFTER $after"
if [ -n "$before" ] && [ -n "$after" ] && [ "$before" = "$after" ]; then echo NEXUS_HOME_PROCESS_RETAINED; fi
echo NEXUS_HOME_FRAME_STATS_BEGIN
timeout 8 dumpsys gfxinfo "$package" | head -n 40
echo NEXUS_HOME_FRAME_STATS_END
echo NEXUS_HOME_ACTIVITY_STATE_BEGIN
timeout 8 dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity|ResumedActivity|Hist #0|mFocusedApp'
echo NEXUS_HOME_ACTIVITY_STATE_END
echo NEXUS_HOME_WINDOW_STATE_BEGIN
window_info=$(timeout 8 dumpsys window windows 2>/dev/null)
printf '%s\n' "$window_info" | grep -iE 'mCurrentFocus|mFocusedApp|mHasSurface|isOnScreen|isVisible|mObscuringWindow|Window|package'
# Verify the launcher window is present in the window manager dump.
# The scanout evidence independently proves rendering; this confirms the
# launcher package has a registered window.
if printf '%s\n' "$window_info" | grep -q "$package"; then
    echo NEXUS_HOME_WINDOW_VISIBLE
fi
echo NEXUS_HOME_WINDOW_STATE_END
echo NEXUS_HOME_CRASH_LOG_BEGIN
timeout 8 logcat -b crash -d -t 150 -v threadtime
echo NEXUS_HOME_CRASH_LOG_END
echo NEXUS_HOME_ANR_BEGIN
if ! timeout 10 dumpsys activity lastanr; then echo NEXUS_HOME_ANR_UNAVAILABLE; fi
echo NEXUS_HOME_ANR_END
# Captured evidence is diagnostic, not a success marker or an ANR remediation.
exit 0
