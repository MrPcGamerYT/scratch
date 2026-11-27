#include "clicksequence.h"
#include <algorithm>

ClickSequence::ClickSequence(QObject *parent) : QObject(parent) {
    timer.setSingleShot(true);
    connect(&timer, &QTimer::timeout, this, &ClickSequence::step);
}
bool ClickSequence::press(int key, const QList<KeyBinding> &bindings) {
    if (active()) return false;
    points.clear();
    for (const auto &binding : bindings) if (keysMatch(binding.key, key)) points.append(binding);
    if (points.isEmpty()) return false;
    activeKey = key;
    physicallyHeld = true;
    index = 0;
    const int burst = std::clamp(points[0].burstCount, 1, 50);
    remainingCycles = points[0].repeat ? 1 : burst;
    simpleHold = points.size() == 1 && !points[0].repeat && burst == 1;
    pointerHeld = true;
    emit pointerDown(points[0].x, points[0].y);
    if (!simpleHold) {
        const int holdMs = burst > 1 ? std::clamp(points[0].intervalMs / 2, 15, 45) : 50;
        timer.start(holdMs);
    }
    return true;
}
void ClickSequence::release(int key) {
    if (!keysMatch(activeKey, key)) return;
    physicallyHeld = false;
    if (simpleHold || points[0].repeat) cancel();
    // One-shot multi-position sequences and fixed bursts finish cleanly on quick tap.
}
void ClickSequence::reset() {
    const bool wasActive = activeKey != 0;
    timer.stop();
    points.clear();
    activeKey = 0;
    index = 0;
    remainingCycles = 1;
    physicallyHeld = false;
    pointerHeld = false;
    simpleHold = false;
    if (wasActive) emit finished();
}
void ClickSequence::cancel() {
    if (pointerHeld) { pointerHeld = false; emit pointerUp(); }
    reset();
}
void ClickSequence::step() {
    if (!active()) return;
    const int burst = std::clamp(points[0].burstCount, 1, 50);
    const int holdMs = burst > 1 ? std::clamp(points[0].intervalMs / 2, 15, 45) : 50;
    if (pointerHeld) {
        emit pointerUp();
        pointerHeld = false;
        ++index;
        if (index >= points.size()) {
            if (points[0].repeat) {
                if (!physicallyHeld) { reset(); return; }
                index = 0;
                timer.start(std::clamp(points[0].intervalMs, 25, 5000));
                return;
            }
            --remainingCycles;
            if (remainingCycles <= 0) { reset(); return; }
            index = 0;
            timer.start(std::clamp(points[0].intervalMs - holdMs, 15, 5000));
            return;
        }
        timer.start(holdMs);
        return;
    }
    pointerHeld = true;
    emit pointerDown(points[index].x, points[index].y);
    timer.start(holdMs);
}
