#pragma once
#include <QObject>
#include <QLocalSocket>
#include <QTimer>
#include <QElapsedTimer>
#include <QStringList>
#include <functional>

// Private QEMU named-pipe backend for virtio-serial. No guest TCP/ADB transport.
class NativeGuestBridge : public QObject {
public:
    explicit NativeGuestBridge(QObject *parent) : QObject(parent), socket(this), retry(this) {
        retry.setInterval(1000);
        connect(&retry, &QTimer::timeout, this, [this] {
            if (online && heartbeat.isValid() && heartbeat.elapsed() > 20000) socket.abort();
            if (!pipe.isEmpty() && socket.state() == QLocalSocket::UnconnectedState) socket.connectToServer(pipe);
        });
        connect(&socket, &QLocalSocket::readyRead, this, [this] {
            buffer += socket.readAll();
            if (buffer.size() > 4096) { socket.abort(); return; }
            while (buffer.contains('\n')) {
                const auto index = buffer.indexOf('\n');
                const QByteArray line = buffer.left(index); buffer.remove(0,index+1);
                if (line.size() > 127) { socket.abort(); return; }
                if (line == "NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK") {
                    online = true; heartbeat.restart();
                    if (diagnostic) diagnostic("Native guest bridge v1 handshake received.");
                } else if (online && (line == "NEXUS 1 READY 0" || line == "NEXUS 1 READY 1")) {
                    heartbeat.restart();
                    if (bootStatus) bootStatus(line.endsWith('1'));
                } else if (online && (line.startsWith("NEXUS 1 RESULT ") || line.startsWith("NEXUS 1 ERROR "))) {
                    if (diagnostic) diagnostic(QString::fromLatin1(line));
                } else { if (diagnostic) diagnostic("Unsupported guest bridge message/version."); socket.abort(); return; }
            }
        });
        connect(&socket, &QLocalSocket::disconnected, this, [this] {
            online = false; buffer.clear();
            if (disconnected) disconnected();
        });
    }
    ~NativeGuestBridge() override { bootStatus={}; disconnected={}; diagnostic={}; stop(); }
    void start(const QString &name) { stop(); pipe=name; retry.start(); socket.connectToServer(pipe); }
    void stop() { pipe.clear(); retry.stop(); socket.abort(); online=false; buffer.clear(); }
    bool isOnline() const { return online && socket.state()==QLocalSocket::ConnectedState; }
    bool send(const QString &command) {
        static const QStringList allowed{"BACK","HOME","RECENTS","VOLUME_UP","VOLUME_DOWN","MUTE","SHUTDOWN"};
        if (!isOnline() || !allowed.contains(command) || socket.bytesToWrite() > 1024) return false;
        const QByteArray packet = "NEXUS 1 "+command.toLatin1()+'\n';
        return socket.write(packet)==packet.size(); // Acceptance != completed guest action.
    }
    std::function<void(bool)> bootStatus;
    std::function<void()> disconnected;
    std::function<void(const QString &)> diagnostic;
private:
    QLocalSocket socket;
    QTimer retry;
    QElapsedTimer heartbeat;
    QByteArray buffer;
    QString pipe;
    bool online = false;
};
