#include "qmp.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <algorithm>

Qmp::Qmp(QProcess *p, QObject *parent) : QObject(parent), process(p) {
    connect(process, &QProcess::readyReadStandardOutput, this, &Qmp::readOutput);
}
void Qmp::reset() { buffer.clear(); touches.clear(); if (ready) { ready = false; emit readyChanged(false); } }
void Qmp::readOutput() {
    buffer += process->readAllStandardOutput();
    if (buffer.size() > 1024 * 1024) { buffer.clear(); emit error("QEMU monitor returned too much data."); return; }
    while (buffer.contains('\n')) {
        const int end = buffer.indexOf('\n');
        const auto line = buffer.left(end).trimmed(); buffer.remove(0, end + 1);
        const auto doc = QJsonDocument::fromJson(line);
        if (!doc.isObject()) continue;
        const auto object = doc.object();
        if (object.contains("QMP")) {
            process->write(QJsonDocument(QJsonObject{{"execute", "qmp_capabilities"}, {"id", "init"}}).toJson(QJsonDocument::Compact) + '\n');
        } else if (object.value("id").toString() == "init") {
            if (object.contains("return")) { ready = true; emit readyChanged(true); }
            else emit error("Could not initialize the QEMU monitor.");
        } else if (object.contains("error")) {
            emit error(object.value("error").toObject().value("desc").toString("QEMU rejected a command."));
        }
    }
}
void Qmp::send(const QString &command, const QJsonArray &events) {
    if (!ready || process->state() != QProcess::Running) return;
    QJsonObject object{{"execute", command}};
    if (!events.isEmpty()) object.insert("arguments", QJsonObject{{"events", events}});
    process->write(QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n');
}
QJsonArray Qmp::moveEvents(double x, double y) {
    const auto axis = [](const char *name, double value) {
        return QJsonObject{{"type", "abs"}, {"data", QJsonObject{{"axis", name}, {"value", qRound(std::clamp(value, 0.0, 1.0) * 32767)}}}};
    };
    return QJsonArray{axis("x", x), axis("y", y)};
}
QJsonArray Qmp::pointerEvents(double x, double y, bool down) {
    QJsonArray events = down ? moveEvents(x, y) : QJsonArray{};
    events.append(QJsonObject{{"type", "btn"}, {"data", QJsonObject{{"button", "left"}, {"down", down}}}});
    return events;
}
void Qmp::pointerMove(double x, double y) { send("input-send-event", moveEvents(x, y)); }
void Qmp::powerDown() { send("system_powerdown"); }
void Qmp::pointerDown(double x, double y) { send("input-send-event", pointerEvents(x, y, true)); }
void Qmp::pointerUp() { send("input-send-event", pointerEvents(0, 0, false)); }

// Touch events target a display-bound virtio touchscreen, not the USB tablet.
QJsonArray Qmp::touchEvents(const QString &type, int slot, double x, double y, bool firstOrLast) {
    const auto mtt = [slot](const QString &kind, const QString &axis, int value, int tracking) {
        return QJsonObject{{"type", "mtt"}, {"data", QJsonObject{{"type", kind}, {"slot", slot}, {"tracking-id", tracking}, {"axis", axis}, {"value", value}}}};
    };
    QJsonArray events;
    if (type == "begin" && firstOrLast) events.append(QJsonObject{{"type", "btn"}, {"data", QJsonObject{{"button", "touch"}, {"down", true}}}});
    events.append(mtt(type, "x", 0, type == "end" ? -1 : slot));
    if (type != "end") {
        events.append(mtt("data", "x", qRound(std::clamp(x, 0.0, 1.0) * 32767), slot));
        events.append(mtt("data", "y", qRound(std::clamp(y, 0.0, 1.0) * 32767), slot));
    }
    if (type == "end" && firstOrLast) events.append(QJsonObject{{"type", "btn"}, {"data", QJsonObject{{"button", "touch"}, {"down", false}}}});
    return events;
}
void Qmp::sendTouch(const QJsonArray &events) {
    if (!ready || process->state() != QProcess::Running) return;
    QJsonObject object{{"execute", "input-send-event"}, {"arguments", QJsonObject{{"device", "nexus_display"}, {"events", events}}}};
    process->write(QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n');
}
void Qmp::touchDown(int slot, double x, double y) {
    if (!ready || process->state() != QProcess::Running || slot < 0 || slot >= 10 || touches.contains(slot)) return;
    const bool first = touches.isEmpty(); touches.insert(slot); sendTouch(touchEvents("begin", slot, x, y, first));
}
void Qmp::touchMove(int slot, double x, double y) {
    if (!touches.contains(slot)) return;
    sendTouch(touchEvents("update", slot, x, y, false));
}
void Qmp::touchUp(int slot) {
    if (!touches.remove(slot)) return;
    sendTouch(touchEvents("end", slot, 0, 0, touches.isEmpty()));
}
