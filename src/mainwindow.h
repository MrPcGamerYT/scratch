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
class QComboBox;
class QLineEdit;
class QLabel;
class QFrame;
class QProcess;
class QPushButton;
class QSpinBox;
class QTimer;
class QCloseEvent;

class MainWindow : public QMainWindow {
public:
    explicit MainWindow(QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;

private:
    void launch();
    void downloadAndroid();
    void findGuestWindow();
    void editKeymap();
    void updateKeymapControls();
    void setRunning(bool running);
    void saveSettings();
    void loadSettings();

    EmulatorView *view;
    QLineEdit *qemuPath;
    QLineEdit *isoPath;
    QLineEdit *diskPath;
    QSpinBox *memory;
    QSpinBox *cpus;
    QSpinBox *adbPort;
    QComboBox *boot;
    QCheckBox *adbForward;
    QCheckBox *showCursor;
    QCheckBox *autoStart;
    QFrame *settingsPanel;
    QCheckBox *keymapEnabled;
    QPushButton *editKeymapButton;
    QPushButton *fullscreenButton;
    QPushButton *mouseLockButton;
    MouseLock *mouseLock;
    QLabel *vmStatus;
    Qmp *qmp;
    InputMapper *mapper;
    QList<KeyBinding> bindings;
    QString keymapPath;
    QPushButton *downloadButton;
    QPushButton *launchButton;
    QPushButton *shutdownButton;
    QProcess *process;
    QTimer *embedTimer;
    int embedAttempts = 0;
};
