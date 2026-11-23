#include "imagedownload.h"
#include <QFileInfo>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QTimer>

ImageDownload::ImageDownload(QObject *parent) : QObject(parent) {}

void ImageDownload::start(const QUrl &url, const QString &destination, const QByteArray &sha256Hex) {
    if (started) return;
    started = true;
    expected = sha256Hex.toLower();
    if (QFileInfo::exists(destination)) {
        QTimer::singleShot(0, this, [this] { emit finished(false, "Destination already exists. Choose a new file name."); });
        return;
    }
    file.setFileName(destination);
    file.setDirectWriteFallback(false);
    if (!file.open(QIODevice::WriteOnly)) {
        const QString error = file.errorString();
        QTimer::singleShot(0, this, [this, error] { emit finished(false, error); });
        return;
    }
    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::NoLessSafeRedirectPolicy);
    reply = network.get(request);
    connect(reply, &QNetworkReply::readyRead, this, &ImageDownload::consume);
    connect(reply, &QNetworkReply::downloadProgress, this, &ImageDownload::progress);
    connect(reply, &QNetworkReply::finished, this, &ImageDownload::complete);
}

void ImageDownload::cancel() {
    cancelled = true;
    if (reply) reply->abort();
}

void ImageDownload::consume() {
    const QByteArray bytes = reply->readAll();
    if (bytes.isEmpty() || !writeError.isEmpty()) return;
    if (file.write(bytes) != bytes.size()) {
        writeError = file.errorString();
        reply->abort();
        return;
    }
    hash.addData(bytes);
}

void ImageDownload::complete() {
    consume();
    QString error;
    if (cancelled) error = "Download cancelled.";
    else if (!writeError.isEmpty()) error = "Could not write image: " + writeError;
    else if (reply->error() != QNetworkReply::NoError) error = "Download failed: " + reply->errorString();
    else if (hash.result().toHex() != expected) error = "SHA-256 mismatch. The downloaded image was discarded.";
    else if (QFileInfo::exists(file.fileName())) error = "Destination appeared during download; refusing to overwrite it.";
    if (!error.isEmpty()) {
        file.cancelWriting();
        emit finished(false, error);
        return;
    }
    if (!file.commit()) {
        emit finished(false, "Could not save image: " + file.errorString());
        return;
    }
    emit finished(true, file.fileName());
}
