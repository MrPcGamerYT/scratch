#pragma once
#include <QString>
#include <QRegularExpression>
#include <QSet>

// Parse only Android's explicit focused/resumed activity, never background packages.
inline QString foregroundPackage(const QString &text) {
    const QRegularExpression pattern("(?:mResumedActivity|topResumedActivity|mCurrentFocus)[^\\n]*?\\b([A-Za-z][A-Za-z0-9_]*(?:\\.[A-Za-z0-9_]+)+)/[A-Za-z0-9_.$]+");
    const auto match = pattern.match(text);
    return match.hasMatch() ? match.captured(1) : QString();
}

// Default SurfaceFlinger latency is compositor/display data, NOT game FPS.
// Require a recent timestamp window in the guest monotonic time domain.
inline QString compositorFps(const QString &text) {
    const auto sections = text.split("NEXUS_LATENCY\n");
    if (sections.size() != 2) return "Display FPS: N/A";
    bool ok = false;
    const double uptime = sections[0].trimmed().section(' ', 0, 0).toDouble(&ok);
    if (!ok || uptime <= 0) return "Display FPS: N/A";
    QSet<qint64> times;
    const qint64 now = static_cast<qint64>(uptime * 1000000000.0);
    for (const auto &line : sections[1].split('\n')) {
        const auto fields = line.simplified().split(' ');
        if (fields.size() != 3) continue;
        const auto present = fields[1].toLongLong(&ok);
        if (ok && present > 0 && present <= now && present > now - 1000000000LL) times.insert(present);
    }
    if (times.isEmpty()) return "Display FPS: N/A";
    return QString("Display FPS: %1").arg(times.size());
}

// Bound failed/offline telemetry retries instead of spawning adb every tick.
inline int telemetryRetryDelayMs(int failures) {
    if (failures <= 0) return 0;
    return failures >= 4 ? 30000 : (2000 << failures);
}

inline QString guestTelemetryCommand(bool fps) {
    QString command = "boot=$(getprop sys.boot_completed); printf '%s\\n' \"$boot\"; echo NEXUS_ACTIVITY; "
        "if [ \"$boot\" = 1 ]; then dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity'; fi; echo NEXUS_FPS; ";
    if (fps) command += "if [ \"$boot\" = 1 ]; then cat /proc/uptime; echo NEXUS_LATENCY; dumpsys SurfaceFlinger --latency; fi; ";
    return command + "exit 0";
}
