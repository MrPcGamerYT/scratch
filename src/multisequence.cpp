#include "multisequence.h"
#include "clicksequence.h"

MultiSequence::MultiSequence(QObject *parent) : QObject(parent) {
    for (int i = 0; i < static_cast<int>(entries.size()); ++i) {
        entries[i].sequence = new ClickSequence(this);
        const int slot = i + 2;
        connect(entries[i].sequence, &ClickSequence::pointerDown, this, [this, slot](double x, double y) { emit touchDown(slot, x, y); });
        connect(entries[i].sequence, &ClickSequence::pointerUp, this, [this, slot] { emit touchUp(slot); });
    }
}
bool MultiSequence::press(int key, const QList<KeyBinding> &bindings) {
    for (const auto &entry : entries) if (entry.key == key && entry.sequence->active()) return true;
    for (auto &entry : entries) if (!entry.sequence->active()) {
        entry.key = key;
        if (entry.sequence->press(key, bindings)) return true;
        entry.key = 0; return false;
    }
    return false;
}
void MultiSequence::release(int key) {
    for (auto &entry : entries) if (entry.key == key && entry.sequence->active()) entry.sequence->release(key);
}
void MultiSequence::cancel() { for (auto &entry : entries) { entry.sequence->cancel(); entry.key = 0; } }
bool MultiSequence::active() const { for (const auto &entry : entries) if (entry.sequence->active()) return true; return false; }
