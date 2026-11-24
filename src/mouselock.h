#pragma once
#include <QObject>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class QTimer;

class MouseLock : public QObject {
    Q_OBJECT
public:
    explicit MouseLock(QObject *parent = nullptr);
    ~MouseLock() override;
    bool lock(HWND guest);
    void unlock();
    bool isLocked() const { return hook != nullptr; }
signals:
    void unlocked();
private:
    static LRESULT CALLBACK hookProc(int code, WPARAM message, LPARAM data);
    bool updateClip();
    HWND window = nullptr;
    HHOOK hook = nullptr;
    QTimer *timer;
    static MouseLock *instance;
};
