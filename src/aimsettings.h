#pragma once
#include "keymap.h"
#include <QDialog>

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QSpinBox;

class AimSettingsDialog : public QDialog {
public:
    explicit AimSettingsDialog(const AimConfig &initial, QWidget *parent = nullptr);
    explicit AimSettingsDialog(QWidget *parent = nullptr);
    AimConfig resultConfig() const;
private:
    void initUi(const AimConfig &initial, bool persistToQSettings);
    QCheckBox *enabledBox = nullptr;
    QComboBox *toggleChoice = nullptr;
    QComboBox *suspendChoice = nullptr;
    QDoubleSpinBox *anchorX = nullptr;
    QDoubleSpinBox *anchorY = nullptr;
    QDoubleSpinBox *sensX = nullptr;
    QDoubleSpinBox *sensY = nullptr;
    QCheckBox *fireBox = nullptr;
    QDoubleSpinBox *fireX = nullptr;
    QDoubleSpinBox *fireY = nullptr;
    QCheckBox *adsBox = nullptr;
    QComboBox *adsModeChoice = nullptr;
    QDoubleSpinBox *adsX = nullptr;
    QDoubleSpinBox *adsY = nullptr;
    QCheckBox *freeLookBox = nullptr;
    QComboBox *freeLookKeyChoice = nullptr;
    QDoubleSpinBox *freeLookX = nullptr;
    QDoubleSpinBox *freeLookY = nullptr;
    QCheckBox *crosshairBox = nullptr;
    QComboBox *crosshairStyleChoice = nullptr;
    QSpinBox *crosshairSizeBox = nullptr;
};
