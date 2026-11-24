#pragma once
#include <QFileInfo>
#include <QMimeData>
#include <QUrl>

// Accept exactly one existing local APK, never remote URLs or directories.
inline QString droppedApk(const QMimeData *mime) {
    if (!mime || !mime->hasUrls() || mime->urls().size() != 1) return {};
    const auto url = mime->urls().first();
    if (!url.isLocalFile()) return {};
    const QFileInfo file(url.toLocalFile());
    if (!file.isFile() || file.size() <= 0 || file.suffix().compare("apk", Qt::CaseInsensitive) != 0) return {};
    return file.absoluteFilePath();
}
