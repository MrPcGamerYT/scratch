#include "mainwindow.h"
#include "emulatorview.h"
#include "imagedownload.h"
#include "inputmapper.h"
#include "keymapdialog.h"
#include "manageddisk.h"
#include "mouselock.h"
#include "qmp.h"

#include <QCoreApplication>
#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QGuiApplication>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QProcess>
#include <QProgressDialog>
#include <QPushButton>
#include <QSettings>
#include <QScreen>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QStandardPaths>
#include <QSpinBox>
#include <QStatusBar>
#include <QTimer>
#include <QVBoxLayout>

namespace {
QWidget *pathRow(QLineEdit *edit, const QString &filter, bool save, QWidget *parent) {
    auto *row = new QWidget(parent);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    auto *button = new QPushButton("Browse...", row);
    layout->addWidget(edit, 1);
    layout->addWidget(button);
    QObject::connect(button, &QPushButton::clicked, row, [edit, filter, save, parent] {
        QString path = save ? QFileDialog::getSaveFileName(parent, "Select virtual disk", edit->text(), filter)
                            : QFileDialog::getOpenFileName(parent, "Select file", edit->text(), filter);
        if (!path.isEmpty()) edit->setText(path);
    });
    return row;
}

struct WindowSearch { DWORD pid; HWND result = nullptr; long long area = 0; };
BOOL CALLBACK findWindow(HWND hwnd, LPARAM data) {
    auto *search = reinterpret_cast<WindowSearch *>(data);
    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != search->pid || !IsWindowVisible(hwnd) || GetWindow(hwnd, GW_OWNER)) return TRUE;
    RECT rect{};
    if (!GetClientRect(hwnd, &rect)) return TRUE;
    const long long area = static_cast<long long>(rect.right - rect.left) * (rect.bottom - rect.top);
    if (area > search->area) { search->area = area; search->result = hwnd; }
    return TRUE;
}
}

MainWindow::MainWindow(QWidget *parent) : QMainWindow(parent), process(new QProcess(this)), embedTimer(new QTimer(this)) {
    qmp = new Qmp(process, this);
    mapper = new InputMapper(qmp, this);
    mouseLock = new MouseLock(this);
    setWindowTitle("Project Nexus");
    resize(1380, 800); setMinimumSize(920, 620);
    auto *central = new QWidget(this);
    auto *root = new QVBoxLayout(central); root->setContentsMargins(0, 0, 0, 0); root->setSpacing(0);

    auto *header = new QFrame(central); header->setObjectName("header");
    auto *headerLayout = new QHBoxLayout(header); headerLayout->setContentsMargins(24, 15, 24, 15);
    auto *brandBox = new QVBoxLayout; brandBox->setSpacing(0);
    auto *eyebrow = new QLabel("ANDROID WORKSPACE", header); eyebrow->setObjectName("eyebrow");
    auto *brand = new QLabel("Project Nexus", header); brand->setObjectName("brand");
    brandBox->addWidget(eyebrow); brandBox->addWidget(brand); headerLayout->addLayout(brandBox);
    headerLayout->addStretch();
    vmStatus = new QLabel("READY", header); vmStatus->setObjectName("statusPill"); headerLayout->addWidget(vmStatus);
    auto *settingsButton = new QPushButton("Settings", header); headerLayout->addWidget(settingsButton);
    fullscreenButton = new QPushButton("Full screen", header); headerLayout->addWidget(fullscreenButton);
    root->addWidget(header);

    auto *body = new QHBoxLayout; body->setContentsMargins(18, 18, 18, 18); body->setSpacing(16); root->addLayout(body, 1);
    auto *side = new QFrame(central); settingsPanel = side; side->setObjectName("sidePanel"); side->setFixedWidth(350); side->hide();
    auto *sideOuter = new QVBoxLayout(side); sideOuter->setContentsMargins(12, 12, 12, 12);
    auto *scroll = new QScrollArea(side); scroll->setWidgetResizable(true); scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    auto *sideContents = new QWidget(scroll); auto *sideLayout = new QVBoxLayout(sideContents); sideLayout->setContentsMargins(2, 2, 2, 2);
    auto *sideTitle = new QLabel("Your virtual machine", sideContents); sideTitle->setObjectName("sectionTitle"); sideLayout->addWidget(sideTitle);
    auto *sideIntro = new QLabel("Configure your Android workspace before launch.", sideContents); sideIntro->setObjectName("muted"); sideIntro->setWordWrap(true); sideLayout->addWidget(sideIntro);

    auto *storage = new QGroupBox("SYSTEM & STORAGE", sideContents); auto *form = new QFormLayout(storage); form->setLabelAlignment(Qt::AlignLeft);
    qemuPath = new QLineEdit(storage); isoPath = new QLineEdit(storage); diskPath = new QLineEdit(storage);
    form->addRow("QEMU", pathRow(qemuPath, "Executables (*.exe)", false, storage));
    form->addRow("ISO", pathRow(isoPath, "ISO images (*.iso)", false, storage));
    form->addRow("Disk", pathRow(diskPath, "QCOW2 disks (*.qcow2)", true, storage));
    boot = new QComboBox(storage); boot->addItem("ISO · install / live", "d"); boot->addItem("Disk · installed system", "c"); form->addRow("Boot", boot);
    downloadButton = new QPushButton("Download Android-x86", storage); form->addRow(downloadButton);
    auto *storageNote = new QLabel("A new disk is created automatically (up to 32 GiB).", storage); storageNote->setObjectName("muted"); storageNote->setWordWrap(true); form->addRow(storageNote);
    storage->hide();
    auto *devToggle = new QPushButton("Developer / legacy image setup", sideContents);
    sideLayout->addWidget(devToggle); sideLayout->addWidget(storage);
    connect(devToggle, &QPushButton::clicked, storage, [storage] { storage->setVisible(!storage->isVisible()); });

    auto *performance = new QGroupBox("PERFORMANCE & DISPLAY", sideContents); auto *perfForm = new QFormLayout(performance);
    memory = new QSpinBox(performance); memory->setRange(512, 131072); memory->setSingleStep(512); memory->setSuffix(" MB");
    cpus = new QSpinBox(performance); cpus->setRange(1, 64);
    showCursor = new QCheckBox("Show host mouse cursor", performance);
    perfForm->addRow("Memory", memory); perfForm->addRow("CPU cores", cpus); perfForm->addRow("Graphics", new QLabel("Standard VGA", performance)); perfForm->addRow(showCursor);
    auto *displayNote = new QLabel("Guest resolution is controlled by Android. Restart the VM after changing these settings.", performance); displayNote->setObjectName("muted"); displayNote->setWordWrap(true); perfForm->addRow(displayNote);
    sideLayout->addWidget(performance);

    auto *network = new QGroupBox("ADVANCED", sideContents); auto *netForm = new QFormLayout(network);
    adbPort = new QSpinBox(network); adbPort->setRange(1024, 65535); adbForward = new QCheckBox("Forward local ADB port", network);
    autoStart = new QCheckBox("Start prepared image automatically", network);
    netForm->addRow(autoStart); netForm->addRow(adbForward); netForm->addRow("Port", adbPort); sideLayout->addWidget(network);
    sideLayout->addStretch(); scroll->setWidget(sideContents); sideOuter->addWidget(scroll); body->addWidget(side);

    auto *viewport = new QFrame(central); viewport->setObjectName("viewportPanel"); auto *viewportLayout = new QVBoxLayout(viewport);
    viewportLayout->setContentsMargins(14, 14, 14, 14); viewportLayout->setSpacing(12);
    auto *viewportHeader = new QHBoxLayout;
    auto *screenTitles = new QVBoxLayout; screenTitles->setSpacing(2);
    auto *screenTitle = new QLabel("Android display", viewport); screenTitle->setObjectName("sectionTitle");
    auto *screenNote = new QLabel("Your VM appears here when it starts", viewport); screenNote->setObjectName("muted");
    screenTitles->addWidget(screenTitle); screenTitles->addWidget(screenNote); viewportHeader->addLayout(screenTitles); viewportHeader->addStretch();
    launchButton = new QPushButton("Start emulator", viewport); launchButton->setObjectName("primary"); viewportHeader->addWidget(launchButton);
    shutdownButton = new QPushButton("Shut down", viewport); viewportHeader->addWidget(shutdownButton); viewportLayout->addLayout(viewportHeader);
    view = new EmulatorView(viewport); viewportLayout->addWidget(view, 1);
    auto *tools = new QHBoxLayout; editKeymapButton = new QPushButton("Edit keymap", viewport); tools->addWidget(editKeymapButton);
    keymapEnabled = new QCheckBox("Enable keymap", viewport); tools->addWidget(keymapEnabled);
    mouseLockButton = new QPushButton("Lock mouse", viewport); tools->addWidget(mouseLockButton); tools->addStretch();
    auto *hint = new QLabel("Click the Android display before using mapped keys", viewport); hint->setObjectName("muted"); tools->addWidget(hint); viewportLayout->addLayout(tools);
    body->addWidget(viewport, 1);
    setCentralWidget(central);
    loadSettings();
    const QString profileDir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/keymaps";
    QDir().mkpath(profileDir); keymapPath = QDir(profileDir).filePath("default.json");
    QString keymapError;
    if (!loadKeymap(keymapPath, &bindings, &keymapError)) statusBar()->showMessage("Keymap could not be loaded: " + keymapError);
    else statusBar()->showMessage("Ready. Select your disk and start the emulator.");
    setRunning(false);

    connect(settingsButton, &QPushButton::clicked, this, [this] { settingsPanel->setVisible(!settingsPanel->isVisible()); });
    connect(launchButton, &QPushButton::clicked, this, [this] { launch(); });
    connect(downloadButton, &QPushButton::clicked, this, [this] { downloadAndroid(); });
    connect(shutdownButton, &QPushButton::clicked, this, [this] {
        qmp->powerDown(); statusBar()->showMessage("Shutdown requested. If Android ignores it, shut down inside the guest.");
    });
    connect(fullscreenButton, &QPushButton::clicked, this, [this] {
        if (isFullScreen()) { showNormal(); fullscreenButton->setText("Full screen"); }
        else { showFullScreen(); fullscreenButton->setText("Exit full screen"); }
    });
    connect(editKeymapButton, &QPushButton::clicked, this, [this] { editKeymap(); });
    connect(mouseLockButton, &QPushButton::clicked, this, [this] {
        if (mouseLock->isLocked()) { mouseLock->unlock(); return; }
        if (!mouseLock->lock(view->windowHandle())) QMessageBox::warning(this, "Mouse lock", "Could not lock the mouse to the emulator display.");
        else { mouseLockButton->setText("Unlock mouse"); statusBar()->showMessage("Mouse locked. Press Ctrl+Alt+L to unlock; switching apps also releases it."); }
    });
    connect(mouseLock, &MouseLock::unlocked, this, [this] { mouseLockButton->setText("Lock mouse"); statusBar()->showMessage("Mouse unlocked."); });
    connect(keymapEnabled, &QCheckBox::toggled, this, [this](bool enabled) {
        if (!enabled) { mapper->stop(); statusBar()->showMessage("Keymap disabled."); return; }
        if (!mapper->start(view->windowHandle(), bindings)) {
            QSignalBlocker block(keymapEnabled); keymapEnabled->setChecked(false);
            QMessageBox::warning(this, "Keymap unavailable", "Could not enable the keymap. Confirm the VM is running, embedded, and has at least one binding.");
        } else statusBar()->showMessage("Keymap enabled. Click the Android display to use it.");
    });
    connect(qmp, &Qmp::readyChanged, this, [this](bool) { updateKeymapControls(); });
    connect(qmp, &Qmp::error, this, [this](const QString &message) {
        if (keymapEnabled->isChecked()) keymapEnabled->setChecked(false);
        statusBar()->showMessage("QEMU monitor: " + message);
    });
    connect(embedTimer, &QTimer::timeout, this, [this] { findGuestWindow(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            embedTimer->stop(); qmp->reset(); setRunning(false);
            QMessageBox::critical(this, "QEMU failed to start", process->errorString());
        }
    });
    connect(process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        embedTimer->stop(); mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop(); qmp->reset(); view->clear(); setRunning(false);
        const QString output = QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
        statusBar()->showMessage(QString("QEMU stopped (exit code %1).").arg(code));
        if (status == QProcess::CrashExit || code != 0)
            QMessageBox::warning(this, "QEMU stopped", QString("QEMU exited with code %1.\n\n%2").arg(code).arg(output.isEmpty() ? "No error output was provided." : output.right(4000)));
    });
    const QString preparedBase = QCoreApplication::applicationDirPath() + "/images/android-base.qcow2";
    if (QFileInfo(preparedBase).isFile() && autoStart->isChecked()) QTimer::singleShot(0, this, [this] { launch(); });
    else if (!QFileInfo(preparedBase).isFile()) statusBar()->showMessage("No prepared Android image found. Open Settings for legacy image setup.");
}

void MainWindow::setRunning(bool running) {
    launchButton->setEnabled(!running);
    downloadButton->setEnabled(!running);
    shutdownButton->setEnabled(running && qmp->isReady());
    vmStatus->setText(running ? "RUNNING" : "READY");
    qemuPath->parentWidget()->setEnabled(!running); isoPath->parentWidget()->setEnabled(!running); diskPath->parentWidget()->setEnabled(!running);
    memory->setEnabled(!running); cpus->setEnabled(!running); boot->setEnabled(!running); autoStart->setEnabled(!running);
    adbForward->setEnabled(!running); adbPort->setEnabled(!running); showCursor->setEnabled(!running);
    updateKeymapControls();
}

void MainWindow::saveSettings() {
    QSettings s;
    s.setValue("qemu", qemuPath->text()); s.setValue("iso", isoPath->text()); s.setValue("disk", diskPath->text());
    s.setValue("memory", memory->value()); s.setValue("cpus", cpus->value());
    s.setValue("showCursor", showCursor->isChecked()); s.setValue("autoStart", autoStart->isChecked());
    s.setValue("adbPort", adbPort->value()); s.setValue("adbForward", adbForward->isChecked()); s.setValue("boot", boot->currentIndex());
}

void MainWindow::loadSettings() {
    QSettings s;
    const QString bundledQemu = QCoreApplication::applicationDirPath() + "/qemu/bin/qemu-system-x86_64.exe";
    const QString savedQemu = s.value("qemu").toString();
    qemuPath->setText(QFileInfo::exists(bundledQemu) ? bundledQemu :
                      !savedQemu.isEmpty() && QFileInfo::exists(savedQemu) ? savedQemu : "C:/Program Files/qemu/qemu-system-x86_64.exe");
    isoPath->setText(s.value("iso").toString()); diskPath->setText(s.value("disk").toString());
    memory->setValue(s.value("memory", 4096).toInt()); cpus->setValue(s.value("cpus", 4).toInt());
    showCursor->setChecked(s.value("showCursor", true).toBool()); autoStart->setChecked(s.value("autoStart", true).toBool());
    adbPort->setValue(s.value("adbPort", 5555).toInt()); adbForward->setChecked(s.value("adbForward", false).toBool());
    boot->setCurrentIndex(s.value("boot", 0).toInt());
}

void MainWindow::downloadAndroid() {
    const QString destination = QFileDialog::getSaveFileName(this, "Save stock Android-x86 image",
        "android-x86_64-9.0-r2.iso", "ISO images (*.iso)");
    if (destination.isEmpty()) return;
    auto *download = new ImageDownload(this);
    auto *progress = new QProgressDialog("Downloading Android-x86 9.0-r2 (about 921 MiB)...", "Cancel", 0, 100, this);
    progress->setWindowTitle("Download Android");
    progress->setWindowModality(Qt::WindowModal);
    progress->setAutoClose(false);
    progress->setAutoReset(false);
    progress->setMinimumDuration(0);
    downloadButton->setEnabled(false);
    launchButton->setEnabled(false);
    connect(progress, &QProgressDialog::canceled, download, &ImageDownload::cancel);
    connect(download, &ImageDownload::progress, progress, [progress](qint64 received, qint64 total) {
        if (total > 0) { progress->setRange(0, 100); progress->setValue(static_cast<int>(received * 100 / total)); }
        else progress->setRange(0, 0);
    });
    connect(download, &ImageDownload::finished, this, [this, download, progress](bool ok, const QString &message) {
        disconnect(progress, &QProgressDialog::canceled, download, &ImageDownload::cancel);
        progress->close(); progress->deleteLater(); download->deleteLater();
        downloadButton->setEnabled(true); launchButton->setEnabled(true);
        if (ok) {
            isoPath->setText(message); saveSettings();
            statusBar()->showMessage("Android-x86 image downloaded and SHA-256 verified. Choose a disk and launch.");
        } else {
            QMessageBox::warning(this, "Android download", message);
        }
    });
    progress->show();
    // SourceForge's Android-x86 project release; hash also published by OSDN.
    download->start(QUrl("https://downloads.sourceforge.net/project/android-x86/Release%209.0/android-x86_64-9.0-r2.iso"), destination,
                    "f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e");
}

void MainWindow::launch() {
    const QFileInfo qemu(qemuPath->text().trimmed());
    const QString basePath = QCoreApplication::applicationDirPath() + "/images/android-base.qcow2";
    const bool managed = QFileInfo(basePath).isFile();
    const QFileInfo iso(managed ? QString() : isoPath->text().trimmed());
    QString selectedDisk = diskPath->text().trimmed();
    if (managed) {
        selectedDisk = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation) + "/instances/default.qcow2";
        QString error;
        if (!prepareManagedDisk(qemu.dir().filePath("qemu-img.exe"), basePath, selectedDisk, &error)) {
            QMessageBox::critical(this, "Android image", error); return;
        }
    }
    const QFileInfo disk(selectedDisk);
    const QString bootOrder = managed ? QStringLiteral("c") : boot->currentData().toString();
    if (!qemu.isFile() || selectedDisk.isEmpty() || !disk.dir().exists() ||
        (bootOrder == "d" && !iso.isFile()) ||
        (!managed && !isoPath->text().trimmed().isEmpty() && !iso.isFile()) || (disk.exists() && !disk.isFile())) {
        QMessageBox::warning(this, "Check paths", "Select an existing QEMU executable, a disk path in an existing folder, and an existing ISO when booting from ISO. Leave the ISO empty to boot only from disk.");
        return;
    }
    if (iso.isFile() && disk.exists() && iso.canonicalFilePath().compare(disk.canonicalFilePath(), Qt::CaseInsensitive) == 0) {
        QMessageBox::warning(this, "Check paths", "The ISO and virtual disk must be different files."); return;
    }
    if (!disk.exists() && bootOrder == "c") {
        QMessageBox::warning(this, "Disk not found", "Choose an existing installed disk, or boot from ISO to create one."); return;
    }
    if (!disk.exists()) {
        const QString img = qemu.dir().filePath("qemu-img.exe");
        if (!QFileInfo::exists(img)) { QMessageBox::warning(this, "qemu-img missing", "qemu-img.exe must be next to qemu-system-x86_64.exe to create a disk."); return; }
        QProcess create;
        create.start(img, {"create", "-f", "qcow2", disk.absoluteFilePath(), "32G"});
        if (!create.waitForStarted(5000) || !create.waitForFinished(30000) || create.exitStatus() != QProcess::NormalExit || create.exitCode() != 0) {
            if (create.state() != QProcess::NotRunning) { create.kill(); create.waitForFinished(); }
            QMessageBox::critical(this, "Disk creation failed", QString::fromLocal8Bit(create.readAllStandardError()) + "\n" + create.errorString()); return;
        }
    }
    saveSettings();
    QStringList args = {"-accel", "whpx,kernel-irqchip=off", "-m", QString::number(memory->value()),
                        "-smp", QString::number(cpus->value()), "-vga", "std", "-usb", "-device", "usb-tablet",
                        "-hda", disk.absoluteFilePath(), "-boot", "order=" + bootOrder,
                        "-name", "Project Nexus", "-qmp", "stdio"};
    if (!showCursor->isChecked()) args << "-display" << "sdl,show-cursor=off";
    const QString bundledFirmware = QDir::cleanPath(qemu.dir().filePath("../share/qemu"));
    if (QFileInfo::exists(QDir(bundledFirmware).filePath("bios-256k.bin"))) args << "-L" << bundledFirmware;
    if (!managed && !isoPath->text().trimmed().isEmpty()) args << "-cdrom" << iso.absoluteFilePath();
    QString net = "user,id=net0";
    if (adbForward->isChecked()) net += ",hostfwd=tcp:127.0.0.1:" + QString::number(adbPort->value()) + "-:5555";
    args << "-netdev" << net << "-device" << "virtio-net-pci,netdev=net0,romfile=";
    process->setProgram(qemu.absoluteFilePath()); process->setArguments(args);
    qmp->reset(); setRunning(true); embedAttempts = 0; embedTimer->start(250); process->start();
    statusBar()->showMessage("Starting QEMU with WHPX...");
}

void MainWindow::findGuestWindow() {
    if (process->state() != QProcess::Running) return;
    WindowSearch search{static_cast<DWORD>(process->processId())};
    EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
    if (search.result && view->attach(search.result)) {
        embedTimer->stop(); updateKeymapControls(); statusBar()->showMessage("QEMU window embedded. Use Android's installer or boot the installed system.");
    } else if (++embedAttempts >= 120) {
        embedTimer->stop();
        statusBar()->showMessage("Could not embed the QEMU window; it may still be running separately.");
    }
}

void MainWindow::updateKeymapControls() {
    const bool running = process->state() == QProcess::Running;
    const bool embedded = running && view->windowHandle() && IsWindow(view->windowHandle());
    shutdownButton->setEnabled(running && qmp->isReady());
    editKeymapButton->setEnabled(embedded);
    mouseLockButton->setEnabled(embedded);
    keymapEnabled->setEnabled(embedded && qmp->isReady() && !bindings.isEmpty());
}

void MainWindow::editKeymap() {
    mouseLock->unlock(); keymapEnabled->setChecked(false);
    if (!view->windowHandle()) return;
    QScreen *screen = QGuiApplication::primaryScreen();
    const QPixmap snapshot = screen ? screen->grabWindow(reinterpret_cast<WId>(view->windowHandle())) : QPixmap();
    if (snapshot.isNull()) {
        QMessageBox::warning(this, "Screenshot unavailable", "Could not capture the Android display. Make sure its window is visible, then try again."); return;
    }
    KeymapDialog dialog(snapshot, bindings, this);
    if (dialog.exec() != QDialog::Accepted) return;
    QString error;
    const auto updated = dialog.resultBindings();
    if (!saveKeymap(keymapPath, updated, &error)) { QMessageBox::warning(this, "Could not save keymap", error); return; }
    bindings = updated; updateKeymapControls();
    statusBar()->showMessage("Keymap saved. Enable it and click the Android display to use your controls.");
}

void MainWindow::closeEvent(QCloseEvent *event) {
    saveSettings();
    if (process->state() != QProcess::NotRunning) {
        const auto choice = QMessageBox::question(this, "VM still running", "QEMU is still running. Force stop it and close? This can corrupt the guest disk. Choose No and shut down Android first.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
        if (choice != QMessageBox::Yes) { event->ignore(); return; }
        mouseLock->unlock(); keymapEnabled->setChecked(false); mapper->stop();
        disconnect(process, nullptr, this, nullptr);
        process->kill(); process->waitForFinished(5000);
    }
    event->accept();
}
