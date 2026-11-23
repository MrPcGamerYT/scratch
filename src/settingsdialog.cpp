#include "settingsdialog.h"
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QSettings>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace {
QWidget *page(const QString &title, const QString &subtitle, QFormLayout **form) {
    auto *widget = new QWidget;
    auto *layout = new QVBoxLayout(widget); layout->setContentsMargins(26, 10, 20, 10); layout->setSpacing(14);
    auto *heading = new QLabel(title, widget); heading->setObjectName("sectionTitle"); layout->addWidget(heading);
    auto *detail = new QLabel(subtitle, widget); detail->setObjectName("muted"); detail->setWordWrap(true); layout->addWidget(detail);
    *form = new QFormLayout; (*form)->setVerticalSpacing(18); (*form)->setHorizontalSpacing(25); layout->addLayout(*form); layout->addStretch();
    return widget;
}
QComboBox *unavailable(const QString &value) {
    auto *box = new QComboBox; box->addItem(value); box->setEnabled(false); return box;
}
}

SettingsDialog::SettingsDialog(bool running, QWidget *parent) : QDialog(parent) {
    setWindowTitle("Settings · Project Nexus"); resize(760, 600); setMinimumSize(650, 520);
    QSettings settings;
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(24, 22, 24, 20);
    auto *heading = new QLabel("Settings", this); heading->setObjectName("dialogTitle"); layout->addWidget(heading);
    auto *description = new QLabel(running ? "Changes take effect the next time you start the player." : "Make your workspace feel right.", this);
    description->setObjectName("muted"); layout->addWidget(description);
    auto *content = new QHBoxLayout; content->setContentsMargins(0, 18, 0, 0);
    auto *navigation = new QListWidget(this); navigation->setObjectName("settingsNav"); navigation->setFixedWidth(155);
    navigation->addItems({"Performance", "Display", "Graphics", "Device", "Preferences"});
    auto *stack = new QStackedWidget(this);
    QFormLayout *form = nullptr;
    auto *performance = page("Performance", "Choose how much of your PC the virtual machine can use.", &form);
    cpus = new QSpinBox(performance); cpus->setRange(1, 64); cpus->setValue(settings.value("cpus", 4).toInt());
    memory = new QSpinBox(performance); memory->setRange(512, 131072); memory->setSingleStep(512); memory->setSuffix(" MB"); memory->setValue(settings.value("memory", 4096).toInt());
    form->addRow("CPU cores", cpus); form->addRow("Memory", memory);
    auto *note = new QLabel("Restart the player after changing performance settings.", performance); note->setObjectName("muted"); note->setWordWrap(true); form->addRow(note); stack->addWidget(performance);
    auto *display = page("Display", "Window and cursor preferences. Guest display controls require Android integration.", &form);
    form->addRow("Resolution", unavailable("Managed by Android · not available yet"));
    form->addRow("Pixel density", unavailable("Managed by Android · not available yet"));
    auto *fps = new QCheckBox("Show FPS at bottom left", display); fps->setEnabled(false); fps->setToolTip("Live guest FPS measurement is not implemented yet."); form->addRow(fps);
    auto *fpsNote = new QLabel("Live game FPS measurement is not available yet.", display); fpsNote->setObjectName("muted"); form->addRow(fpsNote);
    showCursor = new QCheckBox("Show host mouse cursor", display); showCursor->setChecked(settings.value("showCursor", true).toBool()); form->addRow(showCursor); stack->addWidget(display);
    auto *graphics = page("Graphics", "The current player uses QEMU's compatibility display.", &form);
    form->addRow("Renderer", unavailable("Standard VGA · compatibility"));
    form->addRow("GPU acceleration", unavailable("Not available in this build"));
    auto *graphicsNote = new QLabel("No GPU passthrough, DirectX acceleration or FPS limiter is configured.", graphics); graphicsNote->setObjectName("muted"); graphicsNote->setWordWrap(true); form->addRow(graphicsNote); stack->addWidget(graphics);
    auto *device = page("Device", "Device profiles require support from the Android system image.", &form);
    form->addRow("Phone profile", unavailable("Not available in this build")); stack->addWidget(device);
    auto *preferences = page("Preferences", "Startup and debugging options.", &form);
    multitouch = new QCheckBox("Experimental multi-touch keymapping", preferences); multitouch->setChecked(settings.value("multiTouch", false).toBool()); form->addRow(multitouch);
    auto *touchNote = new QLabel("Requires a guest with virtio multitouch support; restart required. Not verified with Bliss OS.", preferences); touchNote->setObjectName("muted"); touchNote->setWordWrap(true); form->addRow(touchNote);
    adbForward = new QCheckBox("Enable local ADB forwarding", preferences); adbForward->setChecked(settings.value("adbForward", false).toBool()); form->addRow(adbForward);
    adbPort = new QSpinBox(preferences); adbPort->setRange(1024, 65535); adbPort->setValue(settings.value("adbPort", 5555).toInt()); form->addRow("Local ADB port", adbPort);
    auto *adbNote = new QLabel("Local forwarding does not enable ADB inside Android.", preferences); adbNote->setObjectName("muted"); adbNote->setWordWrap(true); form->addRow(adbNote); stack->addWidget(preferences);
    content->addWidget(navigation); content->addWidget(stack, 1); layout->addLayout(content, 1);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel | QDialogButtonBox::Save, this); layout->addWidget(buttons);
    connect(navigation, &QListWidget::currentRowChanged, stack, &QStackedWidget::setCurrentIndex);
    navigation->setCurrentRow(0);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] { save(); accept(); });
}
void SettingsDialog::save() {
    QSettings s; s.setValue("cpus", cpus->value()); s.setValue("memory", memory->value());
    s.setValue("showCursor", showCursor->isChecked());
    s.setValue("multiTouch", multitouch->isChecked());
    s.setValue("adbForward", adbForward->isChecked()); s.setValue("adbPort", adbPort->value());
}
