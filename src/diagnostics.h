#pragma once
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QSettings>
#include <QStandardPaths>
#include <QStringList>
#include <QSysInfo>
#include "runtimeconfig.h"
#include "resourceconfig.h"

// Read-only package checks. Presence is not a boot/guest capability test.
inline QString playerDiagnostics(const QString &root) {
    QStringList report{"Project Nexus diagnostics", "Generated: " + QDateTime::currentDateTimeUtc().toString(Qt::ISODate),
        "Host: " + QSysInfo::prettyProductName() + " / " + QSysInfo::currentCpuArchitecture(),
        "Player folder: " + root, "", "Package checks (non-empty files only):"};
    for (const auto &relative : {"qemu/bin/NexusEngine.exe", "qemu/bin/qemu-img.exe", "qemu/share/qemu/bios-256k.bin", "qemu/share/qemu/vgabios-stdvga.bin", "qemu/share/qemu/vgabios-virtio.bin", "qemu/share/qemu/kvmvapic.bin", "images/android-base.qcow2"}) {
        const QFileInfo file(QDir(root).filePath(relative));
        report << QString(file.isFile() && file.size() > 0 ? "OK      " : "MISSING/EMPTY ") + relative;
    }
    const QFileInfo code(root + "/images/android-uefi-code.fd"), vars(root + "/images/android-uefi-vars.fd");
    const bool hasCode = code.isFile() && code.size() > 0, hasVars = vars.isFile() && vars.size() > 0;
    report << (hasCode && hasVars ? "UEFI: code and variables present" : (!code.exists() && !vars.exists() ? "Firmware: legacy BIOS (no prepared UEFI files)" : "WARNING: incomplete/empty UEFI firmware"));
    const auto adb = root + "/platform-tools/adb.exe";
    report << ((QFileInfo(adb).isFile() && QFileInfo(adb).size() > 0) || !QStandardPaths::findExecutable("adb.exe").isEmpty() ? "ADB executable found (guest connection not tested)" : "ADB missing: navigation, volume, APK and display changes require platform-tools");
    QSettings settings;
    const auto resources = selectedResources(settings);
    report << "" << "Configured CPU engine: " + selectedVmAcceleration(settings)
        << QString("Next-launch resources: %1 CPU(s), %2 MB RAM (%3)").arg(resources.cpus).arg(resources.memory_mb).arg(automaticResources(settings) ? "automatic" : "manual")
        << QString("Configured local ADB forwarding: %1 (restart required after changes)").arg(settings.value("adbForward", false).toBool() ? "enabled" : "disabled")
        << QString("Audio output: automatic Intel HDA / Windows DirectSound (guest driver required)")
        << "Network: QEMU user-mode NAT with virtio-net (guest driver/DHCP required; not a connectivity test)"
        << "Configured display: " + selectedVmGraphics(settings) + " (configuration only; actual guest rendering not verified)"
        << "Rust: runtime launch policy and input core; QEMU remains the VM engine"
        << "" << "These checks do not verify ISO integrity, Android boot, ADB authorization or game compatibility."
        << "Privacy: this report and session log may contain local paths/usernames. Review before sharing.";
    return report.join('\n');
}
