#include "mainwindow.h"
#include "emulatorview.h"
#include "imagedownload.h"

#include <QCoreApplication>
#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDir>
#include <QFileDialog>
#include <QFileInfo>
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
    setWindowTitle("Project Nexus");
    resize(1200, 720);
    auto *central = new QWidget(this);
    auto *layout = new QHBoxLayout(central);
    view = new EmulatorView(central);
    layout->addWidget(view, 1);

    auto *panel = new QGroupBox("Virtual machine", central);
    panel->setMaximumWidth(370);
    auto *form = new QFormLayout(panel);
    qemuPath = new QLineEdit(panel);
    isoPath = new QLineEdit(panel);
    diskPath = new QLineEdit(panel);
    form->addRow("QEMU", pathRow(qemuPath, "Executables (*.exe)", false, panel));
    form->addRow("Android ISO", pathRow(isoPath, "ISO images (*.iso)", false, panel));
    downloadButton = new QPushButton("Download Android-x86 9.0", panel);
    form->addRow(downloadButton);
    form->addRow("Disk", pathRow(diskPath, "QCOW2 disks (*.qcow2)", true, panel));
    memory = new QSpinBox(panel); memory->setRange(512, 131072); memory->setSingleStep(512); memory->setSuffix(" MB");
    cpus = new QSpinBox(panel); cpus->setRange(1, 64);
    adbPort = new QSpinBox(panel); adbPort->setRange(1024, 65535);
    adbForward = new QCheckBox("Forward local ADB port", panel);
    boot = new QComboBox(panel); boot->addItem("ISO (install / live)", "d"); boot->addItem("Disk (installed system)", "c");
    form->addRow("Memory", memory); form->addRow("CPU cores", cpus);
    form->addRow("Boot from", boot); form->addRow(adbForward); form->addRow("ADB port", adbPort);
    auto *note = new QLabel("First boot: select an ISO. A missing disk is created as a 32 GiB sparse QCOW2 file. Install Android to the disk, then boot from Disk.", panel);
    note->setWordWrap(true); form->addRow(note);
    launchButton = new QPushButton("Launch", panel);
    shutdownButton = new QPushButton("Request shutdown", panel);
    form->addRow(launchButton); form->addRow(shutdownButton);
    layout->addWidget(panel);
    setCentralWidget(central);
    loadSettings();
    setRunning(false);
    statusBar()->showMessage("Ready. Check WHPX and virtualization before launching.");

    connect(launchButton, &QPushButton::clicked, this, [this] { launch(); });
    connect(downloadButton, &QPushButton::clicked, this, [this] { downloadAndroid(); });
    connect(shutdownButton, &QPushButton::clicked, this, [this] {
        if (process->state() == QProcess::Running) {
            process->write("system_powerdown\n");
            statusBar()->showMessage("Shutdown requested. The guest may need to be shut down from Android instead.");
        }
    });
    connect(embedTimer, &QTimer::timeout, this, [this] { findGuestWindow(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            embedTimer->stop(); setRunning(false);
            QMessageBox::critical(this, "QEMU failed to start", process->errorString());
        }
    });
    connect(process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        embedTimer->stop(); view->clear(); setRunning(false);
        const QString output = QString::fromLocal8Bit(process->readAllStandardError()).trimmed();
        statusBar()->showMessage(QString("QEMU stopped (exit code %1).").arg(code));
        if (status == QProcess::CrashExit || code != 0)
            QMessageBox::warning(this, "QEMU stopped", QString("QEMU exited with code %1.\n\n%2").arg(code).arg(output.isEmpty() ? "No error output was provided." : output.right(4000)));
    });
}

void MainWindow::setRunning(bool running) {
    launchButton->setEnabled(!running);
    downloadButton->setEnabled(!running);
    shutdownButton->setEnabled(running);
    qemuPath->parentWidget()->setEnabled(!running); isoPath->parentWidget()->setEnabled(!running); diskPath->parentWidget()->setEnabled(!running);
    memory->setEnabled(!running); cpus->setEnabled(!running); boot->setEnabled(!running);
    adbForward->setEnabled(!running); adbPort->setEnabled(!running);
}

void MainWindow::saveSettings() {
    QSettings s;
    s.setValue("qemu", qemuPath->text()); s.setValue("iso", isoPath->text()); s.setValue("disk", diskPath->text());
    s.setValue("memory", memory->value()); s.setValue("cpus", cpus->value());
    s.setValue("adbPort", adbPort->value()); s.setValue("adbForward", adbForward->isChecked()); s.setValue("boot", boot->currentIndex());
}

void MainWindow::loadSettings() {
    QSettings s;
    const QString bundledQemu = QCoreApplication::applicationDirPath() + "/qemu/bin/qemu-system-x86_64.exe";
    const QString savedQemu = s.value("qemu").toString();
    qemuPath->setText(!savedQemu.isEmpty() && QFileInfo::exists(savedQemu) ? savedQemu :
                      QFileInfo::exists(bundledQemu) ? bundledQemu : "C:/Program Files/qemu/qemu-system-x86_64.exe");
    isoPath->setText(s.value("iso").toString()); diskPath->setText(s.value("disk").toString());
    memory->setValue(s.value("memory", 4096).toInt()); cpus->setValue(s.value("cpus", 4).toInt());
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
    const QFileInfo iso(isoPath->text().trimmed());
    const QFileInfo disk(diskPath->text().trimmed());
    if (!qemu.isFile() || diskPath->text().trimmed().isEmpty() || !disk.dir().exists() ||
        (boot->currentData().toString() == "d" && !iso.isFile()) ||
        (!isoPath->text().trimmed().isEmpty() && !iso.isFile()) || (disk.exists() && !disk.isFile())) {
        QMessageBox::warning(this, "Check paths", "Select an existing QEMU executable, a disk path in an existing folder, and an existing ISO when booting from ISO. Leave the ISO empty to boot only from disk.");
        return;
    }
    if (iso.isFile() && disk.exists() && iso.canonicalFilePath().compare(disk.canonicalFilePath(), Qt::CaseInsensitive) == 0) {
        QMessageBox::warning(this, "Check paths", "The ISO and virtual disk must be different files."); return;
    }
    if (!disk.exists() && boot->currentData().toString() == "c") {
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
                        "-hda", disk.absoluteFilePath(), "-boot", "order=" + boot->currentData().toString(),
                        "-name", "Project Nexus", "-monitor", "stdio"};
    const QString bundledFirmware = QDir::cleanPath(qemu.dir().filePath("../share/qemu"));
    if (QFileInfo::exists(QDir(bundledFirmware).filePath("bios-256k.bin"))) args << "-L" << bundledFirmware;
    if (!isoPath->text().trimmed().isEmpty()) args << "-cdrom" << iso.absoluteFilePath();
    QString net = "user,id=net0";
    if (adbForward->isChecked()) net += ",hostfwd=tcp:127.0.0.1:" + QString::number(adbPort->value()) + "-:5555";
    args << "-netdev" << net << "-device" << "virtio-net-pci,netdev=net0,romfile=";
    process->setProgram(qemu.absoluteFilePath()); process->setArguments(args);
    setRunning(true); embedAttempts = 0; embedTimer->start(250); process->start();
    statusBar()->showMessage("Starting QEMU with WHPX...");
}

void MainWindow::findGuestWindow() {
    if (process->state() != QProcess::Running) return;
    WindowSearch search{static_cast<DWORD>(process->processId())};
    EnumWindows(findWindow, reinterpret_cast<LPARAM>(&search));
    if (search.result && view->attach(search.result)) {
        embedTimer->stop(); statusBar()->showMessage("QEMU window embedded. Use Android's installer or boot the installed system.");
    } else if (++embedAttempts >= 120) {
        embedTimer->stop();
        statusBar()->showMessage("Could not embed the QEMU window; it may still be running separately.");
    }
}

void MainWindow::closeEvent(QCloseEvent *event) {
    saveSettings();
    if (process->state() != QProcess::NotRunning) {
        const auto choice = QMessageBox::question(this, "VM still running", "QEMU is still running. Force stop it and close? This can corrupt the guest disk. Choose No and shut down Android first.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
        if (choice != QMessageBox::Yes) { event->ignore(); return; }
        disconnect(process, nullptr, this, nullptr);
        process->kill(); process->waitForFinished(5000);
    }
    event->accept();
}
