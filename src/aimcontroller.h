#pragma once
#include <QObject>
#include <QPointF>
#include "aimmodel.h"
#include "keymap.h"
#include "rawmouse.h"
#include <QElapsedTimer>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class Qmp;
class QTimer;

// Win32 aim controller supporting asymmetric X/Y sensitivity, Left-Click Fire,
// Right-Click ADS (tap or hold), Free-Look eye camera, and hold-to-suspend cursor.
class AimController : public QObject {
    Q_OBJECT
public:
    explicit AimController(Qmp *qmp, QObject *parent = nullptr);
    ~AimController() override;
    bool arm(HWND guest, const AimConfig &config, bool multitouch = false);
    bool arm(HWND guest, QPointF anchor, QPointF fire, double sensitivity, bool multitouch = false);
    void disarm();
    bool isArmed() const { return keyboard != nullptr; }
    bool isActive() const { return active; }
    bool isCapturingMouse() const { return active && !suspended; }
    QString inputDiagnostics() const;
    const AimConfig &currentConfig() const { return cfg; }
signals:
    void statusChanged(bool armed, bool active, const QString &message);
    void stateChanged(bool armed, bool active, bool suspended, const QString &message);
private:
    static LRESULT CALLBACK keyboardProc(int code, WPARAM msg, LPARAM data);
    static LRESULT CALLBACK mouseProc(int code, WPARAM msg, LPARAM data);
    bool focused() const;
    bool start();
    void stop();
    void enterSuspend();
    void exitSuspend();
    void setFreeLookActive(bool on);
    void triggerAdsDown();
    void triggerAdsUp();
    void flushMove();
    void queueMove(int dx, int dy);
    RawMouseInput rawMouse;
    QElapsedTimer pendingAge;
    quint64 receivedMoves = 0, sentBatches = 0;
    qint64 maximumBatchWait = 0;
    bool useRawMouse = false;
    bool centerCursor();
    void notifyState(bool armedState, bool activeState, bool suspendedState, const QString &msg);

    Qmp *monitor;
    HWND guest = nullptr;
    HHOOK keyboard = nullptr;
    HHOOK mouse = nullptr;
    QTimer *timer;
    AimConfig cfg;
    AimModel model{QPointF(.5, .5), 1.0, 1.0};
    AimModel freeLookModel{QPointF(.76, .26), 1.0, 1.0};
    POINT cursorCenter{};
    RECT lockedBounds{};
    bool active = false;
    bool suspended = false;
    bool freeLooking = false;
    bool firing = false;
    bool adsHeld = false;
    bool touchMode = false;
    bool toggleHeld = false;
    bool suspendHeld = false;
    int pendingX = 0;
    int pendingY = 0;
    static AimController *instance;
};
