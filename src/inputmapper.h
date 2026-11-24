#pragma once
#include "keymap.h"
#include <QObject>
#include <QPointF>
#include <QSet>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

class Qmp;
class KeymapEngine;
class QTimer;

class InputMapper : public QObject {
    Q_OBJECT
public:
    explicit InputMapper(Qmp *monitor, QObject *parent = nullptr);
    ~InputMapper() override;
    bool start(HWND guest, const KeymapProfile &profile, bool multitouch = false);
    bool start(HWND guest, const QList<KeyBinding> &bindings, bool multitouch = false);
    void stop();
    bool isEnabled() const { return keyHook != nullptr; }
    void setReservedTouchSlots(const QSet<int> &slotsReserved);
    void setAimCapturingMouse(bool capturing) { aimCapturingMouse = capturing; }
signals:
    void stopped(const QString &reason);
private:
    static LRESULT CALLBACK keyHookProc(int code, WPARAM message, LPARAM data);
    static LRESULT CALLBACK mouseHookProc(int code, WPARAM message, LPARAM data);
    bool viewportFocused() const;
    bool cursorNormalized(POINT screenPt, QPointF *outNorm) const;
    QPointF currentCursorNorm() const;
    void release();

    Qmp *qmp;
    HWND guest = nullptr;
    HHOOK keyHook = nullptr;
    HHOOK mouseHook = nullptr;
    KeymapEngine *engine;
    QSet<int> heldKeys;
    QSet<int> heldMouseButtons;
    bool aimCapturingMouse = false;
    QTimer *focusTimer;
    static InputMapper *instance;
};
