#pragma once
#include <QMainWindow>
#include "keymap.h"
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
class EmulatorView;
class Qmp;
class InputMapper;
class MouseLock;
class QCheckBox;
class QLabel;
class QProcess;
class QPushButton;
class QTimer;
class QCloseEvent;

class MainWindow : public QMainWindow {
public:
    explicit MainWindow(QWidget *parent = nullptr);
protected:
    void closeEvent(QCloseEvent *event) override;
private:
    void launch();
    void findGuestWindow();
    void editKeymap();
    void updateControls();
    void setRunning(bool running);
    EmulatorView *view;
    QCheckBox *keymapEnabled;
    QPushButton *editKeymapButton;
    QPushButton *fullscreenButton;
    QPushButton *mouseLockButton;
    QPushButton *launchButton;
    QPushButton *shutdownButton;
    QLabel *vmStatus;
    Qmp *qmp;
    InputMapper *mapper;
    MouseLock *mouseLock;
    QList<KeyBinding> bindings;
    QString keymapPath;
    QProcess *process;
    QTimer *embedTimer;
    int embedAttempts = 0;
};
