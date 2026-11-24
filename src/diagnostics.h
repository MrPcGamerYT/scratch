#pragma once
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QSettings>
#include <QStandardPaths>
#include <QStringList>
#include <QSysInfo>
#include "runtimeconfig.h"

// Read-only package checks. Presence is not a boot/guest capability test.
inline QString playerDiagnostics(const QString &root) {
    QStringList report{"Project Nexus diagnostics", "Generated: " + QDateTime::currentDateTimeUtc().toString(Qt::ISODate),
        "Host: " + QSysInfo::prettyProductName() + " / " + QSysInfo::currentCpuArchitecture(),
        "Player folder: " + root, "", "Package checks (non-empty files only):"};
    for (const auto &relative : {"qemu/bin/qemu-system-x86_64.exe", "qemu/bin/qemu-img.exe", "qemu/share/qemu/bios-256k.bin", "qemu/share/qemu/vgabios-stdvga.bin", "qemu/share/qemu/kvmvapic.bin", "images/android-base.qcow2"}) {
        const QFileInfo file(QDir(root).filePath(relative));
        report << QString(file.isFile() && file.size() > 0 ? "OK      " : "MISSING/EMPTY ") + relative;
    }
    const QFileInfo code(root + "/images/android-uefi-code.fd"), vars(root + "/images/android-uefi-vars.fd");
    const bool hasCode = code.isFile() && code.size() > 0, hasVars = vars.isFile() && vars.size() > 0;
    report << (hasCode && hasVars ? "UEFI: code and variables present" : (!code.exists() && !vars.exists() ? "Firmware: legacy BIOS (no prepared UEFI files)" : "WARNING: incomplete/empty UEFI firmware"));
    const auto adb = root + "/platform-tools/adb.exe";
    report << ((QFileInfo(adb).isFile() && QFileInfo(adb).size() > 0) || !QStandardPaths::findExecutable("adb.exe").isEmpty() ? "ADB executable found (guest connection not tested)" : "ADB missing: navigation, volume, APK and display changes require platform-tools");
    QSettings settings;
    report << "" << "Configured CPU engine: " + selectedVmAcceleration(settings)
        << QString("Configured resources: %1 CPU(s), %2 MB RAM").arg(qBound(1, settings.value("cpus", 4).toInt(), 64)).arg(qBound(512, settings.value("memory", 4096).toInt(), 131072))
        << QString("Configured local ADB forwarding: %1 (restart required after changes)").arg(settings.value("adbForward", false).toBool() ? "enabled" : "disabled")
        << "Graphics: QEMU standard VGA; no accelerated GPU backend"
        << "Rust: runtime launch policy and input core; QEMU remains the VM engine"
        << "" << "These checks do not verify ISO integrity, Android boot, ADB authorization or game compatibility."
        << "Privacy: this report and session log may contain local paths/usernames. Review before sharing.";
    return report.join('\n');
}
