#include "mainwindow.h"
#include "aimcontroller.h"
#include "emulatorview.h"
#include "inputmapper.h"
#include "keymapdialog.h"
#include "manageddisk.h"
#include "mouselock.h"
#include "qmp.h"
#include "settingsdialog.h"
#include <QCheckBox>
#include <QCloseEvent>
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QFileDialog>
#include <QFrame>
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
    qmp = new Qmp(process, this); mapper = new InputMapper(qmp, this); mouseLock = new MouseLock(this); aim = new AimController(qmp, this);
    setWindowTitle("Project Nexus"); resize(1400, 840); setMinimumSize(1100, 600);
    auto *central = new QWidget(this); auto *root = new QVBoxLayout(central); root->setContentsMargins(0, 0, 0, 0); root->setSpacing(0);
    auto *header = new QFrame(central); header->setObjectName("header");
    auto *headerLayout = new QHBoxLayout(header); headerLayout->setContentsMargins(18, 9, 18, 9); headerLayout->setSpacing(8);
    auto *mark = new QLabel("N", header); mark->setObjectName("logoMark"); mark->setAlignment(Qt::AlignCenter); mark->setFixedSize(34, 34); headerLayout->addWidget(mark);
    auto *brand = new QLabel("NEXUS", header); brand->setObjectName("brand"); headerLayout->addWidget(brand);
    headerLayout->addStretch();
    auto *dock = new QFrame(header); dock->setObjectName("topDock"); auto *dockLayout = new QHBoxLayout(dock); dockLayout->setContentsMargins(5, 4, 5, 4); dockLayout->setSpacing(2);
    auto *settingsButton = dockButton("⚙  Settings", dock); dockLayout->addWidget(settingsButton);
    auto *apkButton = dockButton("＋  Install APK", dock); dockLayout->addWidget(apkButton);
    editKeymapButton = dockButton("⌨  Keymap", dock); dockLayout->addWidget(editKeymapButton);
    keymapEnabled = new QCheckBox("On", dock); keymapEnabled->setToolTip("Enable saved keymap"); keymapEnabled->setAccessibleName("Enable keymap"); dockLayout->addWidget(keymapEnabled);
    mouseLockButton = dockButton("◎  Lock", dock); dockLayout->addWidget(mouseLockButton);
    auto *volumeButton = dockButton("♪  Volume", dock); volumeButton->setEnabled(false); volumeButton->setToolTip("Android volume control is not available yet"); dockLayout->addWidget(volumeButton);
    fullscreenButton = dockButton("⛶  Full screen", dock); dockLayout->addWidget(fullscreenButton);
    aimButton = dockButton("⌖  Aim & fire", dock); aimButton->setToolTip("Experimental single-pointer aim and fire"); dockLayout->addWidget(aimButton);
    auto *cleanButton = dockButton("✦  RAM", dock); cleanButton->setEnabled(false); cleanButton->setToolTip("Android and Windows memory cleanup is not available yet"); dockLayout->addWidget(cleanButton);
    headerLayout->addWidget(dock); headerLayout->addStretch();
    vmStatus = new QLabel("●  OFFLINE", header); vmStatus->setObjectName("statusPill"); headerLayout->addWidget(vmStatus);
    launchButton = dockButton("Retry", header); launchButton->setToolTip("Retry starting the prepared Android system"); headerLayout->addWidget(launchButton);
    shutdownButton = dockButton("Stop", header); headerLayout->addWidget(shutdownButton);
    root->addWidget(header);

    auto *viewport = new QFrame(central); viewport->setObjectName("viewportPanel"); auto *v = new QVBoxLayout(viewport); v->setContentsMargins(0, 0, 0, 0);
    view = new EmulatorView(viewport); v->addWidget(view, 1); root->addWidget(viewport, 1); setCentralWidget(central);

    const QString profileDir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/keymaps";
    QDir().mkpath(profileDir); keymapPath = QDir(profileDir).filePath("default.json"); QString error;
    if (!loadKeymap(keymapPath, &bindings, &error)) statusBar()->showMessage("Keymap could not be loaded: " + error);
    else statusBar()->showMessage("Ready to play  ·  Your controls stay within the player");
    setRunning(false);
    connect(launchButton, &QPushButton::clicked, this, [this] { launch(); });
    connect(shutdownButton, &QPushButton::clicked, this, [this] { qmp->powerDown(); statusBar()->showMessage("Shutdown requested. If Android ignores it, shut down inside the guest."); });
    connect(apkButton, &QPushButton::clicked, this, [this] { installApk(); });
    connect(settingsButton, &QPushButton::clicked, this, [this] { SettingsDialog dialog(process->state() != QProcess::NotRunning, this); dialog.exec(); });
    connect(fullscreenButton, &QPushButton::clicked, this, [this] { if (isFullScreen()) showNormal(); else showFullScreen(); });
    connect(editKeymapButton, &QPushButton::clicked, this, [this] { editKeymap(); });
    connect(aimButton, &QPushButton::clicked, this, [this] {
        if (aim->isArmed()) { aim->disarm(); return; }
        mouseLock->unlock(); keymapEnabled->setChecked(false);
        QSettings s;
        QPointF anchor(s.value("aim/anchorX", 50.0).toDouble() / 100.0, s.value("aim/anchorY", 50.0).toDouble() / 100.0);
        QPointF fire(s.value("aim/fireX", 80.0).toDouble() / 100.0, s.value("aim/fireY", 75.0).toDouble() / 100.0);
        if (!aim->arm(view->windowHandle(), anchor, fire, s.value("aim/sensitivity", 1.0).toDouble()))
            QMessageBox::warning(this, "Aim & fire", "Could not arm experimental aim mode. The embedded player and QEMU monitor must be ready.");
    });
    connect(aim, &AimController::statusChanged, this, [this](bool armed, bool, const QString &message) {
        aimButton->setText(armed ? "⌖  Disarm aim" : "⌖  Aim & fire"); statusBar()->showMessage(message); updateControls();
    });
    connect(mouseLockButton, &QPushButton::clicked, this, [this] {
        if (mouseLock->isLocked()) { mouseLock->unlock(); return; }
        if (!mouseLock->lock(view->windowHandle())) QMessageBox::warning(this, "Mouse lock", "Could not lock the mouse to the player display.");
        else { mouseLockButton->setText("◎  Unlock"); statusBar()->showMessage("Mouse locked. Press Ctrl+Alt+L to unlock; switching apps also releases it."); }
    });
    connect(mouseLock, &MouseLock::unlocked, this, [this] { mouseLockButton->setText("◎  Lock"); statusBar()->showMessage("Mouse unlocked."); });
    connect(keymapEnabled, &QCheckBox::toggled, this, [this](bool enabled) {
        if (!enabled) { mapper->stop(); statusBar()->showMessage("Keymap disabled."); return; }
        if (!mapper->start(view->windowHandle(), bindings)) {
            QSignalBlocker block(keymapEnabled); keymapEnabled->setChecked(false);
            QMessageBox::warning(this, "Keymap unavailable", "The player must be running and embedded with at least one binding.");
        } else statusBar()->showMessage("Keymap enabled. Click the Android display to use it.");
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
        embedTimer->stop(); aim->disarm(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); qmp->reset(); view->clear(); setRunning(false);
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
    if (!QFileInfo(base).isFile()) { QMessageBox::information(this, "System image unavailable", "This player has no prepared Android system image. Please contact the player developer."); return; }
    if (!QFileInfo(qemu).isFile()) { QMessageBox::critical(this, "Engine unavailable", "The bundled QEMU engine is missing. Reinstall the complete player package."); return; }
    const QString disk = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/instances/default.qcow2";
    QString error;
    if (!prepareManagedDisk(img, base, disk, &error)) { QMessageBox::critical(this, "Android system", error); return; }
    const QSettings s;
    const int ram = qBound(512, s.value("memory", 4096).toInt(), 131072);
    const int cpus = qBound(1, s.value("cpus", 4).toInt(), 64);
    const int port = qBound(1024, s.value("adbPort", 5555).toInt(), 65535);
    QStringList args = {"-accel", "whpx,kernel-irqchip=off", "-m", QString::number(ram), "-smp", QString::number(cpus),
                        "-vga", "std", "-usb", "-device", "usb-tablet", "-hda", disk, "-boot", "order=c",
                        "-name", "Project Nexus", "-qmp", "stdio"};
    if (!s.value("showCursor", true).toBool()) args << "-display" << "sdl,show-cursor=off";
    const QString firmware = root + "/qemu/share/qemu";
    if (QFileInfo::exists(firmware + "/bios-256k.bin")) args << "-L" << firmware;
    QString net = "user,id=net0";
    runningAdbForward = s.value("adbForward", false).toBool(); runningAdbPort = port;
    if (runningAdbForward) net += ",hostfwd=tcp:127.0.0.1:" + QString::number(port) + "-:5555";
    args << "-netdev" << net << "-device" << "virtio-net-pci,netdev=net0,romfile=";
    process->setProgram(qemu); process->setArguments(args); qmp->reset(); setRunning(true); embedAttempts = 0; embedTimer->start(250); process->start();
    statusBar()->showMessage("Starting Android with WHPX...");
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
    shutdownButton->setEnabled(running && qmp->isReady()); editKeymapButton->setEnabled(embedded && !armed); mouseLockButton->setEnabled(embedded && !armed);
    keymapEnabled->setEnabled(embedded && qmp->isReady() && !bindings.isEmpty() && !armed);
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
    mouseLock->unlock(); keymapEnabled->setChecked(false);
    if (!view->windowHandle()) return;
    QScreen *screen = QGuiApplication::primaryScreen();
    const QPixmap snapshot = screen ? screen->grabWindow(reinterpret_cast<WId>(view->windowHandle())) : QPixmap();
    if (snapshot.isNull()) { QMessageBox::warning(this, "Screenshot unavailable", "Could not capture the Android display. Make sure the window is visible, then try again."); return; }
    KeymapDialog dialog(snapshot, bindings, this); if (dialog.exec() != QDialog::Accepted) return;
    QString error; const auto updated = dialog.resultBindings();
    if (!saveKeymap(keymapPath, updated, &error)) { QMessageBox::warning(this, "Could not save keymap", error); return; }
    bindings = updated; updateControls(); statusBar()->showMessage("Keymap saved. Enable it and click the display to use your controls.");
}
void MainWindow::closeEvent(QCloseEvent *event) {
    if (process->state() != QProcess::NotRunning) {
        const auto choice = QMessageBox::question(this, "Player still running", "Force stop and close? This can corrupt the guest disk. Choose No and shut down Android first.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
        if (choice != QMessageBox::Yes) { event->ignore(); return; }
        aim->disarm(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); disconnect(process, nullptr, this, nullptr); process->kill(); process->waitForFinished(5000);
    }
    event->accept();
}
