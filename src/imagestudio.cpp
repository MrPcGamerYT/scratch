#include "windowchrome.h"
#include <QApplication>
#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDir>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMainWindow>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QProcess>
#include <QProgressBar>
#include <QPushButton>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTextCursor>
#include <QTimer>
#include <QVBoxLayout>
#include <memory>

class ImageStudio : public QMainWindow {
public:
    ImageStudio() {
        setWindowTitle("Nexus Image Studio · Developer Tools"); resize(1000, 800); setMinimumSize(760, 650);
        auto *central = new QWidget; setCentralWidget(central); auto *root = new QVBoxLayout(central); root->setContentsMargins(26, 18, 26, 20); root->setSpacing(12);
        auto *heading = new QLabel("IMAGE STUDIO"); heading->setStyleSheet("font-size:24px;font-weight:700;color:#f8fafc;"); root->addWidget(heading);
        auto *description = new QLabel("Prepare Bliss OS for Nexus • install, verify, then publish a fixed-name base image. No physical disks are attached."); description->setWordWrap(true); root->addWidget(description);
        auto *form = new QFormLayout; form->setVerticalSpacing(12); root->addLayout(form);
        mode = new QComboBox; mode->setObjectName("preparationMode"); mode->addItems({"Install from ISO / ZIP containing an ISO", "Import installed disk / ZIP containing a whole disk", "Resume retained installation workspace", "Extract OTA partitions (payload.bin / ZIP)"}); form->addRow("Workflow", mode);
        auto pathRow = [&](const QString &label, QLineEdit **field, bool directory) {
            auto *row = new QWidget; auto *line = new QHBoxLayout(row); line->setContentsMargins(0,0,0,0);
            *field = new QLineEdit; auto *edit = *field; line->addWidget(edit); auto *browse = new QPushButton("Browse…"); line->addWidget(browse);
            connect(browse, &QPushButton::clicked, this, [=] {
                QString value = directory ? QFileDialog::getExistingDirectory(this, label, edit->text()) : QFileDialog::getOpenFileName(this, label, edit->text(), "Images (*.bin *.zip *.iso *.qcow2 *.img *.raw *.vhd *.vhdx *.fd);;All files (*)");
                if (!value.isEmpty()) edit->setText(QDir::toNativeSeparators(value));
            });
            inputs.append(row); form->addRow(label, row);
        };
        pathRow("Source archive / image", &source, false);
        pathRow("Workspace folder", &workspace, true);
        pathRow("Nexus player folder", &player, true);
        pathRow("Imported UEFI variables", &vars, false);
        workspace->setText(QDir::toNativeSeparators(QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation) + "/Nexus Image Workspace"));
        vars->setPlaceholderText("Required for Bliss UEFI disk import; not needed for fresh installation");
        hash = new QLineEdit; hash->setPlaceholderText("Optional SHA-256 of the source ISO, ZIP or disk"); form->addRow("Source integrity", hash);
        bliss = new QCheckBox("Bliss OS · UEFI firmware (recommended for recent Bliss releases)"); bliss->setChecked(true); form->addRow(bliss);
        output = new QLabel; output->setObjectName("fixedOutputPath"); output->setTextInteractionFlags(Qt::TextSelectableByMouse); output->setWordWrap(true); form->addRow("Fixed emulator image", output);
        connect(player, &QLineEdit::textChanged, this, [this] { output->setText(QDir::toNativeSeparators(QDir(player->text()).absoluteFilePath("images/android-base.qcow2"))); });
        player->setText(QDir::toNativeSeparators(QCoreApplication::applicationDirPath()));
        auto *help = new QLabel("ZIP support: one ISO for Install, or one standalone whole-disk image for Import. Loose system.img / kernel packages need a bootloader installation and are rejected with a diagnostic. Existing player images are never overwritten. Shut down Nexus before publishing."); help->setWordWrap(true); help->setStyleSheet("color:#99a8bf;"); root->addWidget(help);
        auto *row = new QHBoxLayout; start = new QPushButton("Start preparation"); start->setObjectName("startPreparation"); row->addWidget(start);
        approve = new QPushButton("Confirm stage…"); approve->setEnabled(false); row->addWidget(approve);
        cancel = new QPushButton("Cancel at checkpoint"); cancel->setEnabled(false); row->addWidget(cancel); root->addLayout(row);
        auto *tools = new QHBoxLayout;
        auto *inspect = new QPushButton("Inspect OTA partitions…"); tools->addWidget(inspect);
        auto *edit = new QPushButton("Edit offline image files (WSLg)…"); tools->addWidget(edit); root->addLayout(tools);
        inputs.append(inspect); inputs.append(edit);
        connect(inspect, &QPushButton::clicked, this, [this] { runPayload(true); });
        connect(edit, &QPushButton::clicked, this, [this] {
            const auto editor = QCoreApplication::applicationDirPath() + "/offline-files/offline_editor.py";
            if (!QFileInfo::exists(editor)) { QMessageBox::warning(this,"Editor missing","Extract the complete Developer package."); return; }
            QMessageBox::information(this,"Offline file editor", "Requires Windows 11 WSLg and an Ubuntu WSL distribution with:\nsudo apt install python3-tk python3-guestfs qemu-utils libguestfs-tools\n\nThe editor opens a NEW copy, including raw OTA partition images. Compressed, EROFS, dynamic-super and verified-boot images may need additional tooling; edits do not rebuild or re-sign an OTA. Stop all VMs first.");
            const QStringList args{"--exec","bash","-lc","exec python3 \"$(wslpath -u \"$1\")\"","nexus-editor",editor};
            if (!QProcess::startDetached("wsl.exe", args)) QMessageBox::warning(this,"Could not start WSLg", "Install WSLg and the listed dependencies first.");
        });
        progress = new QProgressBar; progress->setRange(0,1); progress->setValue(0); progress->setTextVisible(false); root->addWidget(progress);
        status = new QLabel("Ready · source files stay unchanged; installation and boot verification are interactive."); status->setWordWrap(true); root->addWidget(status);
        log = new QPlainTextEdit; log->setReadOnly(true); log->setMaximumBlockCount(10000); log->setObjectName("preparationLog"); root->addWidget(log,1);
        auto *saveLog = new QPushButton("Export log…"); root->addWidget(saveLog,0,Qt::AlignRight);
        connect(saveLog, &QPushButton::clicked, this, [this] {
            auto path = QFileDialog::getSaveFileName(this,"Export preparation log", "image-studio.log");
            if (path.isEmpty()) return;
            QFile file(path); if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate) || file.write(log->toPlainText().toUtf8()) < 0) QMessageBox::warning(this,"Log export","Could not write the log.");
        });
        process = new QProcess(this); process->setProcessChannelMode(QProcess::MergedChannels);
        connect(process, &QProcess::readyReadStandardOutput, this, [this] { appendOutput(); });
        connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
            if (error == QProcess::FailedToStart) { log->appendPlainText(process->errorString()); finish(false); }
        });
        connect(process, qOverload<int,QProcess::ExitStatus>(&QProcess::finished), this, [this](int code,QProcess::ExitStatus exit) { appendOutput(); finish(code == 0 && exit == QProcess::NormalExit); });
        connect(start, &QPushButton::clicked, this, [this] { run(); });
        connect(approve, &QPushButton::clicked, this, [this] {
            if (QMessageBox::question(this,"Verify guest shutdown", pendingPrompt + "\n\nOnly confirm if Android completed the requested stage and shut down cleanly.", QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes) respond("YES");
        });
        connect(cancel, &QPushButton::clicked, this, [this] { respond("CANCEL"); });
        auto *timer = new QTimer(this); timer->setInterval(400);
        connect(timer, &QTimer::timeout, this, [this] {
            if (!approval || process->state() == QProcess::NotRunning) return;
            QFile request(approval->filePath("request.txt"));
            if (request.open(QIODevice::ReadOnly)) {
                pendingPrompt = QString::fromUtf8(request.readAll()); status->setText("Waiting for your confirmation: " + pendingPrompt);
                const bool answered = QFileInfo::exists(approval->filePath("approve.txt")); approve->setEnabled(!answered); cancel->setEnabled(!answered);
            } else { approve->setEnabled(false); cancel->setEnabled(false); }
        }); timer->start();
    }
protected:
    void closeEvent(QCloseEvent *event) override {
        if (process->state() != QProcess::NotRunning) {
            QMessageBox::information(this,"Preparation active","Shut down the VM cleanly, then use Cancel at checkpoint or complete the workflow. Closing now could interrupt image preparation."); event->ignore(); return;
        }
        event->accept();
    }
private:
    void appendOutput() { log->moveCursor(QTextCursor::End); log->insertPlainText(QString::fromLocal8Bit(process->readAllStandardOutput())); log->ensureCursorVisible(); }
    void finish(bool success) {
        progress->setRange(0,1); progress->setValue(success ? 1 : 0); start->setEnabled(true); mode->setEnabled(true); hash->setEnabled(true); bliss->setEnabled(true);
        for (auto *input : inputs) input->setEnabled(true);
        approve->setEnabled(false); cancel->setEnabled(false);
        if (payloadJob) { status->setText(success ? "OTA operation complete. See partition list / workspace/ota-partitions. These are NOT a bootable PC disk; no player image was changed." : "OTA operation failed. See log; source and player image were not changed."); return; }
        status->setText(success ? "Published images/android-base.qcow2. Start Nexus to use it; no disk renaming needed." : "Preparation stopped. Review the log. Existing player images and source files were not replaced; keep the workspace for Resume.");
        approval.reset();
    }
    void respond(const QByteArray &answer) {
        if (!approval) return;
        QFile response(approval->filePath("approve.txt"));
        if (!response.open(QIODevice::WriteOnly | QIODevice::NewOnly) || response.write(answer) != answer.size()) {
            QMessageBox::warning(this,"Checkpoint","Could not write the checkpoint response. Please retry."); return;
        }
        response.close(); approve->setEnabled(false); cancel->setEnabled(false);
    }
    void runPayload(bool inspectOnly) {
        if (!QFileInfo(source->text()).isFile() || workspace->text().trimmed().isEmpty()) { QMessageBox::warning(this,"OTA source", "Choose payload.bin or its ZIP and a workspace folder first."); return; }
        QString python = QStandardPaths::findExecutable("python");
        if (python.isEmpty()) python = QStandardPaths::findExecutable("python3");
        if (python.isEmpty()) { QMessageBox::warning(this,"Python required", "OTA extraction requires Python 3.10 or newer installed on Windows. It uses only the standard library."); return; }
        const auto script = QCoreApplication::applicationDirPath() + "/offline-files/payload_tool.py";
        if (!QFileInfo::exists(script)) { QMessageBox::warning(this,"Incomplete package", "OTA extraction script is missing. Extract the complete Developer package."); return; }
        QStringList args{script, source->text()};
        if (inspectOnly) args << "--list"; else args << "--output" << QDir(workspace->text()).absoluteFilePath("ota-partitions");
        if (!hash->text().trimmed().isEmpty()) args << "--sha256" << hash->text().trimmed();
        payloadJob = true;
        start->setEnabled(false); mode->setEnabled(false); hash->setEnabled(false); bliss->setEnabled(false); for (auto *input : inputs) input->setEnabled(false);
        log->clear(); status->setText("Inspecting / extracting OTA partitions. Full replacement operations only; incremental updates are refused."); progress->setRange(0,0);
        process->start(python,args);
    }
    void run() {
        if (mode->currentIndex() == 3) { runPayload(false); return; }
        payloadJob = false;
        if (player->text().trimmed().isEmpty() || workspace->text().trimmed().isEmpty() || (mode->currentIndex()!=2 && !QFileInfo(source->text()).isFile())) {
            QMessageBox::warning(this,"Missing paths","Choose a source file, workspace folder and Nexus player folder."); return;
        }
        const auto root = QCoreApplication::applicationDirPath();
        const auto script = root + "/image-studio.ps1";
        if (!QFileInfo::exists(script) || !QFileInfo::exists(root + "/qemu/bin/qemu-img.exe")) { QMessageBox::warning(this,"Incomplete package","Use the complete ImageStudio Developer package; backend scripts and qemu/bin must be next to this application."); return; }
        if (bliss->isChecked() && mode->currentIndex()==1 && !QFileInfo(vars->text()).isFile()) { QMessageBox::warning(this,"UEFI variables required","Choose the UEFI variables file from this installed Bliss disk. A fresh template may not preserve boot entries. For a fresh install use Install instead."); return; }
        QString shell = QStandardPaths::findExecutable("pwsh"); if (shell.isEmpty()) shell = QStandardPaths::findExecutable("powershell");
        if (shell.isEmpty()) { QMessageBox::warning(this,"PowerShell missing","Install PowerShell or enable Windows PowerShell to run the preparation backend."); return; }
        approval = std::make_unique<QTemporaryDir>();
        if (!approval->isValid()) { QMessageBox::warning(this,"Workspace","Cannot create checkpoint directory."); approval.reset(); return; }
        QStringList args{"-NoProfile","-ExecutionPolicy","Bypass","-File",script,"-Mode",QStringList{"Install","Import","Resume"}[mode->currentIndex()],"-Workspace",workspace->text(),"-PlayerFolder",player->text(),"-QemuDir",root+"/qemu/bin","-ApprovalDirectory",approval->path()};
        if (mode->currentIndex()!=2) args << "-Source" << source->text();
        if (!hash->text().trimmed().isEmpty()) args << "-ExpectedSha256" << hash->text().trimmed();
        if (bliss->isChecked()) args << "-Bliss";
        if (!vars->text().isEmpty()) args << "-SourceVars" << vars->text();
        start->setEnabled(false); mode->setEnabled(false); hash->setEnabled(false); bliss->setEnabled(false); for (auto *input : inputs) input->setEnabled(false);
        log->clear(); status->setText("Preparing… Watch this log and the VM window. Confirm each checkpoint only after clean shutdown."); progress->setRange(0,0);
        process->setWorkingDirectory(root); process->start(shell,args);
    }
    QComboBox *mode;
    QLineEdit *source, *workspace, *player, *vars, *hash;
    QCheckBox *bliss;
    QLabel *output, *status;
    QPushButton *start, *approve, *cancel;
    QPlainTextEdit *log;
    QProgressBar *progress;
    QProcess *process;
    QList<QWidget *> inputs;
    std::unique_ptr<QTemporaryDir> approval;
    QString pendingPrompt;
    bool payloadJob = false;
};

int main(int argc, char **argv) {
    QApplication app(argc,argv); app.setStyle("Fusion");
    app.setStyleSheet("QWidget{background:#0d111a;color:#d5dfee;font-family:'Segoe UI';font-size:13px;} QLineEdit,QComboBox,QPlainTextEdit{background:#141c2b;border:1px solid #303f58;border-radius:6px;padding:8px;} QPushButton{background:#203555;border:1px solid #365681;border-radius:6px;padding:9px 16px;} QPushButton:hover{background:#2b4b78;} QPushButton:disabled{color:#667085;background:#19202c;} QProgressBar{background:#151d2b;border:0;max-height:5px;} QProgressBar::chunk{background:#3b82f6;}");
    WindowChrome chrome; app.installEventFilter(&chrome);
    ImageStudio window;
    if (app.arguments().contains("--smoke-test")) {
        window.show(); app.processEvents();
        return window.windowFlags().testFlag(Qt::FramelessWindowHint) && window.findChild<QComboBox *>("preparationMode")->count()==4 && window.findChild<QLabel *>("fixedOutputPath")->text().endsWith("android-base.qcow2") ? 0 : 1;
    }
    window.show(); return app.exec();
}
