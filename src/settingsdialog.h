#pragma once
#include <QDialog>
#include <functional>
class QSpinBox;
class QComboBox;
class QCheckBox;
class SettingsDialog : public QDialog {
public:
    explicit SettingsDialog(bool running, QWidget *parent = nullptr);
    void reject() override;
    std::function<void(QString, int)> applyDisplay;
    std::function<QString()> sessionDiagnostics;
private:
    std::function<bool()> hasChanges;
    bool save();
    QComboBox *acceleration;
    QComboBox *graphics;
    QComboBox *mouseInput;
    QComboBox *aimInterval;
    QSpinBox *memory;
    QSpinBox *cpus;
    QSpinBox *adbPort;
    QCheckBox *showFps;
    QCheckBox *hideBoot;
    QCheckBox *showCursor;
    QCheckBox *adbForward;
    QCheckBox *multitouch;
    QCheckBox *autoResources;
};
