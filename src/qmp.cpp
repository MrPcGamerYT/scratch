#include "qmp.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <algorithm>

Qmp::Qmp(QProcess *p, QObject *parent) : QObject(parent), process(p) {
    connect(process, &QProcess::readyReadStandardOutput, this, &Qmp::readOutput);
}
void Qmp::reset() { buffer.clear(); if (ready) { ready = false; emit readyChanged(false); } }
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
QJsonArray Qmp::pointerEvents(double x, double y, bool down) {
    QJsonArray events;
    if (down) {
        const auto axis = [](const char *name, double value) {
            return QJsonObject{{"type", "abs"}, {"data", QJsonObject{{"axis", name}, {"value", qRound(std::clamp(value, 0.0, 1.0) * 32767)}}}};
        };
        events.append(axis("x", x)); events.append(axis("y", y));
    }
    events.append(QJsonObject{{"type", "btn"}, {"data", QJsonObject{{"button", "left"}, {"down", down}}}});
    return events;
}
void Qmp::powerDown() { send("system_powerdown"); }
void Qmp::pointerDown(double x, double y) { send("input-send-event", pointerEvents(x, y, true)); }
void Qmp::pointerUp() { send("input-send-event", pointerEvents(0, 0, false)); }
