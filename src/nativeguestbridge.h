#pragma once
#include <QObject>
#include <QLocalSocket>
#include <QTimer>
#include <QElapsedTimer>
#include <QStringList>
#include <QRegularExpression>
#include <QFile>
#include <QCryptographicHash>
#include <functional>

// Private QEMU named-pipe backend for virtio-serial. No guest TCP/ADB transport.
class NativeGuestBridge : public QObject {
public:
    explicit NativeGuestBridge(QObject *parent) : QObject(parent), apk(this), transferTimer(this), hashTimer(this), hash(QCryptographicHash::Sha256), socket(this), retry(this) {
        transferTimer.setSingleShot(true);
        connect(&transferTimer,&QTimer::timeout,this,[this] { if (installing) { finishInstall(false,"Native APK operation timed out; installation outcome may be unknown."); socket.abort(); } });
        hashTimer.setInterval(0);
        connect(&hashTimer,&QTimer::timeout,this,[this] {
            for (int i=0;i<8 && !apk.atEnd();++i) {
                const auto data=apk.read(65536);
                if (data.isEmpty()) { finishInstall(false,"Could not read APK."); return; }
                hash.addData(data);
            }
            if (!apk.atEnd()) return;
            hashTimer.stop();
            if (!apk.seek(0)) { finishInstall(false,"Could not rewind APK."); return; }
            const QByteArray begin="NEXUS 1 APK BEGIN "+QByteArray::number(apk.size())+' '+hash.result().toHex()+'\n';
            if (socket.write(begin)!=begin.size()) { finishInstall(false,"Could not begin APK transfer."); socket.abort(); }
        });
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
                if (line == "NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK" ||
                    line == "NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP" ||
                    line == "NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK") {
                    appEvents = line.contains(" APP");
                    apkSupported = line.endsWith(" APK");
                    online = true; heartbeat.restart();
                    if (diagnostic) diagnostic("Native guest bridge v1 handshake received.");
                } else if (online && (line == "NEXUS 1 READY 0" || line == "NEXUS 1 READY 1")) {
                    heartbeat.restart();
                    if (bootStatus) bootStatus(line.endsWith('1'));
                } else if (online && installing && line=="NEXUS 1 APK NEXT") {
                    transferTimer.start(180000);
                    if (apk.atEnd()) {
                        apk.close(); socket.write("NEXUS 1 APK COMMIT\n");
                    } else {
                        const auto data=apk.read(65536);
                        if (data.isEmpty()) { finishInstall(false,"APK read failed."); socket.abort(); return; }
                        const auto header="NEXUS 1 APK DATA "+QByteArray::number(data.size())+'\n';
                        if (socket.write(header)!=header.size() || socket.write(data)!=data.size()) {
                            finishInstall(false,"APK transfer failed."); socket.abort(); return;
                        }
                    }
                } else if (online && line=="NEXUS 1 APK FAILED") {
                    finishInstall(false,"Guest refused APK transfer; another command may be active.");
                } else if (online && appEvents && line.startsWith("NEXUS 1 APP ")) {
                    const QString package=QString::fromLatin1(line.mid(12));
                    static const QRegularExpression valid("^[A-Za-z][A-Za-z0-9_]*(?:\\.[A-Za-z0-9_]+)+$");
                    if (package!="-" && !valid.match(package).hasMatch()) { socket.abort(); return; }
                    heartbeat.restart();
                    if (activeApp) activeApp(package=="-" ? QString() : package);
                } else if (online && (line.startsWith("NEXUS 1 RESULT ") || line.startsWith("NEXUS 1 ERROR "))) {
                    heartbeat.restart();
                    if (line.startsWith("NEXUS 1 RESULT ")) {
                        commandPending=false;
                        QTimer::singleShot(100,this,[this] {
                            if (!queued.isEmpty() && isOnline() && !commandPending && !installing) send(queued.takeFirst());
                        });
                    }
                    if (line=="NEXUS 1 ERROR BUSY") {
                        commandPending=false;
                        if (diagnostic) diagnostic("Guest busy; command was not completed.");
                    }
                    if (line=="NEXUS 1 RESULT APK_INSTALL OK") finishInstall(true,"Android reports APK installation succeeded.");
                    else if (line=="NEXUS 1 RESULT APK_INSTALL FAILED") finishInstall(false,"APK checksum or Android package installation failed. No signature checks were bypassed.");
                    if (diagnostic) diagnostic(QString::fromLatin1(line));
                } else { if (diagnostic) diagnostic("Unsupported guest bridge message/version."); socket.abort(); return; }
            }
        });
        connect(&socket, &QLocalSocket::disconnected, this, [this] {
            online = false; appEvents=false; apkSupported=false; commandPending=false; queued.clear(); buffer.clear();
            if (installing) finishInstall(false,"Native guest connection lost; installation outcome may be unknown.");
            if (disconnected) disconnected();
        });
    }
    ~NativeGuestBridge() override { bootStatus={}; disconnected={}; diagnostic={}; activeApp={}; installFinished={}; stop(); }
    void start(const QString &name) { stop(); pipe=name; retry.start(); socket.connectToServer(pipe); }
    void stop() { pipe.clear(); retry.stop(); hashTimer.stop(); transferTimer.stop(); apk.close(); installing=false; socket.abort(); online=false; appEvents=false; apkSupported=false; commandPending=false; queued.clear(); buffer.clear(); }
    bool isOnline() const { return online && socket.state()==QLocalSocket::ConnectedState; }
    bool send(const QString &command) {
        static const QStringList allowed{"BACK","HOME","RECENTS","VOLUME_UP","VOLUME_DOWN","MUTE","SHUTDOWN","QUERY_APP"};
        if (!isOnline() || installing || (command=="QUERY_APP" && commandPending) || !allowed.contains(command) || socket.bytesToWrite() > 1024) return false;
        if (commandPending) {
            if (queued.size()>=16) return false;
            queued.append(command); return true;
        }
        const QByteArray packet = "NEXUS 1 "+command.toLatin1()+'\n';
        commandPending=true;
        return socket.write(packet)==packet.size(); // Acceptance != completed guest action.
    }
    bool supportsApk() const { return isOnline() && apkSupported; }
    bool isInstalling() const { return installing; }
    bool installApk(const QString &path) {
        if (!supportsApk() || installing || commandPending) return false;
        apk.setFileName(path);
        if (!apk.open(QIODevice::ReadOnly)) return false;
        if (apk.size()<=0 || apk.size()>512LL*1024*1024) { apk.close(); return false; }
        installing=true; hash.reset(); transferTimer.start(180000); hashTimer.start(); return true;
    }
    std::function<void(bool,const QString &)> installFinished;
    bool supportsAppEvents() const { return isOnline() && appEvents; }
    std::function<void(const QString &)> activeApp;
    std::function<void(bool)> bootStatus;
    std::function<void()> disconnected;
    std::function<void(const QString &)> diagnostic;
private:
    void finishInstall(bool success,const QString &message) {
        if (!installing) return;
        installing=false; hashTimer.stop(); transferTimer.stop(); apk.close();
        if (installFinished) installFinished(success,message);
    }
    QStringList queued;
    QFile apk;
    QTimer transferTimer, hashTimer;
    QCryptographicHash hash;
    bool installing=false,apkSupported=false,commandPending=false;
    QLocalSocket socket;
    QTimer retry;
    QElapsedTimer heartbeat;
    QByteArray buffer;
    QString pipe;
    bool online = false;
    bool appEvents = false;
};
