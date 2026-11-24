#pragma once

#include "keymap.h"
#include <QWidget>

class QTimer;

class KeymapOverlay : public QWidget {
public:
    explicit KeymapOverlay(QWidget *viewport, QWidget *mainWindow);

    void setProfile(const KeymapProfile &profile);
    void setHintsVisible(bool visible);
    bool hintsVisible() const { return showHints; }
    void setAimCrosshairActive(bool active);
    void refreshSync();

protected:
    void showEvent(QShowEvent *event) override;
    void paintEvent(QPaintEvent *event) override;

private:
    void syncGeometryAndVisibility();

    QWidget *targetViewport = nullptr;
    QWidget *ownerWindow = nullptr;
    KeymapProfile currentProfile;
    bool showHints = false;
    bool crosshairActive = false;
    QTimer *syncTimer = nullptr;
};
