#include "settingsdialog.h"
#include "diagnostics.h"
#include "resourceconfig.h"
#include "hostgraphics.h"
#include <QCoreApplication>
#include <QPlainTextEdit>
#include <QClipboard>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QSettings>
#include <QSysInfo>
#include <QThread>
#include <QSpinBox>
#include <QTabWidget>
#include <QVBoxLayout>
#include <memory>

namespace {
QWidget *page(const QString &title, const QString &subtitle, QFormLayout **form) {
    auto *widget = new QWidget;
    auto *layout = new QVBoxLayout(widget); layout->setContentsMargins(28, 24, 28, 24); layout->setSpacing(16);
    auto *heading = new QLabel(title); heading->setObjectName("sectionTitle"); layout->addWidget(heading);
    auto *detail = new QLabel(subtitle); detail->setObjectName("muted"); detail->setWordWrap(true); layout->addWidget(detail);
    auto *card = new QFrame; card->setObjectName("settingsCard");
    *form = new QFormLayout(card); (*form)->setContentsMargins(22, 22, 22, 22);
    (*form)->setVerticalSpacing(22); (*form)->setHorizontalSpacing(28);
    (*form)->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    (*form)->setRowWrapPolicy(QFormLayout::WrapLongRows);
    layout->addWidget(card); layout->addStretch(); return widget;
}
QComboBox *unavailable(const QString &value) {
    auto *box = new QComboBox; box->addItem(value); box->setEnabled(false);
    box->setToolTip("Not implemented by the current emulator backend."); return box;
}
void note(QFormLayout *form, const QString &text) {
    auto *label = new QLabel(text); label->setObjectName("muted"); label->setWordWrap(true); form->addRow(label);
}
}

SettingsDialog::SettingsDialog(bool running, QWidget *parent) : QDialog(parent) {
    setWindowTitle("Settings · Project Nexus"); resize(1120, 670); setMinimumSize(680, 530);
    setObjectName("emulatorSettings");
    setStyleSheet(R"(
      QDialog#emulatorSettings { background:#0d0f14; color:#f3f4f6; }
      #emulatorSettings QWidget { color:#e5e7eb; font-family:'Segoe UI'; font-size:13px; }
      #emulatorSettings QLabel { background:transparent; }
      #emulatorSettings QLabel#dialogTitle { font-size:16px; font-weight:600; }
      #emulatorSettings QLabel#sectionTitle { font-size:18px; font-weight:600; }
      #emulatorSettings QLabel#muted { color:#929aaa; font-size:12px; }
      #emulatorSettings QLabel#statusPill { color:#7dd3b0; background:#151c22; border:1px solid #26343a; border-radius:12px; padding:5px 12px; }
      #emulatorSettings QLabel#dirtyBadge { color:#fbbf24; }
      #emulatorSettings QFrame#settingsCard { background:#12141b; border:1px solid #222631; border-radius:12px; }
      #emulatorSettings QTabWidget::pane { border:0; border-top:1px solid #222631; background:#0d0f14; }
      #emulatorSettings QTabBar::tab { background:#0a0c10; color:#9ca3af; padding:14px 13px; border-bottom:3px solid transparent; }
      #emulatorSettings QTabBar::tab:selected { color:white; background:#142039; border-bottom:3px solid #3b82f6; }
      #emulatorSettings QTabBar::tab:hover { color:white; background:#171c27; }
      #emulatorSettings QScrollArea { border:0; background:#0d0f14; }
      #emulatorSettings QSpinBox, #emulatorSettings QComboBox { background:#151821; border:1px solid #303647; border-radius:8px; padding:9px; min-width:180px; }
      #emulatorSettings QSpinBox:focus, #emulatorSettings QComboBox:focus { border-color:#3b82f6; }
      #emulatorSettings QComboBox:disabled { color:#697284; background:#11131a; }
      #emulatorSettings QCheckBox { spacing:12px; padding:5px 0; }
      #emulatorSettings QCheckBox::indicator { width:34px; height:19px; border-radius:9px; background:#303747; border:1px solid #424b60; }
      #emulatorSettings QCheckBox::indicator:checked { background:#2563eb; border-color:#60a5fa; }
      #emulatorSettings QPushButton { background:#181c26; border:1px solid #303749; border-radius:8px; padding:9px 18px; }
      #emulatorSettings QPushButton:hover { background:#242d40; }
      #emulatorSettings QPushButton#saveSettings { background:#2563eb; border-color:#3b82f6; color:white; font-weight:600; }
      #emulatorSettings QPushButton:disabled, #emulatorSettings QPushButton#saveSettings:disabled { color:#677083; background:#161922; border-color:#242834; }
      #emulatorSettings QScrollBar:vertical { background:#0d0f14; width:6px; }
      #emulatorSettings QScrollBar::handle:vertical { background:#303747; border-radius:3px; min-height:24px; }
    )");
    auto *layout = new QVBoxLayout(this); layout->setContentsMargins(0, 0, 0, 0); layout->setSpacing(0);
    auto *header = new QHBoxLayout; header->setContentsMargins(22, 16, 22, 16);
    auto *status = new QLabel("QEMU · x86_64"); status->setObjectName("statusPill"); header->addWidget(status);
    auto *dirty = new QLabel("● Unsaved"); dirty->setObjectName("dirtyBadge"); dirty->hide(); header->addWidget(dirty);
    header->addStretch(); auto *title = new QLabel("Settings"); title->setObjectName("dialogTitle"); header->addWidget(title); header->addStretch();
    auto *close = new QPushButton("✕"); close->setAccessibleName("Close settings"); header->addWidget(close); layout->addLayout(header);
    auto *tabs = new QTabWidget; tabs->setUsesScrollButtons(true); tabs->setDocumentMode(true); layout->addWidget(tabs, 1);
    auto addPage = [&](const QString &name, const QString &description, QFormLayout **form) {
        auto *scroll = new QScrollArea; scroll->setWidgetResizable(true);
        auto *widget = page(name, description, form); scroll->setWidget(widget); tabs->addTab(scroll, name); return widget;
    };
    QFormLayout *form = nullptr;
    auto *performance = addPage("Performance", "Allocate host resources to this instance. Changes apply on next launch.", &form);
    cpus = new QSpinBox(performance); cpus->setRange(1, 64); cpus->setObjectName("cpuAllocation");
    memory = new QSpinBox(performance); memory->setRange(512, 131072); memory->setSingleStep(512); memory->setSuffix(" MB"); memory->setObjectName("memoryAllocation");
    autoResources = new QCheckBox("Automatic CPU and memory allocation", performance); autoResources->setObjectName("automaticResources"); form->addRow(autoResources);
    note(form, "Automatic uses half the logical CPU threads (up to 8) and approximately one quarter of physical RAM (512–8192 MB). This is a conservative starting point, not a fastest-game preset. Low-memory hosts may not meet the guest OS minimum. Manual values remain available. Applies on next launch.");
    form->addRow("CPU allocation", cpus); form->addRow("Memory allocation", memory);
    note(form, QString("Detected host: %1 logical CPU threads. Leave resources available for Windows; over-allocation can reduce performance.").arg(qMax(1, QThread::idealThreadCount())));
    acceleration = new QComboBox; acceleration->setObjectName("vmAcceleration");
    acceleration->addItem("Use prepared image's selection", "prepared");
    acceleration->addItem("Hardware acceleration (WHPX)", "whpx");
    acceleration->addItem("Software compatibility (TCG) — slow", "tcg");
    form->addRow("CPU execution", acceleration);
    note(form, "Software compatibility bypasses WHPX MMIO crashes, but can be extremely slow. There is no automatic fallback or compatibility guarantee. Restart player after changes.");
    form->addRow("Frame rate", unavailable("Guest controlled · no FPS limiter"));
    showFps = new QCheckBox("Show engine presentation FPS"); showFps->setObjectName("showFps"); form->addRow(showFps);
    hideBoot = new QCheckBox("Hide guest display until Android boot completes"); hideBoot->setObjectName("hideBoot"); form->addRow(hideBoot);
    note(form, "Engine FPS counts completed SDL 2D presentation calls in the instrumented engine, not game FPS or monitor scanout. It does not use ADB. Unsupported engines/OpenGL renderers show N/A. Native guest images provide app detection and boot status without ADB; older images require authorized debugging. Boot hiding defaults to on; without a usable status connection it falls back after three minutes. Boot hiding does not accelerate boot or skip setup. The display is revealed with a warning after three minutes without boot confirmation. Turning this option off reveals it during the current session; turning it on requires restart.");
    auto *display = addPage("Display", "Host cursor preferences and guest display capabilities.", &form);
    auto *resolution = new QComboBox(display); resolution->setObjectName("guestResolution");
    resolution->addItem("Android default", "reset");
    for (const auto &size : {"1280x720", "1600x900", "1920x1080", "720x1280", "1080x1920"}) resolution->addItem(size, size);
    auto *density = new QComboBox(display); density->setObjectName("guestDensity");
    density->addItem("Android default", 0);
    for (int dpi : {160, 200, 240, 280, 320, 400, 480}) density->addItem(QString::number(dpi) + " DPI", dpi);
    form->addRow("Resolution", resolution); form->addRow("Pixel density (DPI)", density);
    auto *apply = new QPushButton("Apply to running Android", display); apply->setObjectName("applyGuestDisplay"); apply->setEnabled(running); form->addRow(apply);
    note(form, "Requires platform-tools/adb.exe, local ADB forwarding enabled before launch, and authorized TCP debugging inside Android. Applies immediately, not on Save. Android retains overrides; select Android default in both fields and Apply to reset. Some apps may restart; keymap positions may need adjustment. This does not enable GPU acceleration.");
    connect(apply, &QPushButton::clicked, this, [=] { if (applyDisplay) applyDisplay(resolution->currentData().toString(), density->currentData().toInt()); });
    showCursor = new QCheckBox("Show host mouse cursor", display); form->addRow(showCursor);
    addPage("Graphics", "Current backend capabilities — not simulated graphics options.", &form);
    graphics = new QComboBox; graphics->setObjectName("vmGraphics");
    graphics->addItem("Use prepared image's selection", "prepared"); graphics->addItem("Standard VGA · compatibility", "std"); graphics->addItem("VirtIO VGA · experimental", "virtio"); graphics->addItem("OpenGL / VirGL · experimental backend", "virgl");
    form->addRow("Host GPU preference", new QLabel("Automatic / Windows-managed window"));
    note(form, "Integrated/dedicated GPU inventory is shown below. VirGL is an opt-in OpenGL backend under validation, not automatic dedicated-GPU selection or proof of accelerated Android rendering.");
    auto *gpuReport = new QPlainTextEdit(hostGraphicsReport()); gpuReport->setReadOnly(true); gpuReport->setMaximumHeight(150); gpuReport->setObjectName("hostGpuReport"); form->addRow(gpuReport);
    form->addRow("Display device", graphics);
    note(form, "OpenGL/VirGL requires a GL-enabled QEMU build, a working host OpenGL context and compatible guest Mesa drivers. Failures are reported, never silently replaced with scanout. Android/Windows rendering is not yet certified. Restart required. VirtIO VGA provides an alternative scanout device, not host GPU acceleration. Guest virtio-gpu/DRM drivers are required. Test in Image Studio Resume first. A black/striped display does not prove a device change will fix it. Temporary nomodeset settings can disable the required driver; restart without those experimental boot edits when testing VirtIO."); form->addRow("ASTC textures", unavailable("Not supported"));
    note(form, "Vulkan, DirectX, GPU passthrough and dedicated-GPU selection are not implemented in this player.");
    auto *games = addPage("Game settings", "Configure the native keymapping runtime.", &form);
    mouseInput = new QComboBox(games); mouseInput->setObjectName("mouseInputBackend");
    mouseInput->addItem("Compatibility cursor deltas", false); mouseInput->addItem("Experimental relative raw mouse", true);
    form->addRow("Aim mouse input", mouseInput);
    aimInterval = new QComboBox(games); aimInterval->setObjectName("aimBatchInterval");
    aimInterval->addItem("16 ms · compatibility", 16); aimInterval->addItem("8 ms · lower batch delay", 8); aimInterval->addItem("4 ms · more host work", 4);
    form->addRow("Aim movement batching", aimInterval);
    note(form, "Save then release/reactivate aim to apply. Raw mode uses relative device counts (sensitivity may need retuning), bypasses cursor-position deltas, and rejects absolute devices. Unsupported registration fails visibly rather than silently changing mode. Smaller batching intervals can increase CPU/QMP traffic and do not guarantee lower game latency. Diagnostics reports host-only counters.");
    multitouch = new QCheckBox("Experimental multi-touch keymapping", games); form->addRow(multitouch);
    note(form, "Requires virtio multitouch support in the Android guest and a player restart. Single-pointer fallback remains available.");
    note(form, "Open Keymap Studio from the player toolbar for schemes, D-pad, taps, swipes, pinch zoom, MOBA skills and scripts. Aim settings configure ADS and free-look.");
    addPage("Devices", "Audio and camera forwarding capabilities.", &form);
    note(form, "Guest sound is enabled automatically using Intel HDA output and the Windows default playback device. A compatible guest audio driver is required. Microphone capture, camera forwarding and per-device routing are not implemented.");
    form->addRow("Audio output", new QLabel("Automatic · Windows default playback device"));
    form->addRow("Camera", unavailable("Not supported"));
    addPage("Gamepad", "Controller integration status.", &form); form->addRow("Gamepad detection", unavailable("Not implemented")); note(form, "Keyboard and mouse mapping are available in Keymap Studio. Controller rumble and XInput mapping are not implemented.");
    addPage("Preferences", "Player configuration and save behavior.", &form); note(form, "Settings are saved locally with QSettings. Save writes only supported options; Discard restores your saved values without closing this window.");
    addPage("Phone", "Device identity is controlled by the installed Android system image.", &form); form->addRow("Phone profile", unavailable("Guest integration required")); note(form, "No device spoofing, carrier override or high-FPS game unlock is provided.");
    addPage("Shortcuts", "Input bindings are managed per control scheme.", &form); note(form, "Use Keymap Studio's key capture buttons to assign keyboard, mouse and wheel bindings. Conflict diagnostics flag overlapping assignments. See the player toolbar for available emulator actions.");
    addPage("User data", "Your guest disk and mapping profiles remain local.", &form); note(form, "Managed disk overlays protect the base image. Use the existing image tools for offline changes. Automatic cleanup and cloud backup are not implemented; no disk space is reclaimed by this dialog.");
    auto *advanced = addPage("Advanced", "Local developer access. Forwarding does not enable ADB inside Android.", &form);
    adbForward = new QCheckBox("Enable local ADB forwarding", advanced); form->addRow(adbForward);
    adbPort = new QSpinBox(advanced); adbPort->setRange(1024, 65535); form->addRow("Local ADB port", adbPort);
    form->addRow("Root access", unavailable("Controlled by guest image")); note(form, "Forwarding binds to loopback. Enable Android debugging in the guest separately. Restart required.");
    auto *diagnosticsPage = addPage("Diagnostics", "Read-only package checks and current-session events. Review local paths before sharing.", &form);
    auto *report = new QPlainTextEdit(diagnosticsPage); report->setObjectName("diagnosticsReport"); report->setReadOnly(true); report->setMinimumHeight(210);
    report->setPlainText(playerDiagnostics(QCoreApplication::applicationDirPath())); form->addRow(report);
    auto *refreshReport = new QPushButton("Refresh diagnostics", diagnosticsPage); refreshReport->setObjectName("refreshDiagnostics"); form->addRow(refreshReport);
    auto *copyReport = new QPushButton("Copy report and session log", diagnosticsPage); copyReport->setObjectName("copyDiagnostics"); form->addRow(copyReport);
    auto refresh = [=] { report->setPlainText(playerDiagnostics(QCoreApplication::applicationDirPath()) + "\n\n" + hostGraphicsReport() + (sessionDiagnostics ? "\n\nCurrent session:\n" + sessionDiagnostics() : "\n\nNo player session attached.")); };
    connect(refreshReport, &QPushButton::clicked, this, refresh);
    connect(copyReport, &QPushButton::clicked, this, [=] { refresh(); QApplication::clipboard()->setText(report->toPlainText()); });
    addPage("About", "Project Nexus · native Qt player and Rust gesture core", &form);
    note(form, QString("Host: %1 · %2\n").arg(QSysInfo::prettyProductName(), QSysInfo::currentCpuArchitecture()));
    note(form, "Engine: QEMU x86_64\nInput: QMP pointer / experimental virtio multitouch\nKeymaps: v3 profiles with legacy import\n\nAndroid version, virtualization availability and game compatibility depend on the installed guest and host. No ARM translation or commercial-emulator compatibility is claimed.");
    auto *footer = new QHBoxLayout; footer->setContentsMargins(24, 16, 24, 16);
    auto *notice = new QLabel(running ? "ⓘ  Changes apply on next player launch" : "ⓘ  Some changes apply on next launch"); notice->setObjectName("muted"); footer->addWidget(notice); footer->addStretch();
    auto *discard = new QPushButton("Discard"); auto *saveButton = new QPushButton("Save changes"); saveButton->setObjectName("saveSettings"); footer->addWidget(discard); footer->addWidget(saveButton); layout->addLayout(footer);
    auto snapshot = [this] { return QString("%1/%2/%3/%4/%5/%6/%7/%8/%9/%10/%11/%12/%13").arg(cpus->value()).arg(memory->value()).arg(showCursor->isChecked()).arg(multitouch->isChecked()).arg(adbForward->isChecked()).arg(adbPort->value()).arg(acceleration->currentData().toString()).arg(mouseInput->currentIndex()).arg(aimInterval->currentIndex()).arg(graphics->currentIndex()).arg(autoResources->isChecked()).arg(showFps->isChecked()).arg(hideBoot->isChecked()); };
    auto baseline = std::make_shared<QString>();
    auto update = [=] { bool changed = snapshot() != *baseline; dirty->setVisible(changed); discard->setEnabled(changed); saveButton->setEnabled(changed); adbPort->setEnabled(adbForward->isChecked()); cpus->setEnabled(!autoResources->isChecked()); memory->setEnabled(!autoResources->isChecked()); };
    auto restore = [=] {
        QSettings s;
        showFps->setChecked(s.value("showFps", false).toBool()); hideBoot->setChecked(s.value("hideBoot", true).toBool());
        autoResources->setChecked(automaticResources(s));
        const auto resources = selectedResources(s);
        cpus->setValue(static_cast<int>(resources.cpus)); memory->setValue(static_cast<int>(resources.memory_mb));
        showCursor->setChecked(s.value("showCursor", true).toBool()); multitouch->setChecked(s.value("multiTouch", false).toBool());
        adbForward->setChecked(s.value("adbForward", false).toBool()); adbPort->setValue(s.value("adbPort", 5555).toInt());
        acceleration->setCurrentIndex(qMax(0, acceleration->findData(s.value("acceleration", "prepared").toString())));
        graphics->setCurrentIndex(qMax(0, graphics->findData(s.value("graphics", "prepared").toString())));
        mouseInput->setCurrentIndex(s.value("input/rawMouse", false).toBool() ? 1 : 0);
        aimInterval->setCurrentIndex(qMax(0, aimInterval->findData(s.value("input/aimInterval", 16).toInt())));
        *baseline = snapshot(); update();
    };
    connect(autoResources, &QCheckBox::toggled, this, [=](bool enabled) {
        if (!enabled) return;
        const auto resources = nexus_resource_budget(qMax(1, QThread::idealThreadCount()), nexus_host_memory_mb());
        cpus->setValue(static_cast<int>(resources.cpus)); memory->setValue(static_cast<int>(resources.memory_mb));
    });
    for (auto *spin : {cpus, memory, adbPort}) connect(spin, &QSpinBox::valueChanged, this, [=](int) { update(); });
    for (auto *check : {showCursor, multitouch, adbForward, autoResources, showFps, hideBoot}) connect(check, &QCheckBox::toggled, this, [=](bool) { update(); });
    for (auto *combo : {acceleration, mouseInput, aimInterval, graphics}) connect(combo, &QComboBox::currentIndexChanged, this, [=](int) { update(); });
    connect(discard, &QPushButton::clicked, this, restore);
    connect(saveButton, &QPushButton::clicked, this, [=] { if (!save()) return; *baseline = snapshot(); update(); notice->setText("✓  Saved locally · restart player to apply"); });
    hasChanges = [=] { return snapshot() != *baseline; };
    connect(close, &QPushButton::clicked, this, &SettingsDialog::reject);
    restore();
}
void SettingsDialog::reject() {
    if (hasChanges && hasChanges() && QMessageBox::question(this, "Unsaved settings", "Discard your unsaved changes?", QMessageBox::Discard | QMessageBox::Cancel, QMessageBox::Cancel) != QMessageBox::Discard) return;
    QDialog::reject();
}
bool SettingsDialog::save() {
    QSettings s; s.setValue("showFps", showFps->isChecked()); s.setValue("hideBoot", hideBoot->isChecked()); s.setValue("automaticResources", autoResources->isChecked()); s.setValue("cpus", cpus->value()); s.setValue("memory", memory->value());
    s.setValue("showCursor", showCursor->isChecked()); s.setValue("multiTouch", multitouch->isChecked());
    s.setValue("adbForward", adbForward->isChecked()); s.setValue("adbPort", adbPort->value());
    s.setValue("acceleration", acceleration->currentData());
    s.setValue("graphics", graphics->currentData());
    s.setValue("input/rawMouse", mouseInput->currentData());
    s.setValue("input/aimInterval", aimInterval->currentData());
    s.sync();
    if (s.status() != QSettings::NoError) {
        QMessageBox::critical(this, "Settings not saved", "Could not write local settings. Check your user configuration folder permissions and free space.");
        return false;
    }
    return true;
}
