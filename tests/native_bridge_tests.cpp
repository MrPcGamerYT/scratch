#include "nativeguestbridge.h"
#include "enginepresentation.h"
#include <QCoreApplication>
#include <QLocalServer>
#include <QUuid>
#include <QThread>
#include <QTemporaryFile>

static bool until(const std::function<bool()> &condition) {
    QElapsedTimer timer; timer.start();
    while (!condition() && timer.elapsed()<3000) { QCoreApplication::processEvents(); QThread::msleep(2); }
    return condition();
}
int main(int argc,char **argv) {
    QCoreApplication app(argc,argv);
    QLocalServer server;
    const QString name="nexus-test-"+QUuid::createUuid().toString(QUuid::WithoutBraces);
    if (!server.listen(name)) return 1;
    NativeGuestBridge bridge(&app);
    bool ready=false; int disconnects=0;
    bridge.bootStatus=[&](bool value) {ready=value;};
    bridge.disconnected=[&] {++disconnects;};
    bridge.start(name);
    if (!until([&]{return server.hasPendingConnections();})) return 2;
    auto *peer=server.nextPendingConnection();
    if (bridge.send("HOME")) return 3;
    peer->write("NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK\nNEXUS 1 READY 1\n"); peer->flush();
    if (!until([&]{return ready && bridge.isOnline();})) return 4;
    if (bridge.send("HOME;reboot") || !bridge.send("HOME")) return 5;
    if (!until([&]{return peer->canReadLine();}) || peer->readLine()!="NEXUS 1 HOME\n") return 6;
    peer->write("NEXUS 2 READY 1\n"); peer->flush();
    if (!until([&]{return !bridge.isOnline() && disconnects>0;})) return 7;
    bridge.stop();
    QString package;
    bool installed=false;
    bridge.activeApp=[&](const QString &value) {package=value;};
    bridge.installFinished=[&](bool ok,const QString &) {installed=ok;};
    bridge.start(name);
    if (!until([&]{return server.hasPendingConnections();})) return 8;
    auto *second=server.nextPendingConnection();
    second->write("NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK\nNEXUS 1 APP org.example.game\n"); second->flush();
    if (!until([&]{return bridge.supportsApk() && package=="org.example.game";})) return 9;
    QTemporaryFile fixture; if (!fixture.open()) return 10;
    fixture.write("fake-apk-test"); fixture.flush();
    if (!bridge.installApk(fixture.fileName())) return 11;
    if (!until([&]{return second->canReadLine();})) return 12;
    if (!second->readLine().startsWith("NEXUS 1 APK BEGIN 13 ")) return 13;
    second->write("NEXUS 1 APK NEXT\n"); second->flush();
    if (!until([&]{return second->bytesAvailable()>=33;})) return 14;
    if (second->readLine()!="NEXUS 1 APK DATA 13\n" || second->read(13)!="fake-apk-test") return 15;
    second->write("NEXUS 1 APK NEXT\n"); second->flush();
    if (!until([&]{return second->canReadLine();}) || second->readLine()!="NEXUS 1 APK COMMIT\n") return 16;
    second->write("NEXUS 1 RESULT APK_INSTALL OK\n"); second->flush();
    if (!until([&]{return installed && !bridge.isInstalling();})) return 17;
    bridge.stop();
    EnginePresentation counter(&app);
    double fps=-1;
    counter.sample=[&](double rate) {fps=rate;};
    const auto endpoint=counter.start();
    if (endpoint.isEmpty()) return 18;
    QLocalSocket producer; producer.connectToServer(endpoint);
    if (!until([&]{return producer.state()==QLocalSocket::ConnectedState;})) return 19;
    producer.write("123: sdl2_2d_nexus_present console=0 rectangles=1\n123: sdl2_2d_nexus_present console=1 rectangles=20\n123: sdl2_2d_nexus_present console=0 rectangles=50\nnoise\n"); producer.flush();
    if (!until([&]{return fps>0;})) return 20;
    if (fps<1 || fps>4) return 21; // two presents, not 51 dirty rectangles
    producer.abort();
    if (!until([&]{return fps<0;})) return 22;
    counter.stop();
    return 0;
}
