#!/system/bin/sh
# Disposable native-engine diagnostic image only. Never certifies usable Home.
exec >/dev/ttyS0 2>&1
echo NEXUS_HOME_CANDIDATES_BEGIN
candidates=$(timeout 10 cmd package query-activities --brief --components --user 0 -a android.intent.action.MAIN -c android.intent.category.HOME)
printf '%s\n' "$candidates"
echo NEXUS_HOME_CANDIDATES_END
# Use only a component returned by PackageManager, not an invented package name.
component=$(printf '%s\n' "$candidates" | grep -E '^[A-Za-z0-9_.]+/[A-Za-z0-9_.$]+$' | grep -v '^android/' | head -n 1)
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
sleep 10
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
