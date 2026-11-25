#include "nativeguestbridge.h"
#include <QCoreApplication>
#include <QLocalServer>
#include <QUuid>
#include <QThread>

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
    return 0;
}
