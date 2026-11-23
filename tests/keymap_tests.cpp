#include "keymap.h"
#include "clicksequence.h"
#include "qmp.h"
#include <QCoreApplication>
#include <QEventLoop>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QTimer>
#include <iostream>

static void wait(int ms) { QEventLoop loop; QTimer::singleShot(ms, &loop, &QEventLoop::quit); loop.exec(); }
int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QTemporaryDir tmp; if (!tmp.isValid()) return 1;
    const QString path = tmp.filePath("profile.json"); QString error;
    QList<KeyBinding> bindings{{'W', 0.25, 0.75, true, 200}, {'W', 0.9, 0.1, true, 200}, {0x20, 1.0, 0.0}}, result;
    if (!saveKeymap(path, bindings, &error) || !loadKeymap(path, &result, &error) || result.size() != 3 || !result[0].repeat || result[1].key != 'W') return 2;
    if (saveKeymap(path, {{'W', 0.5, 0.5, true, 200}, {'W', 0.1, 0.2, false, 200}}, &error)) return 3;
    if (!loadKeymap(path, &result, &error) || result.size() != 3) return 4;
    if (saveKeymap(path, {{'A', -0.1, 0.5}}, &error) || saveKeymap(path, {{10, 0.5, 0.5}}, &error) || saveKeymap(path, {{'A', 0.1, 0.5, true, 1}}, &error)) return 5;
    QFile file(path); if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) return 6;
    file.write("{\"version\":1,\"bindings\":[{\"key\":65,\"x\":0.2,\"y\":0.3}]}\n"); file.close();
    if (!loadKeymap(path, &result, &error) || result.size() != 1 || result[0].repeat || result[0].intervalMs != 250) return 7;
    auto press = Qmp::pointerEvents(0.5, 1.0, true); auto release = Qmp::pointerEvents(0, 0, false);
    if (press.size() != 3 || release.size() != 1 || press[0].toObject().value("data").toObject().value("value").toInt() != 16384) return 8;
    ClickSequence seq; QString events;
    QObject::connect(&seq, &ClickSequence::pointerDown, &app, [&](double, double) { events += 'D'; });
    QObject::connect(&seq, &ClickSequence::pointerUp, &app, [&] { events += 'U'; });
    if (!seq.press('A', {{'A', 0.2, 0.3}})) return 9;
    seq.release('A'); if (events != "DU" || seq.active()) return 10;
    events.clear();
    if (!seq.press('A', {{'A', 0.2, 0.3}, {'A', 0.8, 0.9}})) return 11;
    seq.release('A'); wait(250);
    if (events != "DUDU" || seq.active()) return 12;
    events.clear();
    if (!seq.press('A', {{'A', 0.2, 0.3, true, 100}})) return 13;
    wait(600); seq.release('A'); wait(180);
    if (events.count('D') < 2 || events.count('D') != events.count('U') || seq.active()) return 14;
    events.clear(); seq.press('A', {{'A', 0.2, 0.3, true, 100}}); seq.cancel(); wait(150);
    if (events != "DU" || seq.active()) return 15;
    std::cout << "Keymap compatibility, sequential taps, repeat and cancellation passed.\n";
    return 0;
}
