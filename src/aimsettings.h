#pragma once
#include <QDialog>
class QDoubleSpinBox;
class AimSettingsDialog : public QDialog {
public:
    explicit AimSettingsDialog(QWidget *parent = nullptr);
private:
    QDoubleSpinBox *anchorX;
    QDoubleSpinBox *anchorY;
    QDoubleSpinBox *fireX;
    QDoubleSpinBox *fireY;
    QDoubleSpinBox *sensitivity;
};
