#pragma once
#include "keymap.h"
#include <QDialog>
#include <QPixmap>
class KeymapCanvas;
class QComboBox;
class QListWidget;
class QSpinBox;

class KeymapDialog : public QDialog {
public:
    KeymapDialog(const QPixmap &screenshot, const QList<KeyBinding> &bindings, QWidget *parent = nullptr);
    QList<KeyBinding> resultBindings() const;
private:
    void updateList();
    KeymapCanvas *canvas;
    QComboBox *keyChoice;
    QListWidget *list;
    QComboBox *modeChoice;
    QSpinBox *interval;
};
