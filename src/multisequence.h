#pragma once
#include "keymap.h"
#include <QObject>
#include <array>
class ClickSequence;
// Slots 0 and 1 are reserved for camera and fire; keys use slots 2 through 9.
class MultiSequence : public QObject {
    Q_OBJECT
public:
    explicit MultiSequence(QObject *parent = nullptr);
    bool press(int key, const QList<KeyBinding> &bindings);
    void release(int key);
    void cancel();
    bool active() const;
signals:
    void touchDown(int slot, double x, double y);
    void touchUp(int slot);
private:
    struct Entry { int key = 0; ClickSequence *sequence = nullptr; };
    std::array<Entry, 8> entries;
};
