#include <QIcon>
#include "mainwindow.h"
#include "windowchrome.h"
#include <QApplication>
#include <QTimer>

namespace {
MainWindow *fullscreenPlayer = nullptr;
bool fullscreenHeld = false;
LRESULT CALLBACK fullscreenHook(int code, WPARAM message, LPARAM data) {
    if (code == HC_ACTION && fullscreenPlayer) {
        const auto *event = reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
        if (event->vkCode == VK_F11 && !(event->flags & LLKHF_INJECTED)) {
            if ((message == WM_KEYUP || message == WM_SYSKEYUP) && fullscreenHeld) { fullscreenHeld = false; return 1; }
            const HWND root = reinterpret_cast<HWND>(fullscreenPlayer->winId());
            const HWND foreground = GetForegroundWindow();
            if (!QApplication::activeModalWidget() && (foreground == root || IsChild(root, foreground)) &&
                (message == WM_KEYDOWN || message == WM_SYSKEYDOWN) && !(GetAsyncKeyState(VK_MENU) & 0x8000) && !(GetAsyncKeyState(VK_CONTROL) & 0x8000)) {
                if (!fullscreenHeld) {
                    fullscreenHeld = true;
                    QTimer::singleShot(0, fullscreenPlayer, [] { if (fullscreenPlayer->isFullScreen()) fullscreenPlayer->showNormal(); else fullscreenPlayer->showFullScreen(); });
                }
                return 1;
            }
        }
    }
    return CallNextHookEx(nullptr, code, message, data);
}
}

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setWindowIcon(QIcon(":/assets/nexus-engine.png"));
    QApplication::setOrganizationName("ProjectNexus");
    QApplication::setApplicationName("Project Nexus");
    app.setStyle("Fusion");
    app.setStyleSheet(R"(
        QWidget { background: #10111a; color: #eeeef8; font-family: 'Segoe UI'; font-size: 10pt; }
        QMainWindow, QDialog { background: #11121c; }
        QLabel { background: transparent; }
        QLabel#logoMark { background: #3b82f6; color: #fff; font-size: 19pt; font-weight: 800; border-radius: 11px; }
        QLabel#brand { font-size: 13pt; font-weight: 800; letter-spacing: 2px; color: #f8f7ff; }
        QLabel#eyebrow { color: #777a91; font-size: 8pt; font-weight: 700; letter-spacing: 1px; }
        QLabel#sectionTitle { color: #f5f3ff; font-size: 16pt; font-weight: 700; }
        QLabel#dialogTitle { color: #f7f6ff; font-size: 20pt; font-weight: 700; }
        QLabel#muted { color: #9b9db3; font-size: 9pt; }
        QLabel#statusPill { background: #1c202c; color: #a7aaba; border: 1px solid #2c2e3b; border-radius: 11px; padding: 5px 10px; font-size: 9pt; }
        QFrame#header { background: qlineargradient(x1:0,y1:0,x2:1,y2:0,stop:0 #0c1423,stop:1 #111a2b); border-bottom: 1px solid #243e62; }
        QFrame#viewportPanel { background: #0b0d17; border: 1px solid #282a3a; border-radius: 12px; }
        QFrame#topDock { background: #101b2d; border: 1px solid #273d5c; border-radius: 12px; }
        QPushButton { background: #242633; border: 1px solid #363849; border-radius: 8px; padding: 8px 14px; font-weight: 600; }
        QPushButton:hover { background: #343546; }
        QPushButton:pressed { background: #20212c; }
        QPushButton:disabled { color: #666777; background: #1c1d28; border-color: #292b37; }
        QPushButton#primary { background: #2563eb; border-color: #60a5fa; color: white; padding: 9px 16px; }
        QPushButton#primary:hover { background: #3b82f6; }
        QPushButton#dockButton { background: transparent; border: 1px solid transparent; color: #b8b8cb; padding: 2px 3px; font-size: 9pt; font-weight: 500; }
        QPushButton#dockButton:hover { background: #2a293b; border-color: #46405e; color: #f5f3ff; }
        QPushButton#dockButton:disabled { color: #5e6070; }
        QLineEdit, QSpinBox, QComboBox, QListWidget { background: #1b1c29; border: 1px solid #363748; border-radius: 7px; padding: 7px; selection-background-color: #2563eb; }
        QLineEdit:focus, QSpinBox:focus, QComboBox:focus { border: 1px solid #60a5fa; }
        QComboBox QAbstractItemView { background: #21222f; selection-background-color: #2563eb; }
        QListWidget#settingsNav { border: 0; background: #191a26; padding: 6px; }
        QListWidget#settingsNav::item { border-radius: 6px; padding: 10px 8px; }
        QListWidget#settingsNav::item:selected { background: #3d345f; color: #f7f4ff; }
        QCheckBox { spacing: 8px; }
        QCheckBox::indicator { width: 16px; height: 16px; border: 1px solid #626477; border-radius: 4px; background: #1b1c29; }
        QCheckBox::indicator:checked { background: #2563eb; border-color: #60a5fa; }
        QStatusBar { background: #161722; color: #999caf; }
        QProgressBar { border: 1px solid #393849; border-radius: 5px; text-align: center; }
        QProgressBar::chunk { background: #2563eb; border-radius: 4px; }
    )");
    WindowChrome chrome;
    app.installEventFilter(&chrome);
    MainWindow window;
    window.show();
    fullscreenPlayer = &window;
    const HHOOK fullscreenKeyboard = SetWindowsHookExW(WH_KEYBOARD_LL, fullscreenHook, nullptr, 0);
    const int result = app.exec();
    if (fullscreenKeyboard) UnhookWindowsHookEx(fullscreenKeyboard);
    fullscreenPlayer = nullptr;
    return result;
}
