#pragma once
#include <QDialog>
class QSpinBox;
class QCheckBox;

class SettingsDialog : public QDialog {
public:
    explicit SettingsDialog(bool running, QWidget *parent = nullptr);
private:
    void save();
    QSpinBox *memory;
    QSpinBox *cpus;
    QSpinBox *adbPort;
    QCheckBox *showCursor;
    QCheckBox *autoStart;
    QCheckBox *adbForward;
};
