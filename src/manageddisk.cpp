#include "manageddisk.h"
#include <QDir>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLockFile>
#include <QProcess>
#include <QSaveFile>

static bool runImg(const QString &program, const QStringList &args, QByteArray *out, QString *error) {
    QProcess process; process.start(program, args);
    if (!process.waitForStarted(5000) || !process.waitForFinished(30000)) {
        if (process.state() != QProcess::NotRunning) { process.kill(); process.waitForFinished(); }
        *error = "qemu-img did not complete: " + process.errorString(); return false;
    }
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0) {
        *error = QString::fromLocal8Bit(process.readAllStandardError()).trimmed();
        if (error->isEmpty()) *error = "qemu-img returned an error.";
        return false;
    }
    *out = process.readAllStandardOutput(); return true;
}
static bool imageInfo(const QString &img, const QString &path, QJsonObject *info, QString *error) {
    QByteArray output;
    if (!runImg(img, {"info", "--output=json", path}, &output, error)) return false;
    QJsonParseError parse; const auto doc = QJsonDocument::fromJson(output, &parse);
    if (parse.error != QJsonParseError::NoError || !doc.isObject()) { *error = "Invalid qemu-img information."; return false; }
    *info = doc.object(); return true;
}
bool prepareManagedDisk(const QString &qemuImg, const QString &base, const QString &overlay, QString *error) {
    const QFileInfo baseInfo(base); const QString canonicalBase = baseInfo.canonicalFilePath();
    if (canonicalBase.isEmpty() || !baseInfo.isFile() || !QFileInfo(qemuImg).isFile()) { *error = "Prepared Android image or qemu-img was not found."; return false; }
    if (!QDir().mkpath(QFileInfo(overlay).absolutePath())) { *error = "Could not create the user data folder."; return false; }
    QLockFile lock(overlay + ".lock");
    if (!lock.tryLock(0)) { *error = "Another instance is preparing this disk."; return false; }
    QJsonObject baseJson;
    if (!imageInfo(qemuImg, canonicalBase, &baseJson, error)) return false;
    if (baseJson.value("format").toString() != "qcow2" || baseJson.contains("backing-filename")) { *error = "The prepared base must be a standalone QCOW2 image without a backing file."; return false; }
    const QJsonObject expected{{"base", canonicalBase}, {"size", QString::number(baseInfo.size())}, {"modified", QString::number(baseInfo.lastModified().toMSecsSinceEpoch())}};
    const QString metaPath = overlay + ".json";
    if (QFileInfo::exists(overlay) || QFileInfo::exists(metaPath)) {
        QFile meta(metaPath);
        if (!QFileInfo(overlay).isFile() || !meta.open(QIODevice::ReadOnly) || QJsonDocument::fromJson(meta.readAll()).object() != expected) {
            *error = "The Android base image changed or user disk metadata is missing. Your disk was preserved; restore the original base or back up and migrate the user disk."; return false;
        }
        QJsonObject info;
        if (!imageInfo(qemuImg, overlay, &info, error)) return false;
        QString backing = info.value("full-backing-filename").toString(info.value("backing-filename").toString());
        if (info.value("format").toString() != "qcow2" || QFileInfo(backing).canonicalFilePath().compare(canonicalBase, Qt::CaseInsensitive) != 0) {
            *error = "User disk backing file does not match the prepared image. No disk was changed."; return false;
        }
        return true;
    }
    QByteArray output;
    if (!runImg(qemuImg, {"create", "-f", "qcow2", "-F", "qcow2", "-b", canonicalBase, overlay}, &output, error)) return false;
    QSaveFile meta(metaPath);
    const auto bytes = QJsonDocument(expected).toJson();
    if (!meta.open(QIODevice::WriteOnly) || meta.write(bytes) != bytes.size() || !meta.commit()) {
        *error = "Could not save disk metadata. The new user disk was preserved; back it up before retrying."; return false;
    }
    return true;
}
