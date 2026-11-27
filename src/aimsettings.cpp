#include "aimsettings.h"
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QScrollArea>
#include <QSettings>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {
void populateKeyCombo(QComboBox *box, bool allowNone, int currentKey) {
    if (allowNone) box->addItem(QStringLiteral("Disabled"), 0);
    for (const auto &pair : allBindableKeys()) {
        if (!isMouseBindingKey(pair.first))
            box->addItem(pair.second, pair.first);
    }
    const int idx = box->findData(currentKey);
    if (idx >= 0) box->setCurrentIndex(idx);
}
}

AimSettingsDialog::AimSettingsDialog(QWidget *parent) : QDialog(parent) {
    QSettings s;
    AimConfig initial;
    initial.enabled = s.value("aim/enabled", true).toBool();
    initial.toggleKey = s.value("aim/toggleKey", 0x77).toInt();
    initial.suspendKey = s.value("aim/suspendKey", 0x09).toInt();
    initial.anchorX = s.value("aim/anchorX", 56.0).toDouble() / 100.0;
    initial.anchorY = s.value("aim/anchorY", 48.0).toDouble() / 100.0;
    initial.sensitivityX = s.value("aim/sensitivity", 1.0).toDouble();
    initial.sensitivityY = s.value("aim/sensitivityY", initial.sensitivityX).toDouble();
    initial.fireEnabled = s.value("aim/fireEnabled", true).toBool();
    initial.fireX = s.value("aim/fireX", 82.0).toDouble() / 100.0;
    initial.fireY = s.value("aim/fireY", 74.0).toDouble() / 100.0;
    initial.adsEnabled = s.value("aim/adsEnabled", false).toBool();
    initial.adsHoldMode = s.value("aim/adsHoldMode", false).toBool();
    initial.adsX = s.value("aim/adsX", 90.0).toDouble() / 100.0;
    initial.adsY = s.value("aim/adsY", 56.0).toDouble() / 100.0;
    initial.freeLookEnabled = s.value("aim/freeLookEnabled", false).toBool();
    initial.freeLookKey = s.value("aim/freeLookKey", 0x12).toInt();
    initial.freeLookX = s.value("aim/freeLookX", 76.0).toDouble() / 100.0;
    initial.freeLookY = s.value("aim/freeLookY", 26.0).toDouble() / 100.0;
    initial.showCrosshair = s.value("aim/showCrosshair", true).toBool();
    initial.crosshairStyle = static_cast<CrosshairStyle>(s.value("aim/crosshairStyle", 0).toInt());
    initial.crosshairSize = s.value("aim/crosshairSize", 14).toInt();
    initUi(initial, true);
}

AimSettingsDialog::AimSettingsDialog(const AimConfig &initial, QWidget *parent) : QDialog(parent) {
    initUi(initial, false);
}

void AimSettingsDialog::initUi(const AimConfig &initial, bool persistToQSettings) {
    setWindowTitle("Aim, Pan, Shoot & ADS");
    resize(580, 620);

    auto *root = new QVBoxLayout(this);
    auto *title = new QLabel("Aim, Pan, Shoot & ADS", this);
    title->setObjectName("sectionTitle");
    root->addWidget(title);

    auto *note = new QLabel(
        "Configure shooter camera panning, independent X/Y sensitivity, Left-Click Fire, "
        "Right-Click ADS scope, Free-Look eye camera, cursor suspend key, and crosshair overlay.",
        this);
    note->setWordWrap(true);
    note->setObjectName("muted");
    root->addWidget(note);

    auto *scroll = new QScrollArea(this);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    auto *container = new QWidget(scroll);
    auto *form = new QFormLayout(container);
    form->setVerticalSpacing(11);
    form->setHorizontalSpacing(18);

    auto pctBox = [container](double unitVal, double minPct, double maxPct) {
        auto *b = new QDoubleSpinBox(container);
        b->setRange(minPct, maxPct);
        b->setDecimals(1);
        b->setSingleStep(1.0);
        b->setSuffix(" %");
        b->setValue(unitVal * 100.0);
        return b;
    };
    auto gainBox = [container](double val) {
        auto *b = new QDoubleSpinBox(container);
        b->setRange(0.05, 10.0);
        b->setDecimals(2);
        b->setSingleStep(0.05);
        b->setSuffix(" ×");
        b->setValue(val);
        return b;
    };

    enabledBox = new QCheckBox("Include Aim & Shoot in this control scheme", container);
    enabledBox->setChecked(initial.enabled);
    form->addRow(enabledBox);

    toggleChoice = new QComboBox(container);
    populateKeyCombo(toggleChoice, false, initial.toggleKey);
    form->addRow("Start / Stop key", toggleChoice);

    suspendChoice = new QComboBox(container);
    populateKeyCombo(suspendChoice, true, initial.suspendKey);
    form->addRow("Hold for free cursor", suspendChoice);

    anchorX = pctBox(initial.anchorX, 10.0, 90.0);
    anchorY = pctBox(initial.anchorY, 10.0, 90.0);
    form->addRow("Camera anchor X", anchorX);
    form->addRow("Camera anchor Y", anchorY);

    sensX = gainBox(initial.sensitivityX);
    sensY = gainBox(initial.sensitivityY);
    form->addRow("Mouse sensitivity X", sensX);
    form->addRow("Mouse sensitivity Y", sensY);

    fireBox = new QCheckBox("Left-Click triggers Fire button", container);
    fireBox->setChecked(initial.fireEnabled);
    form->addRow(fireBox);
    fireX = pctBox(initial.fireX, 0.0, 100.0);
    fireY = pctBox(initial.fireY, 0.0, 100.0);
    form->addRow("Fire button X", fireX);
    form->addRow("Fire button Y", fireY);

    adsBox = new QCheckBox("Right-Click triggers ADS / Scope", container);
    adsBox->setChecked(initial.adsEnabled);
    form->addRow(adsBox);
    adsModeChoice = new QComboBox(container);
    adsModeChoice->addItem("Tap scope button on Right-Click", false);
    adsModeChoice->addItem("Hold scope button while Right-Click held", true);
    adsModeChoice->setCurrentIndex(initial.adsHoldMode ? 1 : 0);
    form->addRow("ADS behavior", adsModeChoice);
    adsX = pctBox(initial.adsX, 0.0, 100.0);
    adsY = pctBox(initial.adsY, 0.0, 100.0);
    form->addRow("ADS button X", adsX);
    form->addRow("ADS button Y", adsY);

    freeLookBox = new QCheckBox("Enable Free-Look eye camera while key held", container);
    freeLookBox->setChecked(initial.freeLookEnabled);
    form->addRow(freeLookBox);
    freeLookKeyChoice = new QComboBox(container);
    populateKeyCombo(freeLookKeyChoice, false, initial.freeLookKey == 0 ? 0x12 : initial.freeLookKey);
    form->addRow("Free-Look key", freeLookKeyChoice);
    freeLookX = pctBox(initial.freeLookX, 10.0, 90.0);
    freeLookY = pctBox(initial.freeLookY, 10.0, 90.0);
    form->addRow("Free-Look eye X", freeLookX);
    form->addRow("Free-Look eye Y", freeLookY);

    crosshairBox = new QCheckBox("Show on-screen crosshair when Aim is active", container);
    crosshairBox->setChecked(initial.showCrosshair);
    form->addRow(crosshairBox);
    crosshairStyleChoice = new QComboBox(container);
    crosshairStyleChoice->addItem("Cross + Dot", static_cast<int>(CrosshairStyle::CrossDot));
    crosshairStyleChoice->addItem("Cross", static_cast<int>(CrosshairStyle::Cross));
    crosshairStyleChoice->addItem("Dot", static_cast<int>(CrosshairStyle::Dot));
    crosshairStyleChoice->addItem("Circle", static_cast<int>(CrosshairStyle::Circle));
    crosshairStyleChoice->setCurrentIndex(crosshairStyleChoice->findData(static_cast<int>(initial.crosshairStyle)));
    form->addRow("Crosshair style", crosshairStyleChoice);
    crosshairSizeBox = new QSpinBox(container);
    crosshairSizeBox->setRange(6, 48);
    crosshairSizeBox->setSuffix(" px");
    crosshairSizeBox->setValue(initial.crosshairSize);
    form->addRow("Crosshair size", crosshairSizeBox);

    scroll->setWidget(container);
    root->addWidget(scroll, 1);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel, this);
    root->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(buttons, &QDialogButtonBox::accepted, this, [this, persistToQSettings] {
        if (persistToQSettings) {
            const auto c = resultConfig();
            QSettings s;
            s.setValue("aim/enabled", c.enabled);
            s.setValue("aim/toggleKey", c.toggleKey);
            s.setValue("aim/suspendKey", c.suspendKey);
            s.setValue("aim/anchorX", c.anchorX * 100.0);
            s.setValue("aim/anchorY", c.anchorY * 100.0);
            s.setValue("aim/sensitivity", c.sensitivityX);
            s.setValue("aim/sensitivityY", c.sensitivityY);
            s.setValue("aim/fireEnabled", c.fireEnabled);
            s.setValue("aim/fireX", c.fireX * 100.0);
            s.setValue("aim/fireY", c.fireY * 100.0);
            s.setValue("aim/adsEnabled", c.adsEnabled);
            s.setValue("aim/adsHoldMode", c.adsHoldMode);
            s.setValue("aim/adsX", c.adsX * 100.0);
            s.setValue("aim/adsY", c.adsY * 100.0);
            s.setValue("aim/freeLookEnabled", c.freeLookEnabled);
            s.setValue("aim/freeLookKey", c.freeLookKey);
            s.setValue("aim/freeLookX", c.freeLookX * 100.0);
            s.setValue("aim/freeLookY", c.freeLookY * 100.0);
            s.setValue("aim/showCrosshair", c.showCrosshair);
            s.setValue("aim/crosshairStyle", static_cast<int>(c.crosshairStyle));
            s.setValue("aim/crosshairSize", c.crosshairSize);
        }
        accept();
    });
}

AimConfig AimSettingsDialog::resultConfig() const {
    AimConfig c;
    c.enabled = enabledBox->isChecked();
    c.toggleKey = toggleChoice->currentData().toInt();
    c.suspendKey = suspendChoice->currentData().toInt();
    c.anchorX = anchorX->value() / 100.0;
    c.anchorY = anchorY->value() / 100.0;
    c.sensitivityX = sensX->value();
    c.sensitivityY = sensY->value();
    c.fireEnabled = fireBox->isChecked();
    c.fireX = fireX->value() / 100.0;
    c.fireY = fireY->value() / 100.0;
    c.adsEnabled = adsBox->isChecked();
    c.adsHoldMode = adsModeChoice->currentData().toBool();
    c.adsX = adsX->value() / 100.0;
    c.adsY = adsY->value() / 100.0;
    c.freeLookEnabled = freeLookBox->isChecked();
    c.freeLookKey = freeLookKeyChoice->currentData().toInt();
    c.freeLookX = freeLookX->value() / 100.0;
    c.freeLookY = freeLookY->value() / 100.0;
    c.showCrosshair = crosshairBox->isChecked();
    c.crosshairStyle = static_cast<CrosshairStyle>(crosshairStyleChoice->currentData().toInt());
    c.crosshairSize = crosshairSizeBox->value();
    return c;
}
