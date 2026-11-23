#include "keymap.h"
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QSaveFile>
#include <QSet>
#include <cmath>

bool validKey(int key) { return (key >= 'A' && key <= 'Z') || (key >= '0' && key <= '9') || key == 0x20; }
QString keyName(int key) { return key == 0x20 ? QStringLiteral("Space") : QString(QChar(key)); }

bool loadKeymap(const QString &path, QList<KeyBinding> *bindings, QString *error) {
    QFile f(path);
    if (!f.exists()) { bindings->clear(); return true; }
    if (!f.open(QIODevice::ReadOnly)) { *error = f.errorString(); return false; }
    if (f.size() > 65536) { *error = "Profile is too large."; return false; }
    QJsonParseError parse;
    const auto doc = QJsonDocument::fromJson(f.readAll(), &parse);
    if (parse.error != QJsonParseError::NoError || !doc.isObject() || doc.object().value("version").toInt() != 1 || !doc.object().value("bindings").isArray()) {
        *error = "Invalid keymap profile."; return false;
    }
    QList<KeyBinding> result;
    QSet<int> keys;
    const auto entries = doc.object().value("bindings").toArray();
    if (entries.size() > 37) { *error = "Too many key bindings."; return false; }
    for (const auto &entry : entries) {
        if (!entry.isObject()) { *error = "Invalid key binding."; return false; }
        const auto o = entry.toObject();
        if (!o.value("key").isDouble() || !o.value("x").isDouble() || !o.value("y").isDouble()) { *error = "Invalid key binding fields."; return false; }
        const double rawKey = o.value("key").toDouble();
        if (!std::isfinite(rawKey) || rawKey != std::floor(rawKey) || rawKey < 0 || rawKey > 255) { *error = "Invalid key code."; return false; }
        KeyBinding b{static_cast<int>(rawKey), o.value("x").toDouble(), o.value("y").toDouble()};
        if (!validKey(b.key) || keys.contains(b.key) || !std::isfinite(b.x) || !std::isfinite(b.y) || b.x < 0 || b.x > 1 || b.y < 0 || b.y > 1) {
            *error = "Invalid or duplicate key binding."; return false;
        }
        keys.insert(b.key); result.append(b);
    }
    *bindings = result;
    return true;
}

bool saveKeymap(const QString &path, const QList<KeyBinding> &bindings, QString *error) {
    QSet<int> keys; QJsonArray entries;
    for (const auto &b : bindings) {
        if (!validKey(b.key) || keys.contains(b.key) || !std::isfinite(b.x) || !std::isfinite(b.y) || b.x < 0 || b.x > 1 || b.y < 0 || b.y > 1) {
            *error = "Invalid or duplicate key binding."; return false;
        }
        keys.insert(b.key);
        entries.append(QJsonObject{{"key", b.key}, {"x", b.x}, {"y", b.y}});
    }
    QSaveFile f(path);
    if (!f.open(QIODevice::WriteOnly)) { *error = f.errorString(); return false; }
    const QByteArray bytes = QJsonDocument(QJsonObject{{"version", 1}, {"bindings", entries}}).toJson();
    if (f.write(bytes) != bytes.size() || !f.commit()) { *error = f.errorString(); return false; }
    return true;
}
