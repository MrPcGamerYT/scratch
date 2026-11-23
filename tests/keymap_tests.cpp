#include "keymap.h"
#include "qmp.h"
#include <QCoreApplication>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QTemporaryDir>
#include <iostream>

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QTemporaryDir tmp; if (!tmp.isValid()) return 1;
    const QString path = tmp.filePath("profile.json");
    QString error;
    QList<KeyBinding> bindings{{'W', 0.25, 0.75}, {0x20, 1.0, 0.0}}, result;
    if (!saveKeymap(path, bindings, &error) || !loadKeymap(path, &result, &error) || result.size() != 2 || result[0].key != 'W' || result[0].x != 0.25 || result[1].key != 0x20) return 2;
    if (saveKeymap(path, {{'W', 0.5, 0.5}, {'W', 0.1, 0.2}}, &error)) return 3;
    if (!loadKeymap(path, &result, &error) || result.size() != 2) return 4;
    if (saveKeymap(path, {{'A', -0.1, 0.5}}, &error) || saveKeymap(path, {{10, 0.5, 0.5}}, &error)) return 5;
    QFile file(path); if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) return 6;
    file.write("{\"version\":1,\"bindings\":[{\"key\":65,\"x\":2,\"y\":0}]}\n"); file.close();
    if (loadKeymap(path, &result, &error)) return 7;
    auto press = Qmp::pointerEvents(0.5, 1.0, true);
    auto release = Qmp::pointerEvents(0, 0, false);
    if (press.size() != 3 || release.size() != 1) return 8;
    if (press[0].toObject().value("data").toObject().value("value").toInt() != 16384 || press[1].toObject().value("data").toObject().value("value").toInt() != 32767) return 9;
    if (release[0].toObject().value("data").toObject().value("down").toBool(true)) return 10;
    std::cout << "Keymap persistence, validation and QMP pointer events passed.\n";
    return 0;
}
