#pragma once
#include <QFileDialog>
#include <QFileInfo>
#include <QDir>

// A Qt-owned picker avoids shell/COM lifetime issues and is deliberately excluded
// from application title-bar rewriting. Cancellation never changes the field.
class StudioPathDialog : public QFileDialog {
public:
    StudioPathDialog(QWidget *parent, const QString &title, const QString &initial, bool directory)
        : QFileDialog(parent, title) {
        setObjectName("studioPathDialog");
        setOption(QFileDialog::DontUseNativeDialog, true);
        setAcceptMode(QFileDialog::AcceptOpen);
        setFileMode(directory ? QFileDialog::Directory : QFileDialog::ExistingFile);
        setOption(QFileDialog::ShowDirsOnly, directory);
        if (!directory) setNameFilter("Bliss PC installer ISO (*.iso *.ISO)");
        const QFileInfo path(initial);
        if (path.isDir()) setDirectory(path.absoluteFilePath());
        else if (path.exists()) { setDirectory(path.absolutePath()); if (!directory) selectFile(path.fileName()); }
        else setDirectory(QDir::homePath());
    }
};
inline QString chooseStudioPath(QWidget *parent, const QString &title, const QString &initial, bool directory) {
    StudioPathDialog dialog(parent, title, initial, directory);
    if (dialog.exec() != QDialog::Accepted || dialog.selectedFiles().isEmpty()) return {};
    return dialog.selectedFiles().first();
}
