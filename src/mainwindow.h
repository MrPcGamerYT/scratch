#pragma once

#include <QMainWindow>
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

class EmulatorView;
class QCheckBox;
class QComboBox;
class QLineEdit;
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
    QPushButton *downloadButton;
    QPushButton *launchButton;
    QPushButton *shutdownButton;
    QProcess *process;
    QTimer *embedTimer;
    int embedAttempts = 0;
};
