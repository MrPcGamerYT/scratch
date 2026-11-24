#pragma once

#include <QObject>
#include <QCryptographicHash>
#include <QNetworkAccessManager>
#include <QSaveFile>
#include <QUrl>

class QNetworkReply;

// Streams into an atomic temporary file; the destination is committed only after
// the expected SHA-256 has been verified. One instance handles one download.
class ImageDownload : public QObject {
    Q_OBJECT
public:
    explicit ImageDownload(QObject *parent = nullptr);
    void start(const QUrl &url, const QString &destination, const QByteArray &sha256Hex);
    void cancel();

signals:
    void progress(qint64 received, qint64 total);
    void finished(bool success, const QString &message);

private:
    void consume();
    void complete();
    QNetworkAccessManager network;
    QNetworkReply *reply = nullptr;
    QSaveFile file;
    QCryptographicHash hash{QCryptographicHash::Sha256};
    QByteArray expected;
    QString writeError;
    bool cancelled = false;
    bool started = false;
};
