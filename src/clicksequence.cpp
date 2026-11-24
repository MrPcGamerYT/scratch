#include "clicksequence.h"

ClickSequence::ClickSequence(QObject *parent) : QObject(parent) {
    timer.setSingleShot(true);
    connect(&timer, &QTimer::timeout, this, &ClickSequence::step);
}
bool ClickSequence::press(int key, const QList<KeyBinding> &bindings) {
    if (active()) return false;
    points.clear();
    for (const auto &binding : bindings) if (binding.key == key) points.append(binding);
    if (points.isEmpty()) return false;
    activeKey = key; physicallyHeld = true; index = 0;
    simpleHold = points.size() == 1 && !points[0].repeat;
    pointerHeld = true; emit pointerDown(points[0].x, points[0].y);
    if (!simpleHold) timer.start(50);
    return true;
}
void ClickSequence::release(int key) {
    if (key != activeKey) return;
    physicallyHeld = false;
    if (simpleHold || points[0].repeat) cancel();
    // A one-shot multi-position sequence completes even for a quick keypress.
}
void ClickSequence::reset() { timer.stop(); points.clear(); activeKey = 0; index = 0; physicallyHeld = false; pointerHeld = false; simpleHold = false; }
void ClickSequence::cancel() { if (pointerHeld) emit pointerUp(); reset(); }
void ClickSequence::step() {
    if (!active()) return;
    if (pointerHeld) {
        emit pointerUp(); pointerHeld = false; ++index;
        if (index >= points.size()) {
            if (!points[0].repeat || !physicallyHeld) { reset(); return; }
            index = 0; timer.start(points[0].intervalMs); return;
        }
        timer.start(50); return;
    }
    pointerHeld = true; emit pointerDown(points[index].x, points[index].y); timer.start(50);
}
