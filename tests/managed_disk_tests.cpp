#include "manageddisk.h"
#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QTemporaryDir>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    const QString img = qEnvironmentVariable("QEMU_IMG");
    QTemporaryDir tmp; if (img.isEmpty() || !tmp.isValid()) return 1;
    const QString base = tmp.filePath("base image.qcow2"), overlay = tmp.filePath("user data/overlay.qcow2");
    if (QProcess::execute(img, {"create", "-f", "qcow2", base, "16M"}) != 0) return 2;
    QString error;
    if (!prepareManagedDisk(img, base, overlay, &error)) { std::cerr << error.toStdString() << '\n'; return 3; }
    if (!QFile::exists(overlay) || !QFile::exists(overlay + ".json") || !prepareManagedDisk(img, base, overlay, &error)) { std::cerr << error.toStdString() << '\n'; return 4; }
    const qint64 overlaySize = QFileInfo(overlay).size();
    if (QProcess::execute(img, {"resize", base, "32M"}) != 0) return 5;
    if (prepareManagedDisk(img, base, overlay, &error)) return 6;
    if (!QFile::exists(overlay) || QFileInfo(overlay).size() != overlaySize) return 7;
    std::cout << "Managed overlay creation, reuse and base-change protection passed.\n";
    return 0;
}
