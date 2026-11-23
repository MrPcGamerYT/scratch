#pragma once
#include <QObject>
#include <QPointF>
#include "aimmodel.h"
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class Qmp;
class QTimer;

// Opt-in experimental Win32 mouse-to-single-pointer drag and tap.
class AimController : public QObject {
    Q_OBJECT
public:
    explicit AimController(Qmp *qmp, QObject *parent = nullptr);
    ~AimController() override;
    bool arm(HWND guest, QPointF anchor, QPointF fire, double sensitivity);
    void disarm();
    bool isArmed() const { return keyboard != nullptr; }
signals:
    void statusChanged(bool armed, bool active, const QString &message);
private:
    static LRESULT CALLBACK keyboardProc(int code, WPARAM msg, LPARAM data);
    static LRESULT CALLBACK mouseProc(int code, WPARAM msg, LPARAM data);
    bool focused() const;
    bool start();
    void stop();
    void flushMove();
    bool centerCursor();
    Qmp *monitor;
    HWND guest = nullptr;
    HHOOK keyboard = nullptr;
    HHOOK mouse = nullptr;
    QTimer *timer;
    AimModel model{QPointF(.5, .5), 1};
    QPointF firePoint{.8, .75};
    POINT cursorCenter{};
    RECT lockedBounds{};
    bool active = false;
    bool firing = false;
    bool f8Held = false;
    int pendingX = 0;
    int pendingY = 0;
    static AimController *instance;
};
