#include "settingsdialog.h"
#include <QApplication>
#include <QPushButton>
#include <QSettings>
#include <QSpinBox>
#include <QTabWidget>
#include <QTemporaryDir>
#include <iostream>
int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QTemporaryDir tmp;
    if (!tmp.isValid()) return 1;
    QCoreApplication::setOrganizationName("NexusSettingsTests");
    QCoreApplication::setApplicationName("Isolated");
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, tmp.path());
    SettingsDialog dialog(false);
    auto *tabs = dialog.findChild<QTabWidget *>();
    auto *cpu = dialog.findChild<QSpinBox *>("cpuAllocation");
    auto *save = dialog.findChild<QPushButton *>("saveSettings");
    QPushButton *discard = nullptr;
    for (auto *button : dialog.findChildren<QPushButton *>()) if (button->text() == "Discard") discard = button;
    if (!tabs || tabs->count() != 12 || !cpu || !save || !discard) return 2;
    if (save->isEnabled() || discard->isEnabled() || cpu->value() != 4) return 3;
    cpu->setValue(6);
    if (!save->isEnabled() || !discard->isEnabled()) return 4;
    discard->click();
    if (cpu->value() != 4 || save->isEnabled()) return 5;
    cpu->setValue(8); save->click();
    if (QSettings().value("cpus").toInt() != 8 || save->isEnabled()) return 6;
    cpu->setValue(2); discard->click();
    if (cpu->value() != 8) return 7;
    SettingsDialog reopened(true);
    if (reopened.findChild<QSpinBox *>("cpuAllocation")->value() != 8) return 8;
    for (int i = 0; i < tabs->count(); ++i) tabs->setCurrentIndex(i);
    std::cout << "Settings categories, dirty state, discard and persistence passed.\n";
    return 0;
}
