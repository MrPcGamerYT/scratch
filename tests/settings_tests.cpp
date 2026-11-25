#include "settingsdialog.h"
#include "windowchrome.h"
#include <QApplication>
#include <QComboBox>
#include "runtimeconfig.h"
#include "guesttelemetry.h"
#include "diagnostics.h"
#include "apkdrop.h"
#include "resourceconfig.h"
#include <QCheckBox>
#include <QPlainTextEdit>
#include <QFile>
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
    if (foregroundPackage("mResumedActivity: ActivityRecord{u0 com.example.game/.Main t1}") != "com.example.game") return 80;
    if (!foregroundPackage("background: com.example.game/.Main").isEmpty()) return 81;
    if (compositorFps("100.0 10.0\nNEXUS_LATENCY\n16666666\n1 99500000000 3\n1 99500000000 3\n1 99000000001 3\n1 9223372036854775807 3\n") != "Display FPS: 2") return 82;
    if (compositorFps("100.0 10.0\nNEXUS_LATENCY\n16666666\n1 1000 3\n") != "Display FPS: N/A") return 83;
    if (bootDisplayWaitExpired(-1) || bootDisplayWaitExpired(179999) || !bootDisplayWaitExpired(180000) || !bootDisplayWaitExpired(3600000)) return 86;
    QSettings().setValue("cpus", 4); QSettings().setValue("memory", 4096); // existing manual user
    WindowChrome chrome;
    app.installEventFilter(&chrome);
    SettingsDialog dialog(false);
    dialog.show(); app.processEvents();
    if (!dialog.windowFlags().testFlag(Qt::FramelessWindowHint)) return 9;
    auto *tabs = dialog.findChild<QTabWidget *>();
    auto *cpu = dialog.findChild<QSpinBox *>("cpuAllocation");
    auto *save = dialog.findChild<QPushButton *>("saveSettings");
    QPushButton *discard = nullptr;
    for (auto *button : dialog.findChildren<QPushButton *>()) if (button->text() == "Discard") discard = button;
    if (!tabs || tabs->count() != 13 || !cpu || !save || !discard) return 2;
    if (save->isEnabled() || discard->isEnabled() || cpu->value() != 4) return 3;
    auto *fps = dialog.findChild<QCheckBox *>("showFps");
    auto *hideBoot = dialog.findChild<QCheckBox *>("hideBoot");
    if (!fps || !hideBoot || fps->isChecked() || !hideBoot->isChecked()) return 84;
    hideBoot->setChecked(false); save->click();
    if (QSettings().value("hideBoot").toBool()) return 87;
    SettingsDialog savedBootPreference(false);
    if (savedBootPreference.findChild<QCheckBox *>("hideBoot")->isChecked()) return 88;
    fps->setChecked(true); hideBoot->setChecked(true); save->click();
    if (!QSettings().value("showFps").toBool() || !QSettings().value("hideBoot").toBool()) return 85;
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
    auto *accel = dialog.findChild<QComboBox *>("vmAcceleration");
    if (!accel || accel->count() != 3) return 10;
    accel->setCurrentIndex(accel->findData("tcg")); save->click();
    if (selectedVmAcceleration(QSettings()) != "tcg") return 11;
    accel->setCurrentIndex(accel->findData("whpx")); discard->click();
    if (accel->currentData().toString() != "tcg") return 12;
    auto *resolution = reopened.findChild<QComboBox *>("guestResolution");
    auto *density = reopened.findChild<QComboBox *>("guestDensity");
    auto *apply = reopened.findChild<QPushButton *>("applyGuestDisplay");
    if (!resolution || !density || !apply || !apply->isEnabled()) return 13;
    bool applied = false;
    reopened.applyDisplay = [&](QString size, int dpi) { applied = size == "1920x1080" && dpi == 240; };
    resolution->setCurrentIndex(resolution->findData("1920x1080"));
    density->setCurrentIndex(density->findData(240)); apply->click();
    if (!applied || dialog.findChild<QPushButton *>("applyGuestDisplay")->isEnabled()) return 14;
    applied = false;
    reopened.applyDisplay = [&](QString size, int dpi) { applied = size == "reset" && dpi == 0; };
    resolution->setCurrentIndex(0); density->setCurrentIndex(0); apply->click();
    if (!applied) return 15;
    auto *report = reopened.findChild<QPlainTextEdit *>("diagnosticsReport");
    auto *refresh = reopened.findChild<QPushButton *>("refreshDiagnostics");
    if (!report || !report->isReadOnly() || !refresh) return 16;
    reopened.sessionDiagnostics = [] { return QString("test-session-event"); };
    refresh->click();
    if (!report->toPlainText().contains("test-session-event")) return 17;
    const auto missing = playerDiagnostics(tmp.path());
    if (!missing.contains("MISSING/EMPTY") || !missing.contains("legacy BIOS")) return 18;
    QFile emptyFirmware(tmp.path() + "/images/android-uefi-code.fd");
    QDir().mkpath(tmp.path() + "/images"); emptyFirmware.open(QIODevice::WriteOnly); emptyFirmware.close();
    if (!playerDiagnostics(tmp.path()).contains("incomplete/empty UEFI")) return 19;
    if (dialog.findChild<QCheckBox *>("audioOutput")) return 20; // automatic, no user toggle
    auto *inputMode = dialog.findChild<QComboBox *>("mouseInputBackend");
    auto *batch = dialog.findChild<QComboBox *>("aimBatchInterval");
    if (!inputMode || !batch || inputMode->currentData().toBool() || batch->currentData().toInt() != 16) return 28;
    inputMode->setCurrentIndex(1); batch->setCurrentIndex(batch->findData(8)); discard->click();
    if (inputMode->currentData().toBool() || batch->currentData().toInt() != 16) return 29;
    inputMode->setCurrentIndex(1); batch->setCurrentIndex(batch->findData(4)); save->click();
    if (!QSettings().value("input/rawMouse").toBool() || QSettings().value("input/aimInterval").toInt() != 4) return 30;
    auto *graphics = dialog.findChild<QComboBox *>("vmGraphics");
    if (!graphics || graphics->count() != 4 || selectedVmGraphics(QSettings()) != "std") return 31;
    graphics->setCurrentIndex(graphics->findData("virtio")); save->click();
    if (selectedVmGraphics(QSettings()) != "virtio") return 32;
    graphics->setCurrentIndex(graphics->findData("std")); discard->click();
    if (graphics->currentData().toString() != "virtio") return 33;
    QMimeData mime;
    mime.setUrls({QUrl("https://example.invalid/test.apk")});
    if (!droppedApk(&mime).isEmpty()) return 23;
    QFile apk(tmp.path() + "/trusted.APK"); apk.open(QIODevice::WriteOnly); apk.write("test"); apk.close();
    mime.setUrls({QUrl::fromLocalFile(apk.fileName())});
    if (droppedApk(&mime).isEmpty()) return 24;
    mime.setUrls({QUrl::fromLocalFile(apk.fileName()), QUrl::fromLocalFile(apk.fileName())});
    if (!droppedApk(&mime).isEmpty()) return 25;
    mime.setUrls({QUrl::fromLocalFile(tmp.path())});
    if (!droppedApk(&mime).isEmpty()) return 26;
    QFile emptyApk(tmp.path() + "/empty.apk"); emptyApk.open(QIODevice::WriteOnly); emptyApk.close();
    mime.setUrls({QUrl::fromLocalFile(emptyApk.fileName())});
    if (!droppedApk(&mime).isEmpty()) return 27;
    auto *automatic = dialog.findChild<QCheckBox *>("automaticResources");
    if (!automatic || automatic->isChecked() || !cpu->isEnabled()) return 34;
    automatic->setChecked(true);
    if (cpu->isEnabled() || !save->isEnabled()) return 35;
    discard->click(); if (automatic->isChecked() || !cpu->isEnabled()) return 36;
    automatic->setChecked(true); save->click();
    if (!automaticResources(QSettings())) return 37;
    const auto expected = nexus_resource_budget(qMax(1, QThread::idealThreadCount()), nexus_host_memory_mb());
    if (selectedResources(QSettings()).cpus != expected.cpus || selectedResources(QSettings()).memory_mb != expected.memory_mb) return 38;
    auto *gpuReport = dialog.findChild<QPlainTextEdit *>("hostGpuReport");
    if (!gpuReport || !gpuReport->isReadOnly() || !gpuReport->toPlainText().contains("DXGI")) return 39;
    for (int i = 0; i < tabs->count(); ++i) tabs->setCurrentIndex(i);
    std::cout << "Settings categories, dirty state, discard and persistence passed.\n";
    return 0;
}
