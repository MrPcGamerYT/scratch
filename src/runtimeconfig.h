#pragma once
#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QString>

// Only two whitelisted values may affect launch policy; no file-supplied args.
inline QString selectedVmAcceleration(const QSettings &settings) {
    const auto override = settings.value("acceleration", "prepared").toString();
    if (override == "whpx" || override == "tcg") return override;
    QFile file(QCoreApplication::applicationDirPath() + "/images/acceleration.txt");
    if (file.open(QIODevice::ReadOnly) && file.read(16).trimmed() == "tcg") return "tcg";
    return "whpx";
}

// Whitelisted display device only; never load arbitrary QEMU arguments.
inline QString selectedVmGraphics(const QSettings &settings) {
    const auto override = settings.value("graphics", "prepared").toString();
    if (override == "std" || override == "virtio" || override == "virgl") return override;
    QFile file(QCoreApplication::applicationDirPath() + "/images/graphics.txt");
    if (file.open(QIODevice::ReadOnly) && file.read(16).trimmed() == "virtio") return "virtio";
    return "std";
}

// Fixed compatibility preset only: never accept CPU arguments from an image.
inline bool preparedReducedAvx() {
    QFile file(QCoreApplication::applicationDirPath() + "/images/cpu-profile.txt");
    return file.open(QIODevice::ReadOnly) && file.read(32).trimmed() == "reduced-avx";
}

// Presentation deadline only: never infer Android readiness from elapsed time.
inline bool bootDisplayWaitExpired(qint64 elapsedMs) {
    return elapsedMs >= 180000;
}
