#include "studiobrowse.h"
#include "windowchrome.h"
#include <QApplication>
#include <QFile>
#include <QTemporaryDir>
#include <QTimer>
#include <QVariant>
#include <iostream>
int main(int argc, char **argv) {
    QApplication app(argc, argv);
    WindowChrome chrome; app.installEventFilter(&chrome);
    QTemporaryDir temporary;
    if (!temporary.isValid()) return 1;
    const auto iso = temporary.filePath("Bliss installer.iso");
    QFile file(iso); if (!file.open(QIODevice::WriteOnly)) return 2; file.write("fixture"); file.close();
    QWidget owner; owner.show();
    StudioPathDialog picker(&owner,"Choose ISO",temporary.path(),false);
    const auto flags = picker.windowFlags();
    bool stayedOpen = false;
    QTimer::singleShot(350, &picker, [&] {
        stayedOpen = picker.isVisible() && picker.windowFlags() == flags && !picker.property("nexusChrome").toBool();
        picker.selectFile(iso);
        static_cast<QDialog *>(&picker)->accept();
    });
    if (picker.exec() != QDialog::Accepted || !stayedOpen || picker.selectedFiles().value(0) != iso) return 3;
    StudioPathDialog folders(&owner,"Choose workspace",temporary.path(),true);
    QTimer::singleShot(350, &folders, [&] {
        stayedOpen = folders.isVisible() && !folders.property("nexusChrome").toBool();
        folders.reject();
    });
    if (folders.exec() != QDialog::Rejected || !stayedOpen) return 4;
    // Test the exact helper used by Browse, including cancellation.
    QTimer::singleShot(350, &app, [&] {
        for (auto *widget : QApplication::topLevelWidgets())
            if (auto *dialog = qobject_cast<QFileDialog *>(widget); dialog && dialog->isVisible()) dialog->reject();
    });
    if (!chooseStudioPath(&owner,"Cancel ISO",iso,false).isEmpty()) return 5;
    std::cout << "ISO/folder pickers stay open with chrome filter installed; select and cancel passed.\n";
    return 0;
}
