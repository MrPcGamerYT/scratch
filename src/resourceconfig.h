#pragma once
#include "rustcore.h"
#include <QSettings>
#include <QThread>

inline bool automaticResources(const QSettings &settings) {
    // Existing installations with explicit allocations retain those values.
    return settings.value("automaticResources", !settings.contains("cpus") && !settings.contains("memory")).toBool();
}
inline NexusResourceBudget selectedResources(const QSettings &settings) {
    if (automaticResources(settings)) return nexus_resource_budget(qMax(1, QThread::idealThreadCount()), nexus_host_memory_mb());
    return {static_cast<std::uint32_t>(qBound(1, settings.value("cpus", 4).toInt(), 64)), static_cast<std::uint32_t>(qBound(512, settings.value("memory", 4096).toInt(), 131072))};
}
