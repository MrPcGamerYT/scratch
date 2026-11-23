#include "mainwindow.h"
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
QPushButton *railButton(const QString &icon, const QString &label, QWidget *parent) {
    auto *button = new QPushButton(icon + "\n" + label, parent);
    button->setObjectName("railButton"); button->setToolTip(label); button->setAccessibleName(label);
    button->setMinimumHeight(67); return button;
}
}

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent), process(new QProcess(this)), embedTimer(new QTimer(this)) {
    qmp = new Qmp(process, this); mapper = new InputMapper(qmp, this); mouseLock = new MouseLock(this);
    setWindowTitle("Project Nexus"); resize(1400, 840); setMinimumSize(900, 600);
    auto *central = new QWidget(this); auto *root = new QVBoxLayout(central); root->setContentsMargins(0, 0, 0, 0); root->setSpacing(0);
    auto *header = new QFrame(central); header->setObjectName("header");
    auto *headerLayout = new QHBoxLayout(header); headerLayout->setContentsMargins(24, 12, 20, 12); headerLayout->setSpacing(12);
    auto *mark = new QLabel("N", header); mark->setObjectName("logoMark"); mark->setAlignment(Qt::AlignCenter); mark->setFixedSize(38, 38); headerLayout->addWidget(mark);
    auto *brandBox = new QVBoxLayout; brandBox->setSpacing(0);
    auto *brand = new QLabel("NEXUS", header); brand->setObjectName("brand");
    auto *eyebrow = new QLabel("ANDROID PLAYER", header); eyebrow->setObjectName("eyebrow");
    brandBox->addWidget(brand); brandBox->addWidget(eyebrow); headerLayout->addLayout(brandBox);
    headerLayout->addStretch();
    vmStatus = new QLabel("●  OFFLINE", header); vmStatus->setObjectName("statusPill"); headerLayout->addWidget(vmStatus);
    launchButton = new QPushButton("Start player", header); launchButton->setObjectName("primary"); headerLayout->addWidget(launchButton);
    shutdownButton = new QPushButton("Shut down", header); headerLayout->addWidget(shutdownButton);
    root->addWidget(header);

    auto *body = new QHBoxLayout; body->setContentsMargins(16, 16, 16, 16); body->setSpacing(12); root->addLayout(body, 1);
    auto *viewport = new QFrame(central); viewport->setObjectName("viewportPanel"); auto *v = new QVBoxLayout(viewport); v->setContentsMargins(0, 0, 0, 0); v->setSpacing(0);
    view = new EmulatorView(viewport); v->addWidget(view, 1); body->addWidget(viewport, 1);
    auto *rail = new QFrame(central); rail->setObjectName("toolRail"); rail->setFixedWidth(88); auto *railLayout = new QVBoxLayout(rail); railLayout->setContentsMargins(7, 10, 7, 10); railLayout->setSpacing(4);
    auto *toolsLabel = new QLabel("TOOLS", rail); toolsLabel->setObjectName("eyebrow"); toolsLabel->setAlignment(Qt::AlignCenter); railLayout->addWidget(toolsLabel);
    editKeymapButton = railButton("⌨", "Keymap", rail); railLayout->addWidget(editKeymapButton);
    keymapEnabled = new QCheckBox("On", rail); keymapEnabled->setToolTip("Enable or disable saved keymap"); keymapEnabled->setAccessibleName("Enable keymap"); railLayout->addWidget(keymapEnabled, 0, Qt::AlignHCenter);
    mouseLockButton = railButton("◎", "Lock", rail); railLayout->addWidget(mouseLockButton);
    fullscreenButton = railButton("⛶", "Full screen", rail); railLayout->addWidget(fullscreenButton);
    railLayout->addStretch();
    auto *settingsButton = railButton("⚙", "Settings", rail); railLayout->addWidget(settingsButton);
    body->addWidget(rail); setCentralWidget(central);

    const QString profileDir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/keymaps";
    QDir().mkpath(profileDir); keymapPath = QDir(profileDir).filePath("default.json"); QString error;
    if (!loadKeymap(keymapPath, &bindings, &error)) statusBar()->showMessage("Keymap could not be loaded: " + error);
    else statusBar()->showMessage("Ready to play  ·  Your controls stay within the player");
    setRunning(false);
    connect(launchButton, &QPushButton::clicked, this, [this] { launch(); });
    connect(shutdownButton, &QPushButton::clicked, this, [this] { qmp->powerDown(); statusBar()->showMessage("Shutdown requested. If Android ignores it, shut down inside the guest."); });
    connect(settingsButton, &QPushButton::clicked, this, [this] { SettingsDialog dialog(process->state() != QProcess::NotRunning, this); dialog.exec(); });
    connect(fullscreenButton, &QPushButton::clicked, this, [this] { if (isFullScreen()) showNormal(); else showFullScreen(); });
    connect(editKeymapButton, &QPushButton::clicked, this, [this] { editKeymap(); });
    connect(mouseLockButton, &QPushButton::clicked, this, [this] {
        if (mouseLock->isLocked()) { mouseLock->unlock(); return; }
        if (!mouseLock->lock(view->windowHandle())) QMessageBox::warning(this, "Mouse lock", "Could not lock the mouse to the player display.");
        else { mouseLockButton->setText("◎\nUnlock"); statusBar()->showMessage("Mouse locked. Press Ctrl+Alt+L to unlock; switching apps also releases it."); }
    });
    connect(mouseLock, &MouseLock::unlocked, this, [this] { mouseLockButton->setText("◎\nLock"); statusBar()->showMessage("Mouse unlocked."); });
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
    connect(qmp, &Qmp::readyChanged, this, [this](bool) { updateControls(); });
    connect(qmp, &Qmp::error, this, [this](const QString &message) { if (keymapEnabled->isChecked()) keymapEnabled->setChecked(false); statusBar()->showMessage("QEMU monitor: " + message); });
    connect(embedTimer, &QTimer::timeout, this, [this] { findGuestWindow(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError errorCode) {
        if (errorCode == QProcess::FailedToStart) { embedTimer->stop(); qmp->reset(); setRunning(false); QMessageBox::critical(this, "Player failed to start", process->errorString()); }
    });
    connect(process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        embedTimer->stop(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); qmp->reset(); view->clear(); setRunning(false);
        const QString output = QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
        statusBar()->showMessage(QString("Player stopped (exit code %1).").arg(code));
        if (status == QProcess::CrashExit || code != 0) QMessageBox::warning(this, "Player stopped", QString("QEMU exited with code %1.\n\n%2").arg(code).arg(output.isEmpty() ? "No error output was provided." : output.right(4000)));
    });
    const QString base = QCoreApplication::applicationDirPath() + "/images/android-base.qcow2";
    if (QFileInfo(base).isFile() && QSettings().value("autoStart", true).toBool()) QTimer::singleShot(0, this, [this] { launch(); });
    else if (!QFileInfo(base).isFile()) statusBar()->showMessage("Android system image not installed. Contact the player developer.");
}

void MainWindow::setRunning(bool running) {
    launchButton->setEnabled(!running); vmStatus->setText(running ? "●  ONLINE" : "●  OFFLINE"); updateControls();
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
    if (s.value("adbForward", false).toBool()) net += ",hostfwd=tcp:127.0.0.1:" + QString::number(port) + "-:5555";
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
    shutdownButton->setEnabled(running && qmp->isReady()); editKeymapButton->setEnabled(embedded); mouseLockButton->setEnabled(embedded);
    keymapEnabled->setEnabled(embedded && qmp->isReady() && !bindings.isEmpty());
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
        mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); disconnect(process, nullptr, this, nullptr); process->kill(); process->waitForFinished(5000);
    }
    event->accept();
}
