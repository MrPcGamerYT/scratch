#include "keymap.h"
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QSaveFile>
#include <QHash>
#include <cmath>

bool validKey(int key) { return (key >= 'A' && key <= 'Z') || (key >= '0' && key <= '9') || key == 0x20; }
QString keyName(int key) { return key == 0x20 ? QStringLiteral("Space") : QString(QChar(key)); }

static bool validBindings(const QList<KeyBinding> &bindings, QString *error) {
    if (bindings.size() > 128) { *error = "Too many keymap markers (maximum 128)."; return false; }
    QHash<int, QPair<bool, int>> modes;
    for (const auto &b : bindings) {
        if (!validKey(b.key) || !std::isfinite(b.x) || !std::isfinite(b.y) || b.x < 0 || b.x > 1 || b.y < 0 || b.y > 1 || b.intervalMs < 100 || b.intervalMs > 5000) {
            *error = "Invalid key, position, or repeat interval."; return false;
        }
        const QPair<bool, int> mode{b.repeat, b.intervalMs};
        if (modes.contains(b.key) && modes.value(b.key) != mode) { *error = "Markers for the same key must use the same repeat settings."; return false; }
        modes.insert(b.key, mode);
    }
    return true;
}

bool loadKeymap(const QString &path, QList<KeyBinding> *bindings, QString *error) {
    QFile f(path);
    if (!f.exists()) { bindings->clear(); return true; }
    if (!f.open(QIODevice::ReadOnly)) { *error = f.errorString(); return false; }
    if (f.size() > 65536) { *error = "Profile is too large."; return false; }
    QJsonParseError parse;
    const auto doc = QJsonDocument::fromJson(f.readAll(), &parse);
    const int version = doc.isObject() ? doc.object().value("version").toInt() : 0;
    if (parse.error != QJsonParseError::NoError || (version != 1 && version != 2) || !doc.object().value("bindings").isArray()) {
        *error = "Invalid keymap profile."; return false;
    }
    QList<KeyBinding> result;
    const auto entries = doc.object().value("bindings").toArray();
    if (entries.size() > 128) { *error = "Too many key bindings."; return false; }
    for (const auto &entry : entries) {
        if (!entry.isObject()) { *error = "Invalid key binding."; return false; }
        const auto o = entry.toObject();
        if (!o.value("key").isDouble() || !o.value("x").isDouble() || !o.value("y").isDouble()) { *error = "Invalid key binding fields."; return false; }
        const double rawKey = o.value("key").toDouble();
        if (!std::isfinite(rawKey) || rawKey != std::floor(rawKey) || rawKey < 0 || rawKey > 255) { *error = "Invalid key code."; return false; }
        KeyBinding b{static_cast<int>(rawKey), o.value("x").toDouble(), o.value("y").toDouble()};
        if (version == 2) {
            if (!o.value("repeat").isBool() || !o.value("intervalMs").isDouble()) { *error = "Invalid repeat settings."; return false; }
            double interval = o.value("intervalMs").toDouble();
            if (!std::isfinite(interval) || interval != std::floor(interval) || interval < 100 || interval > 5000) { *error = "Invalid repeat interval."; return false; }
            b.repeat = o.value("repeat").toBool(); b.intervalMs = static_cast<int>(interval);
        }
        result.append(b);
    }
    if (!validBindings(result, error)) return false;
    *bindings = result; return true;
}

bool saveKeymap(const QString &path, const QList<KeyBinding> &bindings, QString *error) {
    if (!validBindings(bindings, error)) return false;
    QJsonArray entries;
    for (const auto &b : bindings) entries.append(QJsonObject{{"key", b.key}, {"x", b.x}, {"y", b.y}, {"repeat", b.repeat}, {"intervalMs", b.intervalMs}});
    QSaveFile f(path);
    if (!f.open(QIODevice::WriteOnly)) { *error = f.errorString(); return false; }
    const QByteArray bytes = QJsonDocument(QJsonObject{{"version", 2}, {"bindings", entries}}).toJson();
    if (f.write(bytes) != bytes.size() || !f.commit()) { *error = f.errorString(); return false; }
    return true;
}
