#include "aimsettings.h"
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QLabel>
#include <QSettings>
#include <QVBoxLayout>

AimSettingsDialog::AimSettingsDialog(QWidget *parent) : QDialog(parent) {
    setWindowTitle("Experimental aim & fire"); resize(540, 390);
    auto *layout = new QVBoxLayout(this);
    auto *title = new QLabel("Experimental aim & fire", this); title->setObjectName("sectionTitle"); layout->addWidget(title);
    auto *note = new QLabel("Positions are percentages of the guest display. Aim drags from the camera area; fire taps the fire button. This uses ONE pointer: aiming pauses while firing, and keyboard mappings cannot run at the same time. Compatibility is not guaranteed.", this);
    note->setWordWrap(true); layout->addWidget(note);
    QSettings s; auto *form = new QFormLayout;
    auto number = [this](double value, double min, double max, const QString &suffix) {
        auto *box = new QDoubleSpinBox(this); box->setRange(min, max); box->setDecimals(1); box->setSingleStep(1.0); box->setSuffix(suffix); box->setValue(value); return box;
    };
    anchorX = number(s.value("aim/anchorX", 50.0).toDouble(), 10, 90, " %");
    anchorY = number(s.value("aim/anchorY", 50.0).toDouble(), 10, 90, " %");
    fireX = number(s.value("aim/fireX", 80.0).toDouble(), 0, 100, " %");
    fireY = number(s.value("aim/fireY", 75.0).toDouble(), 0, 100, " %");
    sensitivity = number(s.value("aim/sensitivity", 1.0).toDouble(), .1, 5, " ×"); sensitivity->setSingleStep(.1);
    form->addRow("Camera area X", anchorX); form->addRow("Camera area Y", anchorY);
    form->addRow("Fire button X", fireX); form->addRow("Fire button Y", fireY); form->addRow("Sensitivity", sensitivity); layout->addLayout(form);
    auto *help = new QLabel("After saving, click Aim & fire in the dock, focus Android and press F8. Press F8 again to release.", this); help->setWordWrap(true); help->setObjectName("muted"); layout->addWidget(help);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel, this); layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        QSettings s; s.setValue("aim/anchorX", anchorX->value()); s.setValue("aim/anchorY", anchorY->value());
        s.setValue("aim/fireX", fireX->value()); s.setValue("aim/fireY", fireY->value()); s.setValue("aim/sensitivity", sensitivity->value()); accept();
    });
}
