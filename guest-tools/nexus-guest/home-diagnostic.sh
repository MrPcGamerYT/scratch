#!/system/bin/sh
# Disposable native-engine diagnostic image only. Never certifies usable Home.
exec >/dev/ttyS0 2>&1
echo NEXUS_HOME_CANDIDATES_BEGIN
candidates=$(timeout 10 cmd package query-activities --brief --components --user 0 -a android.intent.action.MAIN -c android.intent.category.HOME)
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
sleep 3
before=$(pidof "${component%%/*}")
echo "NEXUS_HOME_PID_BEFORE $before"
echo NEXUS_HOME_RECENTS_BEGIN
if timeout 10 input keyevent 187; then echo NEXUS_HOME_RECENTS_KEY_SENT; fi
sleep 2
timeout 10 dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity|ResumedActivity'
echo NEXUS_HOME_RECENTS_END
if timeout 10 input keyevent 3; then echo NEXUS_HOME_HOME_KEY_SENT; fi
sleep 8
after=$(pidof "${component%%/*}")
echo "NEXUS_HOME_PID_AFTER $after"
if [ -n "$before" ] && [ "$before" = "$after" ]; then echo NEXUS_HOME_PROCESS_RETAINED; fi
echo NEXUS_HOME_FRAME_STATS_BEGIN
timeout 10 dumpsys gfxinfo "${component%%/*}" | head -n 65
echo NEXUS_HOME_FRAME_STATS_END
echo NEXUS_HOME_ACTIVITY_STATE_BEGIN
timeout 10 dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity|ResumedActivity|Hist #0|mFocusedApp'
echo NEXUS_HOME_ACTIVITY_STATE_END
echo NEXUS_HOME_WINDOW_STATE_BEGIN
timeout 10 dumpsys window windows | grep -E 'mCurrentFocus|mFocusedApp|mHasSurface|isOnScreen|isVisible|mObscuringWindow'
echo NEXUS_HOME_WINDOW_STATE_END
echo NEXUS_HOME_CRASH_LOG_BEGIN
timeout 10 logcat -b crash -d -t 150 -v threadtime
echo NEXUS_HOME_CRASH_LOG_END
# Captured evidence is diagnostic, not a success marker or an ANR remediation.
exit 0
