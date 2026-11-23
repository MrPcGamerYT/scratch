#pragma once
#include "keymap.h"
#include <QObject>
#include <QList>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class Qmp;
class QTimer;

class InputMapper : public QObject {
    Q_OBJECT
public:
    InputMapper(Qmp *monitor, QObject *parent = nullptr);
    ~InputMapper() override;
    bool start(HWND guest, const QList<KeyBinding> &bindings);
    void stop();
    bool isEnabled() const { return hook != nullptr; }
signals:
    void stopped(const QString &reason);
private:
    static LRESULT CALLBACK hookProc(int code, WPARAM message, LPARAM data);
    bool viewportFocused() const;
    void release();
    Qmp *qmp;
    HWND guest = nullptr;
    HHOOK hook = nullptr;
    QList<KeyBinding> bindings;
    int heldKey = 0;
    QTimer *focusTimer;
    static InputMapper *instance;
};
