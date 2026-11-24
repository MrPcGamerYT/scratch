#pragma once
#include <QDialog>
#include <functional>
class QSpinBox;
class QCheckBox;
class SettingsDialog : public QDialog {
public:
    explicit SettingsDialog(bool running, QWidget *parent = nullptr);
    void reject() override;
private:
    std::function<bool()> hasChanges;
    bool save();
    QSpinBox *memory;
    QSpinBox *cpus;
    QSpinBox *adbPort;
    QCheckBox *showCursor;
    QCheckBox *adbForward;
    QCheckBox *multitouch;
};
