#include "mainwindow.h"
#include "aimcontroller.h"
#include "emulatorview.h"
#include "inputmapper.h"
#include "keymapdialog.h"
#include "keymapoverlay.h"
#include "manageddisk.h"
#include "mouselock.h"
#include "qmp.h"
#include "settingsdialog.h"
#include "rustcore.h"
#include "runtimeconfig.h"
#include "resourceconfig.h"
#include "guesttelemetry.h"
#include "fpsbadge.h"
#include <QApplication>
#include <QCryptographicHash>
#include <QJsonDocument>
#include <QJsonArray>
#include <QCheckBox>
#include <QCloseEvent>
#include <QCoreApplication>
#include <QDir>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QMimeData>
#include "apkdrop.h"
#include <QDateTime>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QEvent>
#include <QShortcut>
#include <QScopeGuard>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QLabel>
#include <QMessageBox>
#include <QMenu>
#include <QWidgetAction>
#include <QProcess>
#include <QPushButton>
#include <QScreen>
#include <QSettings>
#include <QSignalBlocker>
#include <QStandardPaths>
#include <QStatusBar>
#include <QTimer>
#include <QVBoxLayout>
#include <QVariant>

namespace {
struct WindowSearch { DWORD pid; HWND result = nullptr; long long area = 0; };
BOOL CALLBACK findWindow(HWND hwnd, LPARAM data) {
    auto *search = reinterpret_cast<WindowSearch *>(data);
    DWORD pid = 0; GetWindowThreadProcessId(hwnd, &pid);
    if (pid != search->pid || !IsWindowVisible(hwnd) || GetWindow(hwnd, GW_OWNER)) return TRUE;
    RECT rect{}; if (!GetClientRect(hwnd, &rect)) return TRUE;
    const long long area = static_cast<long long>(rect.right - rect.left) * (rect.bottom - rect.top);
    if (area > search->area) { search->area = area; search->result = hwnd; }
    return TRUE;
}
QPushButton *dockButton(const QString &label, QWidget *parent) {
    auto *button = new QPushButton(label, parent);
    button->setText(label.section("  ", 0, 0)); button->setFixedSize(30, 28);
    button->setObjectName("dockButton"); button->setToolTip(label); button->setAccessibleName(label);
    return button;
}
}

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent), process(new QProcess(this)), embedTimer(new QTimer(this)) {
    qmp = new Qmp(process, this);
    mapper = new InputMapper(qmp, this);
    mouseLock = new MouseLock(this);
    aim = new AimController(qmp, this);
    setProperty("nexusPlayer", true); setAcceptDrops(true);
    setWindowTitle("Project Nexus"); resize(1400, 840); setMinimumSize(1100, 600);
    auto *central = new QWidget(this); auto *root = new QVBoxLayout(central); root->setContentsMargins(0, 0, 0, 0); root->setSpacing(0);
    auto *header = new QFrame(central); header->setObjectName("header");
    auto *headerLayout = new QHBoxLayout(header); headerLayout->setContentsMargins(18, 9, 18, 9); headerLayout->setSpacing(8);
    auto *mark = new QLabel("N", header); mark->setObjectName("logoMark"); mark->setAlignment(Qt::AlignCenter); mark->setFixedSize(34, 34); headerLayout->addWidget(mark);
    auto *brand = new QLabel("NEXUS", header); brand->setObjectName("brand"); headerLayout->addWidget(brand);
    headerLayout->addStretch();
    auto *dock = new QFrame(header); dock->setObjectName("topDock"); auto *dockLayout = new QHBoxLayout(dock); dockLayout->setContentsMargins(4, 0, 4, 0); dockLayout->setSpacing(4);
    for (const auto &entry : {qMakePair(QString("◁  Back"), 4), qMakePair(QString("⌂  Home"), 3), qMakePair(QString("▢  Recent apps"), 187)}) {
        auto *button = dockButton(entry.first, dock); dockLayout->addWidget(button);
        connect(button, &QPushButton::clicked, this, [this, code = entry.second] { androidAction(code); });
    }
    fullscreenButton = dockButton("⛶  Full screen", dock); dockLayout->addWidget(fullscreenButton);
    editKeymapButton = dockButton("⌨  Keymap controls", dock); dockLayout->addWidget(editKeymapButton);
    auto *keyMenu = new QMenu(editKeymapButton); editKeymapButton->setMenu(keyMenu);
    auto *editAction = keyMenu->addAction("Open Keymap Studio");
    connect(editAction, &QAction::triggered, this, [this] { editKeymap(); });
    auto addCheck = [&](const QString &text) {
        auto *check = new QCheckBox(text, keyMenu); auto *action = new QWidgetAction(keyMenu);
        action->setDefaultWidget(check); keyMenu->addAction(action); return check;
    };
    keymapEnabled = addCheck("Enable keymap"); keymapEnabled->setAccessibleName("Enable keymap");
    overlayEnabled = addCheck("Show keymap hints"); overlayEnabled->setAccessibleName("Show keymap hints");
    mouseLockButton = dockButton("◎  Lock mouse", keyMenu);
    auto *lockAction = new QWidgetAction(keyMenu); lockAction->setDefaultWidget(mouseLockButton); keyMenu->addAction(lockAction);
    aimButton = dockButton("⌖  Aim & fire", header); aimButton->hide();
    auto *aimAction = keyMenu->addAction("Arm / disarm aim (configure in Keymap Studio)");
    connect(aimAction, &QAction::triggered, this, [this] { aimButton->click(); });
    auto *volume = dockButton("♫  Volume", dock); dockLayout->addWidget(volume);
    auto *volumeMenu = new QMenu(volume); volume->setMenu(volumeMenu);
    for (const auto &entry : {qMakePair(QString("Volume up"), 24), qMakePair(QString("Volume down"), 25), qMakePair(QString("Toggle mute"), 164)}) {
        auto *action = volumeMenu->addAction(entry.first);
        connect(action, &QAction::triggered, this, [this, code = entry.second] { androidAction(code); });
    }
    auto *cleanButton = dockButton("♻  Clean background memory", dock); dockLayout->addWidget(cleanButton);
    cleanButton->setToolTip("Ask Android to stop eligible background processes and trim Nexus/QEMU working sets. May briefly slow apps; not a performance boost guarantee.");
    connect(cleanButton, &QPushButton::clicked, this, [this] {
        if (QMessageBox::question(this, "Clean background memory", "Ask Android to stop eligible background processes and trim this player's and QEMU's Windows working sets?\n\nApps may reload and memory can grow again. This does not clear app data, force-stop every app, or reduce configured VM RAM. Trimming can cause page faults and temporarily worsen smoothness.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes) androidAction(-1);
    });
    auto *apkButton = dockButton("＋  Install APK", dock); dockLayout->addWidget(apkButton);
    auto *settingsButton = dockButton("⚙  Settings", dock); dockLayout->addWidget(settingsButton);
    headerLayout->addWidget(dock); headerLayout->addStretch();
    vmStatus = new QLabel("●  OFFLINE", header); vmStatus->setObjectName("statusPill"); headerLayout->addWidget(vmStatus);
    launchButton = dockButton("Retry", header); launchButton->setToolTip("Retry starting the prepared Android system"); headerLayout->addWidget(launchButton);
    shutdownButton = dockButton("Stop", header); headerLayout->addWidget(shutdownButton);
    header->hide(); // Dock is adopted by the single custom title bar.
    statusBar()->hide();

    auto *viewport = new QFrame(central); viewport->setObjectName("viewportPanel"); auto *v = new QVBoxLayout(viewport); v->setContentsMargins(0, 0, 0, 0);
    view = new EmulatorView(viewport); v->addWidget(view, 1); root->addWidget(viewport, 1); setCentralWidget(central);

    overlay = new KeymapOverlay(view, this);

    QSettings appSettings;
    profileDir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/keymaps";
    globalProfileDir = profileDir;
    QDir().mkpath(profileDir);
    activeScheme = sanitizeSchemeName(appSettings.value("keymap/activeScheme", "default").toString());
    keymapPath = schemeFilePath(profileDir, activeScheme);
    QString error;
    if (!loadKeymapProfile(keymapPath, &profile, &error)) {
        statusBar()->showMessage("Keymap could not be loaded: " + error);
    } else {
        statusBar()->showMessage(QStringLiteral("Ready to play  ·  Active scheme: %1").arg(profile.name));
    }
    overlay->setProfile(profile);
    overlayEnabled->setChecked(appSettings.value("keymap/showHints", false).toBool());
    overlay->setHintsVisible(overlayEnabled->isChecked());

    setRunning(false);
    connect(launchButton, &QPushButton::clicked, this, [this] { launch(); });
    connect(shutdownButton, &QPushButton::clicked, this, [this] { qmp->powerDown(); statusBar()->showMessage("Shutdown requested. If Android ignores it, shut down inside the guest."); });
    connect(apkButton, &QPushButton::clicked, this, [this] { installApk(); });
    connect(settingsButton, &QPushButton::clicked, this, [this] { SettingsDialog dialog(process->state() != QProcess::NotRunning, this); dialog.sessionDiagnostics = [this] { return QString("Process: %1; QMP: %2; display embedded: %3\n").arg(process->state() == QProcess::Running ? "running" : "stopped").arg(qmp->isReady() ? "ready" : "not ready").arg(view->windowHandle() ? "yes" : "no") + aim->inputDiagnostics() + "\n\n" + sessionLog; }; dialog.applyDisplay = [this](QString size, int dpi) { applyGuestDisplay(size, dpi); }; dialog.exec(); });
    auto toggleFullscreen = [this] { if (isFullScreen()) showNormal(); else showFullScreen(); };
    connect(fullscreenButton, &QPushButton::clicked, this, toggleFullscreen);
    auto *fullscreenShortcut = new QShortcut(QKeySequence(Qt::Key_F11), this);
    fullscreenShortcut->setContext(Qt::WindowShortcut);
    connect(fullscreenShortcut, &QShortcut::activated, this, toggleFullscreen);
    connect(overlayEnabled, &QCheckBox::toggled, this, [this](bool shown) {
        QSettings s; s.setValue("keymap/showHints", shown);
        overlay->setHintsVisible(shown);
    });
    connect(aimButton, &QPushButton::clicked, this, [this] {
        if (aim->isArmed()) {
            aim->disarm();
            mapper->setReservedTouchSlots({});
            return;
        }
        mouseLock->unlock();
        if (!runningMultiTouch) keymapEnabled->setChecked(false);
        else mapper->setReservedTouchSlots({0, 1, 2});

        AimConfig cfg = profile.aim;
        if (!cfg.enabled) {
            QSettings s;
            cfg.enabled = true;
            cfg.toggleKey = s.value("aim/toggleKey", 0x77).toInt();
            cfg.suspendKey = s.value("aim/suspendKey", 0x09).toInt();
            cfg.anchorX = s.value("aim/anchorX", 56.0).toDouble() / 100.0;
            cfg.anchorY = s.value("aim/anchorY", 48.0).toDouble() / 100.0;
            cfg.sensitivityX = s.value("aim/sensitivity", 1.0).toDouble();
            cfg.sensitivityY = s.value("aim/sensitivityY", cfg.sensitivityX).toDouble();
            cfg.fireEnabled = s.value("aim/fireEnabled", true).toBool();
            cfg.fireX = s.value("aim/fireX", 82.0).toDouble() / 100.0;
            cfg.fireY = s.value("aim/fireY", 74.0).toDouble() / 100.0;
            cfg.adsEnabled = s.value("aim/adsEnabled", false).toBool();
            cfg.adsHoldMode = s.value("aim/adsHoldMode", false).toBool();
            cfg.adsX = s.value("aim/adsX", 90.0).toDouble() / 100.0;
            cfg.adsY = s.value("aim/adsY", 56.0).toDouble() / 100.0;
            cfg.freeLookEnabled = s.value("aim/freeLookEnabled", false).toBool();
            cfg.freeLookKey = s.value("aim/freeLookKey", 0x12).toInt();
            cfg.freeLookX = s.value("aim/freeLookX", 76.0).toDouble() / 100.0;
            cfg.freeLookY = s.value("aim/freeLookY", 26.0).toDouble() / 100.0;
        }
        if (!aim->arm(view->windowHandle(), cfg, runningMultiTouch)) {
            mapper->setReservedTouchSlots({});
            QMessageBox::warning(this, "Aim & fire", "Could not arm aim mode. The embedded player and QEMU monitor must be ready.");
        }
    });
    connect(aim, &AimController::stateChanged, this, [this](bool armed, bool activeState, bool suspended, const QString &message) {
        aimButton->setText(armed ? "⌖  Disarm aim" : "⌖  Aim & fire");
        mapper->setAimCapturingMouse(activeState && !suspended);
        if (!armed) mapper->setReservedTouchSlots({});
        overlay->setAimCrosshairActive(activeState && !suspended);
        statusBar()->showMessage(message);
        updateControls();
    });
    connect(mouseLockButton, &QPushButton::clicked, this, [this] {
        if (mouseLock->isLocked()) { mouseLock->unlock(); return; }
        if (!mouseLock->lock(view->windowHandle())) QMessageBox::warning(this, "Mouse lock", "Could not lock the mouse to the player display.");
        else { mouseLockButton->setText("◎  Unlock"); statusBar()->showMessage("Mouse locked. Press Ctrl+Alt+L to unlock; switching apps also releases it."); }
    });
    connect(mouseLock, &MouseLock::unlocked, this, [this] { mouseLockButton->setText("◎  Lock"); statusBar()->showMessage("Mouse unlocked."); });
    connect(keymapEnabled, &QCheckBox::toggled, this, [this](bool enabled) {
        if (!switchingApp && !currentPackage.isEmpty()) QSettings().setValue("keymap/enabledApps/" + currentPackage, enabled);
        if (!enabled) { mapper->stop(); qmp->releaseAllInputs(); statusBar()->showMessage("Keymap disabled."); return; }
        if (aim->isArmed() && runningMultiTouch) mapper->setReservedTouchSlots({0, 1, 2});
        else mapper->setReservedTouchSlots({});
        if (!mapper->start(view->windowHandle(), profile, runningMultiTouch)) {
            QSignalBlocker block(keymapEnabled); keymapEnabled->setChecked(false);
            QMessageBox::warning(this, "Keymap unavailable", "The player must be running and embedded with at least one control binding.");
        } else {
            statusBar()->showMessage(QStringLiteral("Keymap '%1' enabled. Click the Android display to use it.").arg(profile.name));
        }
    });
    connect(mapper, &InputMapper::stopped, this, [this](const QString &reason) {
        QSignalBlocker block(keymapEnabled); keymapEnabled->setChecked(false); statusBar()->showMessage(reason);
    });
    connect(qmp, &Qmp::readyChanged, this, [this](bool ready) { if (!ready) aim->disarm();
        if (ready && closePending) qmp->powerDown();
        updateControls(); });
    connect(qmp, &Qmp::error, this, [this](const QString &message) { aim->disarm(); if (keymapEnabled->isChecked()) keymapEnabled->setChecked(false); statusBar()->showMessage("QEMU monitor: " + message); });
    connect(embedTimer, &QTimer::timeout, this, [this] { findGuestWindow(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError errorCode) {
        recordEvent("Process error: " + process->errorString());
        if (errorCode == QProcess::FailedToStart) { embedTimer->stop(); qmp->reset(); setRunning(false); view->setStartupStatus("Engine failed to start", process->errorString()); QMessageBox::critical(this, "Player failed to start", process->errorString()); if (closePending) QTimer::singleShot(0, this, [this] { close(); }); }
    });
    connect(process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        embedTimer->stop(); aim->disarm(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); overlay->setAimCrosshairActive(false); qmp->reset(); view->clear(); setRunning(false);
        const QString output = (engineErrorTail + QString::fromLocal8Bit(process->readAllStandardError())).trimmed();
        statusBar()->showMessage(QString("Player stopped (exit code %1).").arg(code));
        if (shutdownTimer) shutdownTimer->stop();
        if (shutdownTask) shutdownTask->kill();
        if (closePending) QTimer::singleShot(0, this, [this] { close(); });
        if (!forcedClose && (status == QProcess::CrashExit || code != 0)) QMessageBox::warning(this, "Player stopped", QString("QEMU exited with code %1.\n\n%2").arg(code).arg(output.isEmpty() ? "No error output was provided." : output.right(4000)));
    });
    connect(statusBar(), &QStatusBar::messageChanged, this, [this](const QString &message) { recordEvent(message); });
    connect(process, &QProcess::readyReadStandardError, this, [this] {
        const auto output = QString::fromLocal8Bit(process->readAllStandardError());
        engineErrorTail = (engineErrorTail + output).right(16000); recordEvent("QEMU: " + output);
    });
    fpsBadge = new FpsBadge(this, view);
    guestPoll = new QTimer(this); guestPoll->setInterval(2000);
    connect(guestPoll, &QTimer::timeout, this, [this] { pollGuest(); }); guestPoll->start();
    QTimer::singleShot(0, this, [this] { launch(); });
}

void MainWindow::recordEvent(const QString &message) {
    if (message.trimmed().isEmpty()) return;
    sessionLog = (sessionLog + QDateTime::currentDateTimeUtc().toString(Qt::ISODate) + " " + message.left(16000) + "\n").right(65536);
}
void MainWindow::setRunning(bool running) {
    launchButton->setVisible(!running); launchButton->setEnabled(!running); vmStatus->setText(running ? "●  ONLINE" : "●  OFFLINE"); updateControls();
}
void MainWindow::launch() {
    if (process->state() != QProcess::NotRunning) return;
    engineErrorTail.clear(); recordEvent("Launch requested.");
    view->setStartupStatus("Loading your Android", "Checking the prepared disk and firmware. Personal data stays in your per-user instance.", true);
    const auto startupGuard = qScopeGuard([this] {
        if (process->state() == QProcess::NotRunning) view->setStartupStatus("Android could not start", "Open Settings → Diagnostics. The prepared image and your personal disk have not been reset.");
    });
    const QString root = QCoreApplication::applicationDirPath();
    const QString base = root + "/images/android-base.qcow2";
    const QString qemu = root + "/qemu/bin/NexusEngine.exe";
    const QString img = root + "/qemu/bin/qemu-img.exe";
    for (const QString &rom : {QStringLiteral("bios-256k.bin"), QStringLiteral("vgabios-stdvga.bin"), QStringLiteral("kvmvapic.bin")}) {
        const QFileInfo file(root + "/qemu/share/qemu/" + rom);
        if (!file.isFile() || file.size() <= 0) { QMessageBox::critical(this, "Incomplete engine firmware", "Missing or empty QEMU ROM: " + rom + ". Extract the complete updated player package."); return; }
    }
    if (!QFileInfo(base).isFile()) { QMessageBox::information(this, "System image unavailable", "This player has no prepared Android system image. You can still configure and export schemes in Keymap Studio."); return; }
    if (!QFileInfo(qemu).isFile()) { QMessageBox::critical(this, "Engine unavailable", "The bundled QEMU engine is missing. Reinstall the complete player package."); return; }
    const QString imageDir = root + "/images";
    const QString code = imageDir + "/android-uefi-code.fd";
    const QString varsTemplate = imageDir + "/android-uefi-vars.fd";
    const bool hasCode = QFileInfo::exists(code), hasVars = QFileInfo::exists(varsTemplate);
    if (hasCode != hasVars) { QMessageBox::critical(this, "Incomplete system", "The prepared UEFI firmware is incomplete. Contact the player developer; no image was changed."); return; }
    const QString disk = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/instances/default.qcow2";
    QString error;
    if (!prepareManagedDisk(img, base, disk, &error)) { QMessageBox::critical(this, "Android system", error); return; }
    const QString vars = disk + ".uefi-vars.fd";
    if (!hasCode && (QFileInfo::exists(vars) || QFileInfo::exists(vars + ".json"))) {
        QMessageBox::critical(this, "UEFI firmware missing", "This instance already has UEFI variables but the staged firmware is missing. Restore the firmware; user data was preserved."); return;
    }
    if (hasCode && (!QFileInfo(code).isFile() || !prepareManagedFirmware(varsTemplate, vars, &error))) {
        QMessageBox::critical(this, "UEFI firmware", error.isEmpty() ? "UEFI code is not a file." : error); return;
    }
    const QSettings s;
    const auto resources = selectedResources(s);
    const int ram = static_cast<int>(resources.memory_mb), cpus = static_cast<int>(resources.cpus);
    recordEvent(QString("Resource allocation: %1 CPUs, %2 MB (%3)").arg(cpus).arg(ram).arg(automaticResources(s) ? "automatic" : "manual"));
    const int port = qBound(1024, s.value("adbPort", 5555).toInt(), 65535);
    bootReady = false;
    hideGuestBoot = s.value("hideBoot", false).toBool() && s.value("adbForward", false).toBool();
    view->setGuestVisible(!hideGuestBoot);
    selectAppKeymap(QString());
    runningMultiTouch = s.value("multiTouch", false).toBool();
    runningAdbForward = s.value("adbForward", false).toBool(); runningAdbPort = port;
    const bool softwareCpu = selectedVmAcceleration(s) == "tcg";
    if (!softwareCpu && nexus_whpx_available() != 1) {
        QMessageBox::critical(this, "Hardware acceleration unavailable", "The Rust runtime could not confirm Windows Hypervisor Platform availability. Enable CPU virtualization in firmware and Windows Hypervisor Platform, then restart Windows. The player will not silently use slow software CPU emulation.");
        return;
    }
    const bool virgl = selectedVmGraphics(s) == "virgl";
    if (virgl) {
        QProcess capability;
        capability.start(qemu, {"-device", "help"});
        if (!capability.waitForFinished(10000) || capability.exitCode() != 0 ||
            !(capability.readAllStandardOutput() + capability.readAllStandardError()).contains("virtio-vga-gl")) {
            capability.kill(); capability.waitForFinished(1000);
            QMessageBox::critical(this, "OpenGL renderer unavailable", "This QEMU build does not advertise virtio-vga-gl. Install the OpenGL/VirGL-enabled engine. No software-rendering fallback was attempted."); return;
        }
    }
    if (selectedVmGraphics(s) == "virtio" || virgl) {
        const QFileInfo rom(root + "/qemu/share/qemu/vgabios-virtio.bin");
        if (!rom.isFile() || rom.size() <= 0) { QMessageBox::critical(this, "VirtIO firmware missing", "Extract the complete updated package; vgabios-virtio.bin is missing or empty."); return; }
    }
    const QString firmware = root + "/qemu/share/qemu";
    const QByteArray diskBytes = disk.toUtf8();
    const QByteArray firmwareBytes = (QFileInfo::exists(firmware + "/bios-256k.bin") ? firmware : QString()).toUtf8();
    const QByteArray codeBytes = (hasCode ? code : QString()).toUtf8();
    const QByteArray varsBytes = (hasCode ? vars : QString()).toUtf8();
    const std::uint32_t flags = (runningMultiTouch ? 1u : 0u) | (!s.value("showCursor", true).toBool() ? 2u : 0u) | (runningAdbForward ? 4u : 0u) | (softwareCpu ? 8u : 0u) | 16u | (selectedVmGraphics(s) == "virtio" ? 32u : 0u) | (softwareCpu && preparedReducedAvx() ? 64u : 0u) | (virgl ? 128u : 0u);
    const NexusVmRequest request{diskBytes.constData(), firmwareBytes.constData(), codeBytes.constData(), varsBytes.constData(), static_cast<std::uint32_t>(ram), static_cast<std::uint32_t>(cpus), static_cast<std::uint32_t>(port), flags};
    const auto size = nexus_vm_plan(&request, nullptr, 0);
    if (size == 0 || size > 1024 * 1024) { QMessageBox::critical(this, "VM configuration", "Rust runtime rejected the launch configuration."); return; }
    QByteArray encoded(static_cast<int>(size), '\0');
    if (nexus_vm_plan(&request, reinterpret_cast<std::uint8_t *>(encoded.data()), size) != size) { QMessageBox::critical(this, "VM configuration", "Rust launch-plan encoding failed."); return; }
    const auto document = QJsonDocument::fromJson(encoded);
    if (!document.isArray() || document.array().isEmpty()) { QMessageBox::critical(this, "VM configuration", "Invalid Rust launch plan."); return; }
    QStringList args;
    for (const auto &value : document.array()) {
        if (!value.isString()) { QMessageBox::critical(this, "VM configuration", "Invalid Rust launch argument."); return; }
        args << value.toString();
    }
    view->setStartupStatus("Starting Android", "Waiting for Android. Boot hiding requires authorized guest ADB; check Diagnostics if it does not become ready.", true);
    process->setProgram(qemu); process->setArguments(args); qmp->reset(); setRunning(true); embedAttempts = 0; embedTimer->start(250); bootWait.start(); process->start();
    setWindowTitle(softwareCpu ? "Project Nexus · SOFTWARE CPU (slow)" : "Project Nexus");
    statusBar()->showMessage(softwareCpu ? "Starting with explicit software CPU emulation (slow)." : "Starting Android with WHPX...");
}
void MainWindow::findGuestWindow() {
    if (process->state() != QProcess::Running) return;
    WindowSearch search{static_cast<DWORD>(process->processId())}; EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
    if (search.result && view->attach(search.result)) { embedTimer->stop(); updateControls(); statusBar()->showMessage("Android display connected."); }
    else if (++embedAttempts >= 120) { embedTimer->stop(); view->setStartupStatus("Waiting for Android display", "The VM may be running in a separate window. Open Settings → Diagnostics; no Android-ready state has been confirmed."); statusBar()->showMessage("Could not embed the QEMU window; it may be running separately."); }
}
void MainWindow::updateControls() {
    const bool running = process->state() == QProcess::Running && !closePending;
    const bool embedded = running && view->windowHandle() && IsWindow(view->windowHandle());
    const bool armed = aim->isArmed();
    shutdownButton->setEnabled(running && qmp->isReady());
    editKeymapButton->setEnabled(!aim->isCapturingMouse());
    mouseLockButton->setEnabled(embedded && !armed);
    keymapEnabled->setEnabled(embedded && !currentPackage.isEmpty() && qmp->isReady() && profile.hasAnyInputBindings() && (!armed || runningMultiTouch));
    aimButton->setEnabled(embedded && !currentPackage.isEmpty() && qmp->isReady());
}
void MainWindow::androidAction(int keycode) {
    if (closePending) return;
    if (keycode != -1 && keycode != 3 && keycode != 4 && keycode != 187 && keycode != 24 && keycode != 25 && keycode != 164) return;
    if (process->state() != QProcess::Running || !runningAdbForward || apkTask) {
        QMessageBox::information(this, "Android controls", "Start Android with local ADB forwarding enabled and authorized TCP debugging. Wait for other ADB operations to finish."); return;
    }
    QString adb = QCoreApplication::applicationDirPath() + "/platform-tools/adb.exe";
    if (!QFileInfo(adb).isFile()) adb = QStandardPaths::findExecutable("adb.exe");
    if (adb.isEmpty()) { QMessageBox::warning(this, "Android controls", "Android platform-tools/adb.exe is required next to the player or on PATH."); return; }
    auto *task = new QProcess(this); apkTask = task;
    connect(task, &QObject::destroyed, this, [this] { apkTask = nullptr; });
    auto *timer = new QTimer(task); timer->setSingleShot(true);
    const QString endpoint = "127.0.0.1:" + QString::number(runningAdbPort);
    connect(timer, &QTimer::timeout, task, [task] { task->setProperty("timeout", true); task->kill(); });
    connect(task, &QProcess::errorOccurred, this, [this, task, timer](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) { timer->stop(); QMessageBox::warning(this, "Android controls", task->errorString()); task->deleteLater(); }
    });
    connect(task, &QProcess::finished, this, [=](int code, QProcess::ExitStatus status) {
        timer->stop();
        const QString output = QString::fromLocal8Bit(task->readAllStandardOutput() + task->readAllStandardError());
        const bool sent = task->property("sent").toBool();
        if (code != 0 || status != QProcess::NormalExit || task->property("timeout").toBool() || output.contains("error", Qt::CaseInsensitive) || output.contains("exception", Qt::CaseInsensitive) || output.contains("denied", Qt::CaseInsensitive) || (!sent && !output.contains("connected to " + endpoint, Qt::CaseInsensitive))) {
            QMessageBox::warning(this, "Android control failed", "Check Android TCP debugging and authorization.\n\n" + output.right(2000)); task->deleteLater(); return;
        }
        if (sent) {
            if (keycode == -1) {
                const int guestTrim = nexus_trim_working_set(static_cast<std::uint32_t>(process->processId()));
                const int playerTrim = nexus_trim_working_set(static_cast<std::uint32_t>(QCoreApplication::applicationPid()));
                const QString result = QString("Android accepted the eligible-background-process cleanup request. This does not mean every background app was closed.\nQEMU working-set trim: %1\nNexus working-set trim: %2\n\nNo measured RAM saving or FPS improvement is claimed.").arg(guestTrim == 1 ? "requested successfully" : "failed").arg(playerTrim == 1 ? "requested successfully" : "failed");
                recordEvent(result); QMessageBox::information(this, "Memory cleanup", result);
            }
            task->deleteLater(); return;
        }
        task->setProperty("sent", true); timer->start(15000);
        if (keycode == -1) task->start(adb, {"-s", endpoint, "shell", "am", "kill-all"});
        else task->start(adb, {"-s", endpoint, "shell", "input", "keyevent", QString::number(keycode)});
    });
    timer->start(15000); task->start(adb, {"connect", endpoint});
}
void MainWindow::applyGuestDisplay(const QString &size, int dpi) {
    if (closePending) return;
    const QStringList sizes = {"reset", "1280x720", "1600x900", "1920x1080", "720x1280", "1080x1920"};
    const QList<int> densities = {0, 160, 200, 240, 280, 320, 400, 480};
    if (!sizes.contains(size) || !densities.contains(dpi)) return;
    if (process->state() != QProcess::Running || !runningAdbForward || apkTask) {
        QMessageBox::information(this, "Android display", "Start Android with local ADB forwarding enabled, authorize TCP debugging in Android, and wait for any current ADB operation to finish."); return;
    }
    QString adb = QCoreApplication::applicationDirPath() + "/platform-tools/adb.exe";
    if (!QFileInfo(adb).isFile()) adb = QStandardPaths::findExecutable("adb.exe");
    if (adb.isEmpty()) { QMessageBox::warning(this, "Android display", "Supply Android platform-tools/adb.exe next to the player or on PATH. No display settings were changed."); return; }
    const QString endpoint = "127.0.0.1:" + QString::number(runningAdbPort);
    auto *task = new QProcess(this); apkTask = task;
    connect(task, &QObject::destroyed, this, [this] { apkTask = nullptr; });
    auto *timer = new QTimer(task); timer->setSingleShot(true);
    task->setProperty("phase", 0);
    connect(timer, &QTimer::timeout, task, [task] { task->setProperty("timedOut", true); task->kill(); });
    connect(task, &QProcess::errorOccurred, this, [this, task, timer](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) { timer->stop(); QMessageBox::warning(this, "Android display", task->errorString()); task->deleteLater(); }
    });
    connect(task, &QProcess::finished, this, [=](int code, QProcess::ExitStatus status) {
        timer->stop();
        const QString output = QString::fromLocal8Bit(task->readAllStandardOutput() + task->readAllStandardError());
        const int phase = task->property("phase").toInt();
        const bool connected = output.contains("connected to " + endpoint, Qt::CaseInsensitive);
        const bool error = output.contains("error", Qt::CaseInsensitive) || output.contains("exception", Qt::CaseInsensitive) || output.contains("denied", Qt::CaseInsensitive);
        if (code != 0 || status != QProcess::NormalExit || task->property("timedOut").toBool() || error || (phase == 0 && !connected)) {
            QMessageBox::warning(this, "Display change incomplete", "ADB could not finish. A previous command may already have applied. Reconnect and Apply Android default to reset both values.\n\n" + output.right(2000)); task->deleteLater(); return;
        }
        if (phase == 3) {
            QMessageBox::information(this, "Android display", "Android reports these display values:\n\n" + output.trimmed()); task->deleteLater(); return;
        }
        task->setProperty("phase", phase + 1); timer->start(20000);
        if (phase == 0) task->start(adb, {"-s", endpoint, "shell", "wm", "size", size});
        else if (phase == 1) task->start(adb, {"-s", endpoint, "shell", "wm", "density", dpi == 0 ? QString("reset") : QString::number(dpi)});
        else task->start(adb, {"-s", endpoint, "shell", "wm size; wm density"});
    });
    timer->start(15000); task->start(adb, {"connect", endpoint});
}
void MainWindow::dragEnterEvent(QDragEnterEvent *event) {
    if (!droppedApk(event->mimeData()).isEmpty()) event->acceptProposedAction();
}
void MainWindow::dropEvent(QDropEvent *event) {
    const auto path = droppedApk(event->mimeData());
    if (path.isEmpty()) return;
    event->acceptProposedAction(); installApk(path);
}
void MainWindow::installApk(const QString &path) {
    if (closePending) return;
    if (process->state() != QProcess::Running) {
        QMessageBox::information(this, "Install APK", "Start Android first. APK installation requires TCP ADB enabled in the Android image and local ADB forwarding enabled in Settings before startup."); return;
    }
    if (apkTask) { QMessageBox::information(this, "Install APK", "An APK operation is already in progress."); return; }
    if (!runningAdbForward) {
        QMessageBox::information(this, "Install APK", "Enable local ADB forwarding in Settings and restart the player. Android must also have TCP ADB enabled and authorized."); return;
    }
    QString adb = QCoreApplication::applicationDirPath() + "/platform-tools/adb.exe";
    if (!QFileInfo(adb).isFile()) adb = QStandardPaths::findExecutable("adb.exe");
    if (adb.isEmpty()) { QMessageBox::information(this, "Install APK", "Android platform-tools (adb.exe) were not found. A developer must supply platform-tools next to the player or on PATH. No APK was installed."); return; }
    const QString apk = path.isEmpty() ? QFileDialog::getOpenFileName(this, "Choose a trusted APK", QString(), "Android packages (*.apk)") : path;
    if (apk.isEmpty()) return;
    if (QMessageBox::question(this, "Install APK", "Install this APK into Android? Only install APKs you trust. Existing apps with the same package may be updated.\n\n" + QFileInfo(apk).fileName(), QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes) return;
    const QString endpoint = "127.0.0.1:" + QString::number(runningAdbPort);
    auto *task = new QProcess(this); apkTask = task; connect(task, &QObject::destroyed, this, [this] { apkTask = nullptr; }); auto *timeout = new QTimer(task); timeout->setSingleShot(true);
    task->setProperty("installPhase", false); task->setProperty("timedOut", false);
    connect(timeout, &QTimer::timeout, task, [task] { task->setProperty("timedOut", true); task->kill(); });
    connect(task, &QProcess::errorOccurred, this, [this, task, timeout](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) { timeout->stop(); QMessageBox::warning(this, "Install APK", "Could not start ADB: " + task->errorString()); task->deleteLater(); }
    });
    connect(task, &QProcess::finished, this, [this, task, timeout, adb, endpoint, apk](int code, QProcess::ExitStatus status) {
        const QString output = QString::fromLocal8Bit(task->readAllStandardOutput() + task->readAllStandardError()).trimmed();
        if (!task->property("installPhase").toBool() && code == 0 && status == QProcess::NormalExit && !task->property("timedOut").toBool() && (output.contains("connected to " + endpoint, Qt::CaseInsensitive) || output.contains("already connected to " + endpoint, Qt::CaseInsensitive))) {
            task->setProperty("installPhase", true); timeout->start(120000); task->start(adb, {"-s", endpoint, "install", "-r", apk}); return;
        }
        timeout->stop();
        if (task->property("installPhase").toBool() && code == 0 && status == QProcess::NormalExit && output.contains("Success")) QMessageBox::information(this, "Install APK", "ADB reports that the APK was installed.");
        else QMessageBox::warning(this, "Install APK", "ADB could not complete the install. Confirm the Android image supports and authorizes TCP ADB.\n\n" + (task->property("timedOut").toBool() ? QString("ADB timed out.") : output.right(3000)));
        task->deleteLater();
    });
    timeout->start(15000); task->start(adb, {"connect", endpoint});
    statusBar()->showMessage("Connecting to Android to install APK...");
}
void MainWindow::editKeymap() {
    if (closePending) return;
    if (process->state() == QProcess::Running && currentPackage.isEmpty()) {
        QMessageBox::information(this, "Waiting for active app", "Per-app keymaps need authorized local ADB forwarding and an identified foreground Android app. No global keymap will be applied to unknown apps."); return;
    }
    if (aim->isArmed()) aim->disarm();
    mouseLock->unlock();
    keymapEnabled->setChecked(false);
    QPixmap snapshot;
    if (view->windowHandle() && IsWindow(view->windowHandle())) {
        if (QScreen *screen = QGuiApplication::primaryScreen())
            snapshot = screen->grabWindow(reinterpret_cast<WId>(view->windowHandle()));
    }
    const QString editingPackage = currentPackage;
    KeymapDialog dialog(snapshot, profile, profileDir, activeScheme, this);
    dialog.setWindowTitle(editingPackage.isEmpty() ? "Keymap library" : "Keymap · " + editingPackage);
    if (dialog.exec() != QDialog::Accepted) {
        overlay->refreshSync();
        return;
    }
    activeScheme = sanitizeSchemeName(dialog.resultSchemeName());
    keymapPath = schemeFilePath(profileDir, activeScheme);
    const KeymapProfile updated = dialog.resultProfile();
    QString error;
    if (!saveKeymapProfile(keymapPath, updated, &error)) {
        QMessageBox::warning(this, "Could not save keymap", error);
        return;
    }
    QSettings s;
    s.setValue(currentPackage.isEmpty() ? "keymap/activeScheme" : "keymap/apps/" + currentPackage, activeScheme);
    profile = updated;
    overlay->setProfile(profile);
    updateControls();
    statusBar()->showMessage(QStringLiteral("Saved scheme '%1'. Enable On in the dock and click the display to play.").arg(profile.name));
}
void MainWindow::closeEvent(QCloseEvent *event) {
    if (process->state() != QProcess::NotRunning) {
        event->ignore();
        if (!closePending) requestCloseShutdown();
        return;
    }
    if (shutdownTimer) shutdownTimer->stop();
    if (shutdownTask) shutdownTask->kill();
    overlay->hide(); fpsBadge->hide();
    event->accept();
}

void MainWindow::requestCloseShutdown() {
    closePending = true; forcedClose = false;
    aim->disarm(); mouseLock->unlock();
    // Preserve each app's mapping preference while releasing all active input.
    { const QSignalBlocker blocked(keymapEnabled); keymapEnabled->setChecked(false); }
    mapper->stop(); qmp->releaseAllInputs(); overlay->hide(); fpsBadge->hide();
    if (guestTask) guestTask->kill();
    setWindowTitle("Project Nexus · Shutting down Android…");
    recordEvent("Close requested: asking Android to shut down; waiting for engine exit, no automatic kill.");
    if (!shutdownTimer) {
        shutdownTimer = new QTimer(this); shutdownTimer->setSingleShot(true);
        connect(shutdownTimer, &QTimer::timeout, this, [this] { shutdownWaitExpired(); });
    }
    shutdownTimer->start(45000);
    updateControls();
    // ACPI is the fallback even when guest debugging is absent. Some Android
    // builds ignore it; never treat sending the request as completed shutdown.
    if (qmp->isReady()) qmp->powerDown();
    if (!runningAdbForward) return;
    QString adb = QCoreApplication::applicationDirPath() + "/platform-tools/adb.exe";
    if (!QFileInfo(adb).isFile()) adb = QStandardPaths::findExecutable("adb.exe");
    if (adb.isEmpty()) { recordEvent("Shutdown: adb.exe unavailable; relying on ACPI request."); return; }
    auto *task = new QProcess(this); shutdownTask = task;
    auto *deadline = new QTimer(task); deadline->setSingleShot(true);
    connect(deadline, &QTimer::timeout, task, [task] { task->kill(); });
    const QString endpoint = QString("127.0.0.1:%1").arg(runningAdbPort);
    connect(task, &QProcess::errorOccurred, this, [this, task](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            recordEvent("Shutdown ADB failed to start; ACPI request remains the fallback.");
            if (shutdownTask == task) shutdownTask = nullptr;
            task->deleteLater();
        }
    });
    connect(task, &QProcess::finished, this, [this, task, deadline, adb, endpoint](int code, QProcess::ExitStatus status) {
        deadline->stop();
        if (closePending && process->state() == QProcess::Running &&
            !task->property("powerRequested").toBool() && code == 0 && status == QProcess::NormalExit) {
            task->readAllStandardOutput(); task->setProperty("powerRequested", true); deadline->start(10000);
            task->start(adb, {"-s", endpoint, "shell", "svc", "power", "shutdown"}); return;
        }
        recordEvent(QString("Shutdown ADB helper finished (exit %1); only engine exit completes closing.").arg(code));
        if (shutdownTask == task) shutdownTask = nullptr;
        task->deleteLater();
    });
    deadline->start(10000); task->start(adb, {"connect", endpoint});
}

void MainWindow::shutdownWaitExpired() {
    if (!closePending || process->state() == QProcess::NotRunning) return;
    QMessageBox box(QMessageBox::Warning, "Android is still running",
        "Android has not stopped after 45 seconds. It may be busy, or this guest may not support the shutdown request.\n\nWait longer, keep the Player open, or explicitly force-stop. Force-stopping can corrupt Android data.", QMessageBox::NoButton, this);
    auto *wait = box.addButton("Wait another 45 seconds", QMessageBox::AcceptRole);
    auto *keep = box.addButton("Keep Player open", QMessageBox::RejectRole);
    auto *force = box.addButton("Force stop and close", QMessageBox::DestructiveRole);
    box.setDefaultButton(wait); box.setEscapeButton(keep);
    connect(process, &QProcess::finished, &box, [&box] { box.accept(); });
    box.exec();
    if (process->state() == QProcess::NotRunning) { close(); return; }
    if (box.clickedButton() == force) {
        forcedClose = true;
        recordEvent("User explicitly confirmed force-stop after graceful shutdown timed out.");
        process->kill(); // finished signal closes the Player; no blocking GUI wait.
        shutdownTimer->start(45000);
    } else if (box.clickedButton() == wait) {
        shutdownTimer->start(45000);
    } else {
        closePending = false;
        if (shutdownTask) shutdownTask->kill();
        setWindowTitle("Project Nexus · Shutdown not confirmed");
        recordEvent("Close cancelled; guest may still be shutting down. Disk not reset.");
        updateControls();
    }
}

void MainWindow::changeEvent(QEvent *event) {
    QMainWindow::changeEvent(event);
    if (event->type() == QEvent::WindowStateChange) {
        const bool fullscreen = isFullScreen();
        if (auto *header = findChild<QFrame *>("header")) header->hide();
        if (menuWidget()) menuWidget()->setVisible(!fullscreen);
        statusBar()->hide();
        fullscreenButton->setText(fullscreen ? "⛶  Exit full screen · F11" : "⛶  Full screen");
        overlay->refreshSync();
    }
}

void MainWindow::selectAppKeymap(const QString &package) {
    switchingApp = true;
    const auto switchingGuard = qScopeGuard([this] { switchingApp = false; });
    // Never carry held inputs or aim capture into a different/unknown app.
    if (aim->isArmed()) aim->disarm();
    mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); qmp->releaseAllInputs();
    currentPackage = package;
    profile = KeymapProfile{};
    if (package.isEmpty()) {
        profileDir = globalProfileDir; activeScheme = "default";
    } else {
        // Hash the package for a filesystem-safe, per-app library directory.
        const auto hash = QCryptographicHash::hash(package.toUtf8(), QCryptographicHash::Sha256).toHex();
        profileDir = globalProfileDir + "/apps/" + QString::fromLatin1(hash);
        QDir().mkpath(profileDir);
        activeScheme = sanitizeSchemeName(QSettings().value("keymap/apps/" + package, "default").toString());
        QString error;
        if (!loadKeymapProfile(schemeFilePath(profileDir, activeScheme), &profile, &error)) {
            profile = KeymapProfile{}; profile.name = package;
        }
    }
    keymapPath = schemeFilePath(profileDir, activeScheme);
    overlay->setProfile(profile); updateControls();
    if (!package.isEmpty() && profile.hasAnyInputBindings() && QSettings().value("keymap/enabledApps/" + package, false).toBool()) keymapEnabled->setChecked(true);
    editKeymapButton->setToolTip(package.isEmpty() ? "Keymap · waiting for active Android app" : "Keymap · " + package);
}

void MainWindow::pollGuest() {
    if (closePending) return;
    const bool running = process->state() == QProcess::Running;
    // Check before any ADB early-return: missing adb.exe/authorization is the
    // exact case in which an unbounded loading screen used to conceal the VM.
    if (running && hideGuestBoot && !bootReady &&
        (!QSettings().value("hideBoot", false).toBool() ||
         (bootWait.isValid() && bootDisplayWaitExpired(bootWait.elapsed())))) {
        const bool expired = bootWait.isValid() && bootDisplayWaitExpired(bootWait.elapsed());
        hideGuestBoot = false;
        view->setGuestVisible(true);
        view->setStartupStatus("Android startup not confirmed",
            "The boot-status connection is unavailable or Android is still starting. Boot hiding has been released. Check Settings → Diagnostics. Your disk was not reset.", false);
        if (expired) {
            recordEvent("Boot display wait expired after 180 seconds. Guest display revealed; Android readiness NOT confirmed. Check ADB/debugging, guest boot and TCG slowness.");
            setWindowTitle("Project Nexus · Android startup not confirmed");
            QMessageBox::warning(this, "Android startup not confirmed",
                "Nexus has not received Android's boot-complete reply after three minutes. The guest display is now revealed instead of hiding behind an endless spinner.\n\nThis can mean missing/unauthorized ADB, a guest boot hang, or slow software CPU emulation. This timeout does not prove which one. Check Settings → Diagnostics.\n\nYour disk was preserved; no reset or automatic force-stop was performed.");
        }
    }
    const bool fps = QSettings().value("showFps", false).toBool();
    fpsBadge->setVisible(fps && running && isVisible() && !isMinimized());
    if (fpsBadge->isVisible()) {
        fpsBadge->adjustSize(); static_cast<FpsBadge *>(fpsBadge)->syncPosition();
    }
    if (!running || !runningAdbForward) {
        fpsBadge->setText("Display FPS: N/A · ADB required");
        if (!currentPackage.isEmpty()) selectAppKeymap(QString());
        return;
    }
    if (guestTask || QApplication::activeModalWidget()) return;
    QString adb = QCoreApplication::applicationDirPath() + "/platform-tools/adb.exe";
    if (!QFileInfo(adb).isFile()) adb = QStandardPaths::findExecutable("adb.exe");
    if (adb.isEmpty()) { fpsBadge->setText("Display FPS: N/A · adb missing"); return; }
    auto *task = new QProcess(this); guestTask = task;
    auto *timeout = new QTimer(task); timeout->setSingleShot(true);
    connect(timeout, &QTimer::timeout, task, [task] { task->kill(); });
    const QString endpoint = QString("127.0.0.1:%1").arg(runningAdbPort);
    connect(task, &QProcess::errorOccurred, this, [this, task](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) { guestTask = nullptr; task->deleteLater(); fpsBadge->setText("Display FPS: N/A"); }
    });
    connect(task, &QProcess::finished, this, [this, task, timeout, adb, endpoint, fps](int code, QProcess::ExitStatus exit) {
        timeout->stop();
        if (!task->property("queried").toBool() && code == 0 && exit == QProcess::NormalExit) {
            task->readAllStandardOutput(); task->setProperty("queried", true); timeout->start(8000);
            QString command = "getprop sys.boot_completed; echo NEXUS_ACTIVITY; dumpsys activity activities; echo NEXUS_FPS; ";
            if (fps) command += "cat /proc/uptime; echo NEXUS_LATENCY; dumpsys SurfaceFlinger --latency";
            task->start(adb, {"-s", endpoint, "shell", command}); return;
        }
        const QString text = QString::fromUtf8(task->readAllStandardOutput()).replace("\r", "");
        guestTask = nullptr; task->deleteLater();
        if (closePending || process->state() != QProcess::Running) return;
        if (code != 0 || exit != QProcess::NormalExit || !task->property("queried").toBool()) {
            fpsBadge->setText("Display FPS: N/A");
            if (!currentPackage.isEmpty()) selectAppKeymap(QString());
            return;
        }
        const bool ready = text.section('\n', 0, 0).trimmed() == "1";
        if (ready && !bootReady) {
            bootReady = true;
            if (hideGuestBoot) view->setGuestVisible(true);
            recordEvent("Android reports boot complete (not a usable-UI or ANR certification).");
        }
        const QString package = foregroundPackage(text.section("NEXUS_ACTIVITY\n", 1).section("NEXUS_FPS\n", 0, 0));
        if (!QApplication::activeModalWidget() && package != currentPackage) selectAppKeymap(package);
        fpsBadge->setText(fps ? compositorFps(text.section("NEXUS_FPS\n", 1)) : "Display FPS: N/A");
    });
    timeout->start(8000); task->start(adb, {"connect", endpoint});
}
