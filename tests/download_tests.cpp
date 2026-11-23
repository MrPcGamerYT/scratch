#include "imagedownload.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QEventLoop>
#include <QFile>
#include <QHostAddress>
#include <QTemporaryDir>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTimer>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QTemporaryDir dir;
    if (!dir.isValid()) return 1;
    QTcpServer server;
    if (!server.listen(QHostAddress::LocalHost)) return 2;
    const QByteArray data("stock-image-test-data");
    QObject::connect(&server, &QTcpServer::newConnection, &server, [&server, data] {
        while (auto *socket = server.nextPendingConnection()) {
            QObject::connect(socket, &QTcpSocket::readyRead, socket, [socket, data] {
                socket->readAll();
                socket->write("HTTP/1.1 200 OK\r\nContent-Length: " + QByteArray::number(data.size()) + "\r\nConnection: close\r\n\r\n" + data);
                socket->disconnectFromHost();
            });
            QObject::connect(socket, &QTcpSocket::disconnected, socket, &QObject::deleteLater);
        }
    });
    const QUrl url(QString("http://127.0.0.1:%1/image.iso").arg(server.serverPort()));
    const QByteArray goodHash = QCryptographicHash::hash(data, QCryptographicHash::Sha256).toHex();
    auto run = [&](const QString &path, const QByteArray &hash, bool expectedSuccess) {
        ImageDownload download;
        QEventLoop loop;
        QTimer timer;
        timer.setSingleShot(true);
        bool completed = false, success = false;
        QObject::connect(&download, &ImageDownload::finished, &loop, [&](bool ok, const QString &) { completed = true; success = ok; loop.quit(); });
        QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
        timer.start(10000);
        download.start(url, path, hash);
        loop.exec();
        return completed && success == expectedSuccess;
    };
    const QString good = dir.filePath("good.iso");
    const QString bad = dir.filePath("bad.iso");
    if (!run(good, goodHash, true)) { std::cerr << "Valid download failed\n"; return 3; }
    QFile file(good);
    if (!file.open(QIODevice::ReadOnly) || file.readAll() != data) return 4;
    file.close();
    if (!run(bad, QByteArray(64, '0'), false) || QFile::exists(bad)) { std::cerr << "Bad hash was not rejected\n"; return 5; }
    if (!run(good, goodHash, false)) { std::cerr << "Existing file was not rejected\n"; return 6; }
    if (!file.open(QIODevice::ReadOnly) || file.readAll() != data) return 7;
    std::cout << "Verified successful download, checksum rejection, and existing-file protection.\n";
    return 0;
}
