#pragma once

#include <QList>
#include <QString>

struct KeyBinding {
    int key = 0; // Windows virtual key: letters, numbers and space only.
    double x = 0;
    double y = 0;
    bool repeat = false;
    int intervalMs = 250;
};

QString keyName(int key);
bool validKey(int key);
bool loadKeymap(const QString &path, QList<KeyBinding> *bindings, QString *error);
bool saveKeymap(const QString &path, const QList<KeyBinding> &bindings, QString *error);
