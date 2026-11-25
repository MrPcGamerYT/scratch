#pragma once
#include <QMainWindow>
#include <QElapsedTimer>
#include "keymap.h"
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class NativeGuestBridge;
class EmulatorView;
class Qmp;
class InputMapper;
class MouseLock;
class AimController;
class KeymapOverlay;
class QCheckBox;
class QLabel;
class QProcess;
class QPushButton;
class QTimer;
class QCloseEvent;
class QDragEnterEvent;
class QDropEvent;

class MainWindow : public QMainWindow {
public:
    explicit MainWindow(QWidget *parent = nullptr);
protected:
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dropEvent(QDropEvent *event) override;
    void closeEvent(QCloseEvent *event) override;
    void changeEvent(QEvent *event) override;
private:
    void launch();
    void requestCloseShutdown();
    void shutdownWaitExpired();
    bool closePending = false;
    bool forcedClose = false;
    QTimer *shutdownTimer = nullptr;
    QProcess *shutdownTask = nullptr;
    void pollGuest();
    void selectAppKeymap(const QString &package);
    QProcess *guestTask = nullptr;
    bool telemetryConnected = false;
    int telemetryFailures = 0;
    QElapsedTimer telemetryFailureTime;
    QTimer *guestPoll = nullptr;
    QString currentPackage;
    QString globalProfileDir;
    bool bootReady = false;
    bool hideGuestBoot = false;
    QElapsedTimer bootWait;
    bool switchingApp = false;
    QLabel *fpsBadge = nullptr;
    void recordEvent(const QString &message);
    QString sessionLog;
    QString engineErrorTail;
    void findGuestWindow();
    void editKeymap();
    void installApk(const QString &path = QString());
    void androidAction(int keycode);
    void applyGuestDisplay(const QString &size, int dpi);
    void updateControls();
    void setRunning(bool running);
    EmulatorView *view;
    KeymapOverlay *overlay;
    QCheckBox *keymapEnabled;
    QCheckBox *overlayEnabled;
    QPushButton *editKeymapButton;
    QPushButton *fullscreenButton;
    QPushButton *mouseLockButton;
    QPushButton *aimButton;
    QPushButton *launchButton;
    QPushButton *shutdownButton;
    QLabel *vmStatus;
    NativeGuestBridge *nativeBridge;
    Qmp *qmp;
    InputMapper *mapper;
    MouseLock *mouseLock;
    AimController *aim;
    KeymapProfile profile;
    QString profileDir;
    QString activeScheme;
    QString keymapPath;
    QProcess *process;
    QProcess *apkTask = nullptr;
    bool runningAdbForward = false;
    bool runningMultiTouch = false;
    int runningAdbPort = 5555;
    QTimer *embedTimer;
    int embedAttempts = 0;
};
