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
