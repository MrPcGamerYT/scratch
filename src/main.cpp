#include "mainwindow.h"
#include <QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setOrganizationName("ProjectNexus");
    QApplication::setApplicationName("Project Nexus");
    app.setStyle("Fusion");
    app.setStyleSheet(R"(
        QWidget { background: #0b111d; color: #e6edf7; font-family: 'Segoe UI'; font-size: 10pt; }
        QMainWindow, QDialog { background: #0b111d; }
        QLabel { background: transparent; }
        QLabel#brand { font-size: 19pt; font-weight: 700; color: #f4f7ff; }
        QLabel#eyebrow { color: #7b91ae; font-size: 9pt; font-weight: 600; }
        QLabel#sectionTitle { color: #f1f6ff; font-size: 13pt; font-weight: 700; }
        QLabel#muted { color: #90a2ba; font-size: 9pt; }
        QLabel#statusPill { background: #19293a; color: #91d8be; border: 1px solid #294756; border-radius: 10px; padding: 5px 10px; }
        QFrame#header { background: #101a29; border-bottom: 1px solid #263449; }
        QFrame#sidePanel { background: #111b2a; border: 1px solid #263449; border-radius: 12px; }
        QFrame#viewportPanel { background: #101927; border: 1px solid #263449; border-radius: 12px; }
        QGroupBox { border: 1px solid #2a3a50; border-radius: 9px; margin-top: 14px; padding: 14px 10px 10px; font-weight: 600; color: #c6d7ed; }
        QGroupBox::title { subcontrol-origin: margin; left: 12px; padding: 0 5px; }
        QPushButton { background: #24344a; border: 1px solid #344c68; border-radius: 7px; padding: 8px 12px; font-weight: 600; }
        QPushButton:hover { background: #304561; }
        QPushButton:pressed { background: #192a40; }
        QPushButton:disabled { color: #617189; background: #182330; border-color: #273444; }
        QPushButton#primary { background: #367bfa; border-color: #6098ff; color: white; }
        QPushButton#primary:hover { background: #4b8dff; }
        QLineEdit, QSpinBox, QComboBox, QListWidget { background: #0c1523; border: 1px solid #34455c; border-radius: 6px; padding: 6px; selection-background-color: #367bfa; }
        QLineEdit:focus, QSpinBox:focus, QComboBox:focus { border: 1px solid #548dff; }
        QComboBox QAbstractItemView { background: #182637; selection-background-color: #367bfa; }
        QCheckBox { spacing: 8px; }
        QCheckBox::indicator { width: 16px; height: 16px; border: 1px solid #536a87; border-radius: 4px; background: #0c1523; }
        QCheckBox::indicator:checked { background: #367bfa; border-color: #6098ff; }
        QScrollArea { border: 0; background: transparent; }
        QStatusBar { background: #101a29; color: #a9bbd2; }
        QProgressBar { border: 1px solid #34455c; border-radius: 5px; text-align: center; }
        QProgressBar::chunk { background: #367bfa; border-radius: 4px; }
    )");
    MainWindow window;
    window.show();
    return app.exec();
}
