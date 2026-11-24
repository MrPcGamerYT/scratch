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
#include <QJsonDocument>
#include <QJsonArray>
#include <QCheckBox>
#include <QCloseEvent>
#include <QCoreApplication>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QEvent>
#include <QShortcut>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QLabel>
#include <QMessageBox>
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
    button->setObjectName("dockButton"); button->setToolTip(label); button->setAccessibleName(label);
    return button;
}
}

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent), process(new QProcess(this)), embedTimer(new QTimer(this)) {
    qmp = new Qmp(process, this);
    mapper = new InputMapper(qmp, this);
    mouseLock = new MouseLock(this);
    aim = new AimController(qmp, this);
    setProperty("nexusPlayer", true);
    setWindowTitle("Project Nexus"); resize(1400, 840); setMinimumSize(1100, 600);
    auto *central = new QWidget(this); auto *root = new QVBoxLayout(central); root->setContentsMargins(0, 0, 0, 0); root->setSpacing(0);
    auto *header = new QFrame(central); header->setObjectName("header");
    auto *headerLayout = new QHBoxLayout(header); headerLayout->setContentsMargins(18, 9, 18, 9); headerLayout->setSpacing(8);
    auto *mark = new QLabel("N", header); mark->setObjectName("logoMark"); mark->setAlignment(Qt::AlignCenter); mark->setFixedSize(34, 34); headerLayout->addWidget(mark);
    auto *brand = new QLabel("NEXUS", header); brand->setObjectName("brand"); headerLayout->addWidget(brand);
    headerLayout->addStretch();
    auto *dock = new QFrame(header); dock->setObjectName("topDock"); auto *dockLayout = new QHBoxLayout(dock); dockLayout->setContentsMargins(6, 4, 6, 4); dockLayout->setSpacing(4);
    auto *settingsButton = dockButton("⚙  Settings", dock); dockLayout->addWidget(settingsButton);
    auto *apkButton = dockButton("＋  Install APK", dock); dockLayout->addWidget(apkButton);
    editKeymapButton = dockButton("⌨  Keymap", dock); editKeymapButton->setToolTip("Open Keymap Studio (schemes, D-pad, swipes, pinch zoom, MOBA, macros)"); dockLayout->addWidget(editKeymapButton);
    keymapEnabled = new QCheckBox("On", dock); keymapEnabled->setToolTip("Enable active keymap scheme"); keymapEnabled->setAccessibleName("Enable keymap"); dockLayout->addWidget(keymapEnabled);
    overlayEnabled = new QCheckBox("Hints", dock); overlayEnabled->setToolTip("Show translucent key-hint badges over the display"); overlayEnabled->setAccessibleName("Show keymap hints"); dockLayout->addWidget(overlayEnabled);
    mouseLockButton = dockButton("◎  Lock", dock); dockLayout->addWidget(mouseLockButton);
    fullscreenButton = dockButton("⛶  Full screen · F11", dock); dockLayout->addWidget(fullscreenButton);
    aimButton = dockButton("⌖  Aim & fire", dock); aimButton->setToolTip("Arm Aim, Shoot, ADS & Free-Look; multi-touch enables concurrent WASD & taps"); dockLayout->addWidget(aimButton);
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
    connect(settingsButton, &QPushButton::clicked, this, [this] { SettingsDialog dialog(process->state() != QProcess::NotRunning, this); dialog.exec(); });
    auto toggleFullscreen = [this] { if (isFullScreen()) showNormal(); else showFullScreen(); };
    connect(fullscreenButton, &QPushButton::clicked, this, toggleFullscreen);
    auto *fullscreenShortcut = new QShortcut(QKeySequence(Qt::Key_F11), this);
    fullscreenShortcut->setContext(Qt::WindowShortcut);
    connect(fullscreenShortcut, &QShortcut::activated, this, toggleFullscreen);
    connect(editKeymapButton, &QPushButton::clicked, this, [this] { editKeymap(); });
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
    connect(qmp, &Qmp::readyChanged, this, [this](bool ready) { if (!ready) aim->disarm(); updateControls(); });
    connect(qmp, &Qmp::error, this, [this](const QString &message) { aim->disarm(); if (keymapEnabled->isChecked()) keymapEnabled->setChecked(false); statusBar()->showMessage("QEMU monitor: " + message); });
    connect(embedTimer, &QTimer::timeout, this, [this] { findGuestWindow(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError errorCode) {
        if (errorCode == QProcess::FailedToStart) { embedTimer->stop(); qmp->reset(); setRunning(false); QMessageBox::critical(this, "Player failed to start", process->errorString()); }
    });
    connect(process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        embedTimer->stop(); aim->disarm(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); overlay->setAimCrosshairActive(false); qmp->reset(); view->clear(); setRunning(false);
        const QString output = QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
        statusBar()->showMessage(QString("Player stopped (exit code %1).").arg(code));
        if (status == QProcess::CrashExit || code != 0) QMessageBox::warning(this, "Player stopped", QString("QEMU exited with code %1.\n\n%2").arg(code).arg(output.isEmpty() ? "No error output was provided." : output.right(4000)));
    });
    QTimer::singleShot(0, this, [this] { launch(); });
}

void MainWindow::setRunning(bool running) {
    launchButton->setVisible(!running); launchButton->setEnabled(!running); vmStatus->setText(running ? "●  ONLINE" : "●  OFFLINE"); updateControls();
}
void MainWindow::launch() {
    if (process->state() != QProcess::NotRunning) return;
    const QString root = QCoreApplication::applicationDirPath();
    const QString base = root + "/images/android-base.qcow2";
    const QString qemu = root + "/qemu/bin/qemu-system-x86_64.exe";
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
    const int ram = qBound(512, s.value("memory", 4096).toInt(), 131072);
    const int cpus = qBound(1, s.value("cpus", 4).toInt(), 64);
    const int port = qBound(1024, s.value("adbPort", 5555).toInt(), 65535);
    runningMultiTouch = s.value("multiTouch", false).toBool();
    runningAdbForward = s.value("adbForward", false).toBool(); runningAdbPort = port;
    const bool softwareCpu = selectedVmAcceleration(s) == "tcg";
    if (!softwareCpu && nexus_whpx_available() != 1) {
        QMessageBox::critical(this, "Hardware acceleration unavailable", "The Rust runtime could not confirm Windows Hypervisor Platform availability. Enable CPU virtualization in firmware and Windows Hypervisor Platform, then restart Windows. The player will not silently use slow software CPU emulation.");
        return;
    }
    const QString firmware = root + "/qemu/share/qemu";
    const QByteArray diskBytes = disk.toUtf8();
    const QByteArray firmwareBytes = (QFileInfo::exists(firmware + "/bios-256k.bin") ? firmware : QString()).toUtf8();
    const QByteArray codeBytes = (hasCode ? code : QString()).toUtf8();
    const QByteArray varsBytes = (hasCode ? vars : QString()).toUtf8();
    const std::uint32_t flags = (runningMultiTouch ? 1u : 0u) | (!s.value("showCursor", true).toBool() ? 2u : 0u) | (runningAdbForward ? 4u : 0u) | (softwareCpu ? 8u : 0u);
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
    process->setProgram(qemu); process->setArguments(args); qmp->reset(); setRunning(true); embedAttempts = 0; embedTimer->start(250); process->start();
    setWindowTitle(softwareCpu ? "Project Nexus · SOFTWARE CPU (slow)" : "Project Nexus");
    statusBar()->showMessage(softwareCpu ? "Starting with explicit software CPU emulation (slow)." : "Starting Android with WHPX...");
}
void MainWindow::findGuestWindow() {
    if (process->state() != QProcess::Running) return;
    WindowSearch search{static_cast<DWORD>(process->processId())}; EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
    if (search.result && view->attach(search.result)) { embedTimer->stop(); updateControls(); statusBar()->showMessage("Android display connected."); }
    else if (++embedAttempts >= 120) { embedTimer->stop(); statusBar()->showMessage("Could not embed the QEMU window; it may be running separately."); }
}
void MainWindow::updateControls() {
    const bool running = process->state() == QProcess::Running;
    const bool embedded = running && view->windowHandle() && IsWindow(view->windowHandle());
    const bool armed = aim->isArmed();
    shutdownButton->setEnabled(running && qmp->isReady());
    editKeymapButton->setEnabled(!aim->isCapturingMouse());
    mouseLockButton->setEnabled(embedded && !armed);
    keymapEnabled->setEnabled(embedded && qmp->isReady() && profile.hasAnyInputBindings() && (!armed || runningMultiTouch));
    aimButton->setEnabled(embedded && qmp->isReady());
}
void MainWindow::installApk() {
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
    const QString apk = QFileDialog::getOpenFileName(this, "Choose a trusted APK", QString(), "Android packages (*.apk)");
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
    if (aim->isArmed()) aim->disarm();
    mouseLock->unlock();
    keymapEnabled->setChecked(false);
    QPixmap snapshot;
    if (view->windowHandle() && IsWindow(view->windowHandle())) {
        if (QScreen *screen = QGuiApplication::primaryScreen())
            snapshot = screen->grabWindow(reinterpret_cast<WId>(view->windowHandle()));
    }
    KeymapDialog dialog(snapshot, profile, profileDir, activeScheme, this);
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
    s.setValue("keymap/activeScheme", activeScheme);
    profile = updated;
    overlay->setProfile(profile);
    updateControls();
    statusBar()->showMessage(QStringLiteral("Saved scheme '%1'. Enable On in the dock and click the display to play.").arg(profile.name));
}
void MainWindow::closeEvent(QCloseEvent *event) {
    if (process->state() != QProcess::NotRunning) {
        const auto choice = QMessageBox::question(this, "Player still running", "Force stop and close? This can corrupt the guest disk. Choose No and shut down Android first.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
        if (choice != QMessageBox::Yes) { event->ignore(); return; }
        aim->disarm(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); overlay->hide(); disconnect(process, nullptr, this, nullptr); process->kill(); process->waitForFinished(5000);
    }
    overlay->hide();
    event->accept();
}

void MainWindow::changeEvent(QEvent *event) {
    QMainWindow::changeEvent(event);
    if (event->type() == QEvent::WindowStateChange) {
        const bool fullscreen = isFullScreen();
        if (auto *header = findChild<QFrame *>("header")) header->hide();
        if (menuWidget()) menuWidget()->setVisible(!fullscreen);
        statusBar()->hide();
        fullscreenButton->setText(fullscreen ? "⛶  Exit full screen · F11" : "⛶  Full screen · F11");
        overlay->refreshSync();
    }
}
