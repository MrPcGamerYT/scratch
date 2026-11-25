#pragma once
#include <QObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QElapsedTimer>
#include <QTimer>
#include <QUuid>
#include <functional>

// Counts instrumented SDL_RenderPresent completions from the engine, not guest
// compositor timestamps, Qt paint events, refresh timers or game frames.
class EnginePresentation : public QObject {
public:
    explicit EnginePresentation(QObject *parent) : QObject(parent), server(this), timer(this) {
        server.setSocketOptions(QLocalServer::UserAccessOption);
        timer.setInterval(1000);
        connect(&server,&QLocalServer::newConnection,this,[this] {
            auto *candidate=server.nextPendingConnection();
            if (peer) { candidate->abort(); candidate->deleteLater(); return; }
            peer=candidate; buffer.clear(); frames=0; elapsed.start();
            connect(peer,&QLocalSocket::readyRead,this,[this] {
                buffer+=peer->readAll();
                if (buffer.size()>65536) { peer->abort(); return; }
                while (buffer.contains('\n')) {
                    const auto end=buffer.indexOf('\n');
                    const auto line=buffer.left(end); buffer.remove(0,end+1);
                    if (line.contains("sdl2_2d_nexus_present ") && line.contains("console=0 ")) ++frames;
                }
            });
            connect(peer,&QLocalSocket::disconnected,this,[this,candidate] {
                if (peer==candidate) { peer=nullptr; buffer.clear(); if (sample) sample(-1); }
                candidate->deleteLater();
            });
        });
        connect(&timer,&QTimer::timeout,this,[this] {
            if (!peer || !elapsed.isValid()) { if (sample) sample(-1); return; }
            const auto ms=elapsed.restart();
            if (sample) sample(ms>0 ? frames*1000.0/ms : -1);
            frames=0;
        });
    }
    ~EnginePresentation() override { sample={}; stop(); }
    QString start() {
        stop();
        const auto name="nexus-present-"+QUuid::createUuid().toString(QUuid::WithoutBraces);
        if (!server.listen(name)) return {};
        timer.start(); return server.fullServerName();
    }
    void stop() {
        timer.stop(); server.close();
        if (peer) { auto *old=peer; peer=nullptr; old->disconnect(this); old->abort(); old->deleteLater(); }
        buffer.clear(); frames=0; elapsed.invalidate();
    }
    std::function<void(double)> sample;
private:
    QLocalServer server;
    QTimer timer;
    QLocalSocket *peer=nullptr;
    QElapsedTimer elapsed;
    QByteArray buffer;
    quint64 frames=0;
};
