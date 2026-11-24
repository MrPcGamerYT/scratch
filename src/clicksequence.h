#pragma once
#include "keymap.h"
#include <QObject>
#include <QTimer>

// QEMU's tablet is one pointer, so same-key markers are clicked in sequence.
class ClickSequence : public QObject {
    Q_OBJECT
public:
    explicit ClickSequence(QObject *parent = nullptr);
    bool press(int key, const QList<KeyBinding> &bindings);
    void release(int key);
    void cancel();
    bool active() const { return activeKey != 0; }
signals:
    void pointerDown(double x, double y);
    void pointerUp();
private:
    void step();
    void reset();
    QTimer timer;
    QList<KeyBinding> points;
    int activeKey = 0;
    int index = 0;
    bool physicallyHeld = false;
    bool pointerHeld = false;
    bool simpleHold = false;
};
